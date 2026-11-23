//! Data packs in the game: functions (`/function`, `/return`, tick and load
//! tags, `/schedule`), predicates, `/reload` and `/datapack`.
#[path = "functions/loading.rs"]
mod loading;
#[path = "functions/registry_graph.rs"]
mod registry_graph;
use super::{
    Game,
    commands::{
        R, Target, err,
        execute::{ExecState, Source},
    },
};
use myriade_engine::{
    command::{Arg, Context},
    datapack::{self, DataPack, DataStore, Function},
    save,
    scoreboard::bracketed,
    text::{self, Text},
};
use horizon_core::nbt::Tag;
use std::sync::Arc;

/// A failed reload stage: the next generation's data could not be built, or
/// a preparation step returned its error.
#[derive(Debug)]
pub(super) enum ReloadStageError {
    DataSource(String),
    Failed(Box<dyn std::error::Error>),
}

impl std::fmt::Display for ReloadStageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DataSource(error) => f.write_str(error),
            Self::Failed(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ReloadStageError {}

impl From<Box<dyn std::error::Error>> for ReloadStageError {
    fn from(error: Box<dyn std::error::Error>) -> Self {
        Self::Failed(error)
    }
}

/// A `TimerQueue` event.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Scheduled {
    /// `ns:function`, or `#ns:tag` for a function tag.
    pub id: String,
    pub time: i64,
    pub seq: u64,
}
#[derive(Default)]
pub(super) struct FnState {
    pub store: DataStore,
    packs: Vec<Arc<DataPack>>,
    /// Enabled pack ids, in load order (later overrides earlier).
    enabled: Vec<String>,
    loaded: bool,
    /// Run the `minecraft:load` tag on the next tick.
    load_pending: bool,
    pub schedule: Vec<Scheduled>,
    /// The world's saved queue has been read once: it is the live
    /// `TimerQueue` `SavedData`, so a later reload keeps it as it is.
    schedule_loaded: bool,
    next_seq: u64,
    /// A dedicated server's `function-permission-level`.
    pub permission: Option<u8>,
}
/// What a function reported through `return`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    NoReturn,
    Value(i32),
    Fail,
}

#[derive(Clone)]
struct FunctionCall {
    functions: Vec<Function>,
    args: Option<Tag>,
}
/// One owned call for the still-live parsed invocation, not a generic queue.
/// The context address is compared only, never dereferenced or persisted.
pub(in crate::game) struct PreparedFunctionCall {
    invocation: usize,
    source: Source,
    call: std::sync::Arc<FunctionCall>,
}
impl PreparedFunctionCall {
    fn matches(&self, ctx: &Context, source: Source) -> bool {
        let s = self.source;
        self.invocation == ctx as *const Context as usize
            && s.entity == source.entity
            && s.pos.map(f64::to_bits) == source.pos.map(f64::to_bits)
            && s.rot.map(f32::to_bits) == source.rot.map(f32::to_bits)
            && s.eyes == source.eyes
            && s.silent == source.silent
            && s.store == source.store
            && s.dimension == source.dimension
            && s.level == source.level
            && s.block == source.block
            && s.cart == source.cart
    }
}

/// The world's pack stack: the built-in pack (unless disabled), the packs
/// of `<world>/datapacks` and the resources root's feature packs, with the
/// enabled ids from `datapacks.json` (`PackRepository.reload`).
pub(super) struct PackStack {
    pub builtin: Option<DataPack>,
    pub(super) content: DataPack,
    pub packs: Vec<Arc<DataPack>>,
    pub enabled: Vec<String>,
    /// The predicate tags `PackStack::validate` bound (loading.rs).
    tags: std::sync::OnceLock<registry_graph::BoundTags>,
}
impl PackStack {
    /// The enabled packs in `enabled` (the selected) order, later overriding
    /// earlier: `PackRepository.openAllSelected` keeps the selected order.
    pub(super) fn enabled_packs(&self) -> impl Iterator<Item = &DataPack> {
        self.enabled
            .iter()
            .filter_map(|id| self.packs.iter().find(|p| &p.id == id).map(Arc::as_ref))
    }
}
pub(super) fn pack_stack(
    world_root: &std::path::Path,
    resources: &std::path::Path,
    builtin_source: &horizon_assets::resource_source::ResourceSource,
    content_source: &horizon_assets::resource_source::ResourceSource,
    vanilla_disabled: bool,
    enabled_features: &[String],
    additional: &[Arc<DataPack>],
) -> PackStack {
    let builtin = (!vanilla_disabled)
        .then(|| DataPack::from_source("vanilla".into(), builtin_source.clone()));
    let mut packs = myriade_level::datapack::discover_with_features(world_root, resources);
    packs.extend(additional.iter().map(|pack| (**pack).clone()));
    let enabled = datapack::enabled_ids(world_root, &packs, enabled_features);
    PackStack {
        content: DataPack::from_source("horizon-content".into(), content_source.clone()),
        builtin,
        packs: packs.into_iter().map(Arc::new).collect(),
        enabled,
        tags: Default::default(),
    }
}

/// The world's enabled packs' content files (`recipe/`, `loot_table/`,
/// `advancement/`, `tags/…`) for the tables to merge over the resource
/// root's own. Empty when the world has no packs.
pub(super) fn content_overlay(stack: &PackStack) -> Vec<(String, Vec<Vec<u8>>)> {
    let packs: Vec<&DataPack> = std::iter::once(&stack.content)
        .chain(stack.enabled_packs())
        .collect();
    if packs.is_empty() {
        return Vec::new();
    }
    DataStore::load(&packs).files.into_iter().collect()
}

/// The files the content tables load from: the world's enabled packs'
/// (`content_overlay`) with what native mods contribute for this load
/// (`native_data_pack_files`) below them. `PackOverlay::top` is a path's last
/// file, so a pack's file still overrides a mod's for the same path, and a
/// later contribution overrides an earlier one, as packs do. `Game` builds
/// its tables through here on a world's first open and on `/reload`, so the
/// mods are asked once per load.
pub(super) fn overlay_files(
    stack: &PackStack,
    contributed: Vec<(String, Vec<u8>)>,
) -> Vec<(String, Vec<Vec<u8>>)> {
    if contributed.is_empty() {
        return content_overlay(stack);
    }
    // Passive content replaces prepared data, native contributions sit over
    // it, and the world's selected packs retain the highest priority.
    let mut files = DataStore::load(&[&stack.content]).files;
    for (path, bytes) in contributed {
        if let Err(e) = serde_json::from_slice::<Box<serde_json::value::RawValue>>(&bytes) {
            log::warn!("Horizon: a native mod's data pack file {path} is not JSON, skipped: {e}");
            continue;
        }
        files.entry(path).or_default().push(bytes);
    }
    let packs = stack.enabled_packs().collect::<Vec<_>>();
    for (path, versions) in DataStore::load(&packs).files {
        files.entry(path).or_default().extend(versions);
    }
    files.into_iter().collect()
}

impl FnState {
    /// The enabled world packs in resource priority order, highest first: a
    /// later selected pack overrides an earlier one (`PackRepository` order).
    pub(in crate::game) fn pack_sources(
        &self,
    ) -> impl DoubleEndedIterator<Item = &horizon_assets::resource_source::ResourceSource> {
        self.enabled
            .iter()
            .rev()
            .filter_map(|id| self.packs.iter().find(|pack| &pack.id == id))
            .map(|pack| pack.source())
    }

    /// The highest enabled world pack providing this data resource.
    pub(super) fn data_resource(
        &self,
        name: &str,
    ) -> Option<horizon_assets::resource_source::Resource> {
        self.pack_sources()
            .find(|source| source.contains(name))
            .map(|source| {
                horizon_assets::resource_source::Resource::file(source.clone(), name.to_owned())
            })
    }

    /// Enabled pack ids, in load order (later overrides earlier).
    #[cfg(test)]
    pub(super) fn enabled(&self) -> &[String] {
        &self.enabled
    }
    /// The level function files compile at (`ServerFunctionLibrary`'s
    /// `functionCompilationPermissions`): `function-permission-level` on a
    /// dedicated server, game master (2) otherwise. Whoever runs a function,
    /// its commands need no more than this.
    fn command_level(&self) -> u8 {
        self.permission.unwrap_or(2)
    }
}

impl Game<'_> {
    // ---- loading -----------------------------------------------------------

    fn datapack_settings_path(&self) -> std::path::PathBuf {
        self.world_root.join("datapacks.json")
    }
    /// Discover and load the data packs of the world (once, or on `/reload`).
    pub(super) fn ensure_datapacks(&mut self) {
        if self.fns.loaded || self.world_root.as_os_str().is_empty() {
            return;
        }
        let _ = self.reload_datapacks(false, &self.level.enabled_features.clone());
    }
    pub(super) fn open_world_datapacks(
        &mut self,
        enabled_features: &[String],
    ) -> Result<PackStack, ReloadStageError> {
        // Function admission parses commands immediately, including block arguments.
        // Populate static registries on first open; admission synchronizes pack IDs.
        self.build_registries();
        self.reload_datapacks(false, enabled_features)
    }

    pub(super) fn reload_datapacks(
        &mut self,
        run_load: bool,
        enabled_features: &[String],
    ) -> Result<PackStack, ReloadStageError> {
        let resources = self.options.resources.clone();
        let stack = pack_stack(
            &self.world_root,
            &resources,
            &self.pack.data_source,
            &self.pack.data_changes,
            self.cmds.vanilla_disabled,
            enabled_features,
            &[],
        );
        // `ReloadableServerRegistries.reload`: the candidate's registry references bind and
        // validate before anything is published, so a refusal keeps the live library, tables
        // and timer queue.
        if let Err(error) = stack.validate() {
            if self.fns.loaded {
                log::error!("Failed to execute reload");
            } else {
                log::warn!(
                    "Failed to load datapacks, can't proceed with server load. You can either fix your datapacks or reset to vanilla with --safeMode"
                );
            }
            return Err(error);
        }
        let prepared = self.prepare_server_reload(&stack)?;
        let context = self.prepare_loop_context();
        let world = if run_load {
            Some(
                self.prepare_world_reload(&stack, &resources)
                    .map_err(|error| ReloadStageError::DataSource(error.to_string()))?,
            )
        } else {
            None
        };
        self.install_loop_context(context);
        if !run_load {
            self.fns = Default::default();
        }
        self.fns.packs = stack.packs.clone();
        self.fns.enabled = stack.enabled.clone();
        self.fns.loaded = true;
        self.fns.load_pending = true;
        // An explicit reload (`/reload`, `/datapack`) also rebuilds the
        // content tables, as `ReloadableServerResources` does; on world open
        // `finish_open_world` builds them right after this.
        if let Some(world) = world {
            self.install_world_tables(world);
        }
        self.apply_server_reload(prepared, &stack);
        self.run_native_advancement_grants();
        // The schedule is the world's live `TimerQueue` `SavedData`, not pack
        // content: firing an event (`TimerQueue.tick`) and `/schedule clear`
        // (`TimerQueue.remove`) drop it from the queue, and a reload only
        // replaces the function library. Read the saved file when the world's
        // data loads, then keep the queue as it is.
        if !self.fns.schedule_loaded {
            self.fns.schedule_loaded = true;
            self.load_schedule();
        }
        Ok(stack)
    }
    /// `CommandFunction.parseCommand`: compile ordinary lines without running them.
    /// Macro lines are parsed
    /// only after substitution, but ordinary lines in a macro file still
    /// have to compile (`CommandFunction.fromLines`).
    pub(super) fn admit_store(&mut self, mut store: DataStore, stack: &PackStack) {
        // The predicate evaluator reads a `#tag` holder list as the bound tag's entries.
        if let Ok(tags) = stack.validate() {
            tags.expand_predicates(&mut store.predicates);
        }
        // On explicit reload the actual content tables have already refreshed.
        // The candidate's named predicates/modifiers must also be available to
        // argument codecs before any of its functions can be exposed.
        self.sync_enchantment_registries();
        // A function's `loot ... loot <id>` names the reloaded packs' tables.
        self.sync_loot_table_ids();
        // `ReloadableServerRegistries.reload` binds the packs' tags before
        // `ServerFunctionLibrary` compiles its functions: an item or block
        // argument's `#tag` resolves against them.
        self.sync_pack_tag_ids();
        let mut registries = self.session.registries.clone();
        registries.permission = self.fns.command_level();
        registries.functions = store.functions.keys().cloned().collect();
        registries.function_tags = store.function_tags.keys().cloned().collect();
        registries.predicates = store.predicates.keys().cloned().collect();
        registries.item_modifiers = store.item_modifiers.keys().cloned().collect();
        // On world open the content tables get the world's packs only after this: the
        // candidate's own loot tables are named here, as its predicates and modifiers are.
        let pack_tables = store.files.keys().filter_map(|id| id.strip_prefix("loot_table/"));
        registries.loot_tables.extend(pack_tables.map(str::to_owned));
        registries.loot_tables.sort();
        registries.loot_tables.dedup();
        let sources = registries
            .by_registry
            .entry("minecraft:slot_source".into())
            .or_default();
        sources.ids = store.slot_sources.keys().cloned().collect();
        sources.tags = store
            .slot_source_tags
            .keys()
            .filter(|id| store.slot_source_tag(id).is_ok())
            .cloned()
            .collect();
        for (id, ids) in [
            ("minecraft:predicate", &registries.predicates),
            ("minecraft:item_modifier", &registries.item_modifiers),
        ] {
            if let Some(entry) = registries.by_registry.get_mut(id) {
                entry.ids.clone_from(ids);
            }
        }
        let dispatcher = self.function_dispatcher();
        store.functions.retain(|id, function| {
            for line in &function.lines {
                if let datapack::FunctionLine::Command(line) = line
                    && let Err(error) = dispatcher.parse(line, &registries).executable()
                {
                    store.errors.push((
                        id.clone(),
                        format!(
                            "Invalid command {line:?}: {} at position {}",
                            error.message.plain(),
                            error.java_cursor()
                        ),
                    ));
                    return false;
                }
            }
            true
        });
        for (id, e) in &store.errors {
            log::warn!("Horizon: failed to load function {id}: {e}");
        }
        self.fns.store = store;
        self.sync_function_registries(stack);
    }
    /// The item and block tags the world's packs define, as `TagLoader` binds
    /// them into the registries before the functions compile: the item and
    /// block arguments' `#tag` and the completions read them. Recomputed from
    /// the content tables, so a tag a pack no longer defines leaves again.
    pub(in crate::game) fn sync_pack_tag_ids(&mut self) {
        let tags = |ids: &std::collections::HashMap<String, std::collections::HashSet<String>>| {
            let mut out: Vec<String> = ids
                .keys()
                .map(|k| myriade_engine::command::args::normalize(k))
                .collect();
            out.sort();
            out
        };
        let item_tags = tags(&self.items().item_tags);
        let block_tags = tags(&self.items().block_tags);
        let reg = &mut self.session.registries;
        reg.item_tags.clone_from(&item_tags);
        reg.block_tags.clone_from(&block_tags);
        // `Registries::tags` reads an exported registry's entry first. Only
        // update one the export made: a new empty entry would also take over
        // `Registries::ids` and hide every item id.
        for (registry, tags) in [
            ("minecraft:item", item_tags),
            ("minecraft:block", block_tags),
        ] {
            if let Some(entry) = reg.by_registry.get_mut(registry) {
                entry.tags = tags;
            }
        }
    }
    fn save_datapack_settings(&self) {
        let disabled: Vec<&String> = self
            .fns
            .packs
            .iter()
            .filter(|p| !self.fns.enabled.contains(&p.id))
            .map(|p| &p.id)
            .collect();
        let json = serde_json::json!({"enabled": self.fns.enabled, "disabled": disabled});
        let _ = std::fs::write(self.datapack_settings_path(), json.to_string());
    }
    pub(super) fn sync_function_registries(&mut self, stack: &PackStack) {
        self.sync_schedule_registry();
        self.sync_enchantment_registries();
        let reg = &mut self.session.registries;
        reg.functions = self.fns.store.functions.keys().cloned().collect();
        reg.function_tags = self.fns.store.function_tags.keys().cloned().collect();
        reg.predicates = self.fns.store.predicates.keys().cloned().collect();
        reg.item_modifiers = self.fns.store.item_modifiers.keys().cloned().collect();
        let sources = reg
            .by_registry
            .entry("minecraft:slot_source".into())
            .or_default();
        sources.ids = self.fns.store.slot_sources.keys().cloned().collect();
        sources.tags = self
            .fns
            .store
            .slot_source_tags
            .keys()
            .filter(|id| self.fns.store.slot_source_tag(id).is_ok())
            .cloned()
            .collect();
        reg.schedules = self.fns.schedule.iter().map(|s| s.id.clone()).collect();

        reg.datapacks_enabled = std::iter::once("vanilla".to_owned())
            .chain(self.fns.enabled.iter().cloned())
            .collect();
        reg.datapacks_disabled = self
            .fns
            .packs
            .iter()
            .filter(|p| !self.fns.enabled.contains(&p.id))
            .map(|p| p.id.clone())
            .collect();
        // A pack's worldgen ids join the completion registries
        // (`command-registries.json` lists only the game's own), and so do
        // its worldgen tag ids for the `#tag` completions. The packs are the
        // ones the world's worldgen is built from.
        let enabled: Vec<_> = stack.enabled_packs().collect();
        let (overrides, _) = myriade_engine::datapack::worldgen_overrides(&enabled, |_| true);
        for (bundle, registry) in [
            ("biomes", "minecraft:worldgen/biome"),
            ("structures", "minecraft:worldgen/structure"),
            ("structure_sets", "minecraft:worldgen/structure_set"),
            ("template_pools", "minecraft:worldgen/template_pool"),
            ("processor_lists", "minecraft:worldgen/processor_list"),
        ] {
            let entry = reg.by_registry.entry(registry.to_owned()).or_default();
            for (id, _) in overrides.entries(bundle) {
                if !entry.ids.contains(&id.to_owned()) {
                    entry.ids.push(id.to_owned());
                }
            }
            entry.ids.sort();
        }
        let entry = reg
            .by_registry
            .entry("minecraft:worldgen/biome".to_owned())
            .or_default();
        for (id, _, _) in overrides.tags("biome_tags") {
            if !entry.tags.contains(&id.to_owned()) {
                entry.tags.push(id.to_owned());
            }
        }
        entry.tags.sort();
        let entry = reg
            .by_registry
            .entry("minecraft:worldgen/structure".to_owned())
            .or_default();
        for (id, _, _) in overrides.tags("structure_tags") {
            if !entry.tags.contains(&id.to_owned()) {
                entry.tags.push(id.to_owned());
            }
        }
        entry.tags.sort();
    }

    fn sync_schedule_registry(&mut self) {
        self.session.registries.schedules = self.fns.schedule.iter().map(|s| s.id.clone()).collect();
    }

    // ---- scheduled events ------------------------------------------------------

    fn schedule_path(&self) -> std::path::PathBuf {
        self.world_root.join("data/minecraft/scheduled_events.dat")
    }
    fn load_schedule(&mut self) {
        let Some(root) = save::read_dat(&self.schedule_path()) else {
            return;
        };
        let Some(Tag::List(events)) = root.get("data").and_then(|d| d.get("events")) else {
            return;
        };
        for e in events {
            let (Some(Tag::Long(time)), Some(Tag::String(id)), Some(cb)) =
                (e.get("trigger_time"), e.get("id"), e.get("callback"))
            else {
                continue;
            };
            let _ = cb;
            self.fns.next_seq += 1;
            self.fns.schedule.push(Scheduled {
                id: id.to_string_lossy(),
                time: *time,
                seq: self.fns.next_seq,
            });
        }
    }
    pub(super) fn save_schedule(&self) {
        if !self.fns.loaded {
            return;
        }
        let mut events = self.fns.schedule.clone();
        events.sort_by_key(|e| (e.time, e.seq));
        let list = events
            .iter()
            .map(|e| {
                let (kind, name) = match e.id.strip_prefix('#') {
                    Some(tag) => ("minecraft:function_tag", tag),
                    None => ("minecraft:function", e.id.as_str()),
                };
                Tag::compound([
                    ("trigger_time", Tag::Long(e.time)),
                    ("id", Tag::String(e.id.as_str().into())),
                    (
                        "callback",
                        Tag::compound([
                            ("type", Tag::String(kind.into())),
                            ("id", Tag::String(name.into())),
                        ]),
                    ),
                ])
            })
            .collect();
        if events.is_empty() && !self.schedule_path().exists() {
            return;
        }
        let _ = save::write_dat(
            &self.schedule_path(),
            Tag::compound([
                ("data", Tag::compound([("events", Tag::List(list))])),
                ("DataVersion", Tag::Int(horizon_world::DATA_VERSION)),
            ]),
        );
    }

    // ---- ticking -----------------------------------------------------------------

    /// `ServerFunctionManager.tick`: the `#minecraft:load` functions after a
    /// reload, then the `#minecraft:tick` ones, before the levels tick (and
    /// before the game time moves on).
    pub(super) fn tick_functions(&mut self) {
        self.exec = ExecState::default();
        let source = self.server_source();
        if self.fns.load_pending {
            self.fns.load_pending = false;
            for f in self.fns.store.tag_functions("minecraft:load") {
                self.run_function_by_id(&f, None, source);
            }
        }
        for f in self.fns.store.tag_functions("minecraft:tick") {
            self.run_function_by_id(&f, None, source);
        }
    }
    /// `ServerLevel.tickTime`'s `getScheduledEvents().tick`: the scheduled
    /// functions due at the game time this tick has just reached.
    pub(super) fn run_scheduled_functions(&mut self) {
        self.ensure_datapacks();
        self.exec = ExecState::default();
        let source = self.server_source();
        let now = self.world.game_time;
        loop {
            let due = self
                .fns
                .schedule
                .iter()
                .enumerate()
                .filter(|(_, e)| e.time <= now)
                .min_by_key(|(_, e)| (e.time, e.seq))
                .map(|(i, _)| i);
            let Some(i) = due else { break };
            let event = self.fns.schedule.remove(i);
            match event.id.strip_prefix('#') {
                Some(tag) => {
                    for f in self.fns.store.tag_functions(tag) {
                        self.run_function_by_id(&f, None, source);
                    }
                }
                None => self.run_function_by_id(&event.id, None, source),
            }
        }
    }
    fn run_function_by_id(&mut self, id: &str, args: Option<&Tag>, source: Source) {
        self.exec.commands = 0;
        if let Some(f) = self.fns.store.functions.get(id).cloned() {
            // `ServerFunctionManager.execute` logs a function that failed to
            // instantiate or run; its lines' own failures stay silent.
            if let Err(e) = self.run_function(&f, args, source) {
                log::warn!(
                    "Horizon: failed to run function {id}: {}",
                    e.message.plain()
                );
            }
        }
    }

    /// The function branch of advancement rewards: resolve the optional function
    /// and execute it as the rewarded player, with suppressed output and GAMEMASTER permission.
    pub(super) fn run_advancement_reward_function(&mut self, id: &str) {
        self.ensure_datapacks();
        let Some(function) = self.fns.store.functions.get(id).cloned() else {
            return;
        };
        let Some(pos) = self.target_position(Target::Player) else {
            return;
        };
        // createCommandSourceStack belongs to the player, not an enclosing
        // execute's position, rotation, output sink or store callback.
        let source = Source {
            pos,
            rot: self.target_rotation(Target::Player),
            silent: true,
            ..self.player_source()
        };
        // The existing runner applies this level to nested function calls too.
        let permission = self
            .fns
            .permission
            .replace(myriade_engine::command::vanilla::GAMEMASTERS);
        let callback = self.exec.callback.take();
        let result = self.run_function(&function, None, source);
        self.exec.callback = callback;
        self.fns.permission = permission;
        if let Err(error) = result {
            log::warn!(
                "Horizon: failed to run advancement reward function {id}: {}",
                error.message.plain()
            );
        }
    }

    // ---- running -----------------------------------------------------------------

    /// Run one function with `source`: its commands in order, until a `return`.
    pub(in crate::game) fn run_function(
        &mut self,
        f: &Function,
        args: Option<&Tag>,
        source: Source,
    ) -> R<Outcome> {
        let failure = |key: &str, a: &[String]| {
            let args: Vec<&dyn std::fmt::Display> =
                a.iter().map(|s| s as &dyn std::fmt::Display).collect();
            err(
                "commands.function.instantiationFailure",
                &[&f.id, &myriade_engine::text::Text::tr(key, &args).plain()],
            )
        };
        let lines = f.instantiate(args).map_err(|(key, a)| failure(&key, &a))?;
        if let Err((key, a)) = self.parse_macro_lines(f, &lines) {
            return Err(failure(&key, &a));
        }
        if self.exec.functions.len() >= 128 {
            return Ok(Outcome::NoReturn);
        }
        self.exec.functions.push(f.id.clone());
        let outer = self.exec.returned.take();
        let level = std::mem::replace(&mut self.session.registries.permission, self.fns.command_level());
        let inner = Source {
            silent: true,
            store: None,
            ..source
        };
        for line in lines {
            let _ = self.execute_line(&line, inner);
            if self.exec.returned.is_some() {
                break;
            }
            if self.exec.commands
                > self
                    .session
                    .rules
                    .int_at(myriade_engine::gamerules::vanilla::MAX_COMMAND_SEQUENCE_LENGTH)
                    .max(1) as u32
            {
                break;
            }
        }
        self.session.registries.permission = level;
        let returned = std::mem::replace(&mut self.exec.returned, outer);
        self.exec.functions.pop();
        Ok(match returned {
            None => Outcome::NoReturn,
            Some(Some(v)) => Outcome::Value(v),
            Some(None) => Outcome::Fail,
        })
    }
    /// `MacroFunction.MacroEntry.instantiate`: a `$` line is parsed only after
    /// substitution, at the function compilation permission
    /// (`CommandFunction.fromLines`' `compilationContext`), and a line that
    /// does not parse fails the whole call before any of its lines run.
    fn parse_macro_lines(
        &mut self,
        f: &Function,
        lines: &[String],
    ) -> Result<(), (String, Vec<String>)> {
        if !f.is_macro() {
            return Ok(());
        }
        let level = std::mem::replace(&mut self.session.registries.permission, self.fns.command_level());
        let d = self.function_dispatcher();
        let mut out = Ok(());
        for (line, entry) in lines.iter().zip(&f.lines) {
            if matches!(entry, datapack::FunctionLine::Macro(_))
                && let Err(error) = d.parse(line, &self.session.registries).executable()
            {
                out = Err((
                    "commands.function.error.parse".into(),
                    vec![f.id.clone(), line.clone(), error.message.plain()],
                ));
                break;
            }
        }
        self.session.registries.permission = level;
        out
    }
    /// The tree every function line is checked against, a plain line at load and a macro line
    /// after substitution: the session's (live rules, enabled features and the mods' commands,
    /// registered before functions load), as the server's own dispatcher compiles functions.
    fn function_dispatcher(&self) -> Arc<myriade_engine::command::Dispatcher> {
        self.session.dispatcher.0.clone()
    }
    /// The functions a `FunctionArgument` names: a function or a tag.
    pub(in crate::game) fn functions_named(&mut self, name: &str) -> R<Vec<Function>> {
        match name.strip_prefix('#') {
            // ServerFunctionLibrary.getTag: an unknown tag is empty.
            Some(tag) => {
                if !self.fns.store.function_tags.contains_key(tag) {
                    return Ok(Vec::new());
                }
                Ok(self
                    .fns
                    .store
                    .tag_functions(tag)
                    .iter()
                    .filter_map(|id| self.fns.store.functions.get(id).cloned())
                    .collect())
            }
            None => self
                .fns
                .store
                .functions
                .get(name)
                .cloned()
                .map(|f| vec![f])
                .ok_or_else(|| err("arguments.function.unknown", &[&name])),
        }
    }
    /// `if function`: what each function returns (None if it did not).
    pub(in crate::game) fn call_function_ref(
        &mut self,
        name: &str,
        args: Option<&Tag>,
    ) -> R<Vec<Option<i32>>> {
        let functions = self.functions_named(name)?;
        let source = self.command_source.unwrap_or_else(|| self.player_source());
        let mut out = Vec::new();
        for f in functions {
            out.push(match self.run_function(&f, args, source)? {
                Outcome::NoReturn => None,
                Outcome::Value(v) => Some(v),
                Outcome::Fail => Some(0),
            });
        }
        Ok(out)
    }

    // ---- commands ------------------------------------------------------------------

    /// `FunctionCommand.decorateOutputIfNeeded`: consume only this invocation's
    /// prepared call before its body can invoke another function.
    pub(super) fn cmd_function(&mut self, ctx: &Context) -> R<i32> {
        let source = self.command_source.unwrap_or_else(|| self.player_source());
        let call = if self
            .exec
            .prepared_function
            .as_ref()
            .is_some_and(|p| p.matches(ctx, source))
        {
            std::sync::Arc::unwrap_or_clone(self.exec.prepared_function.take().unwrap().call)
        } else {
            let Some(call) = self.prepare_function_call(ctx)? else {
                return Ok(0);
            };
            self.start_function_call(&call.functions);
            call
        };
        self.execute_function_call(call)
    }
    /// `FunctionCommand.getArgumentTag`: retain the existing resolution and argument validation.
    fn prepare_function_call(&mut self, ctx: &Context) -> R<Option<FunctionCall>> {
        let Some(Arg::Resource(name)) = ctx.get("name").cloned() else {
            return Ok(None);
        };
        let functions = self.functions_named(&name)?;
        if functions.is_empty() {
            let id = name.trim_start_matches('#');
            return Err(err("commands.function.scheduled.no_functions", &[&id]));
        }
        let args = if ctx.has("with") {
            let r = self.data_ref_named(ctx, 0, "source")?;
            let data = self.data_of(&r)?;
            let tag = match ctx.get("path") {
                Some(Arg::Str(p)) => {
                    let path = myriade_engine::nbt_path::NbtPath::parse(p)
                        .map_err(myriade_engine::data_cmd::path_error)?;
                    myriade_engine::data_cmd::single_tag(&path, &data)?
                }
                _ => data,
            };
            if !matches!(tag, Tag::Compound(_)) {
                // FunctionCommand.getArgumentTag uses TagType.getName, not the tag's value.
                let names = [
                    "END", "BYTE", "SHORT", "INT", "LONG", "FLOAT", "DOUBLE", "BYTE[]", "STRING",
                    "LIST", "COMPOUND", "INT[]", "LONG[]",
                ];
                return Err(err(
                    "commands.function.error.argument_not_compound",
                    &[&names[usize::from(tag.id())]],
                ));
            }
            Some(tag)
        } else if let Some(Arg::Str(text)) = ctx.get("arguments") {
            Some(
                horizon_core::nbt::parse_snbt(text)
                    .map_err(|e| err("argument.nbt.invalid", &[&e]))?
                    .0,
            )
        } else {
            None
        };
        Ok(Some(FunctionCall { functions, args }))
    }
    /// `FunctionCommand.FunctionCustomExecutor.runGuarded`: send the existing start reply.
    fn start_function_call(&mut self, functions: &[Function]) {
        let message = if functions.len() == 1 {
            Text::translate(
                "commands.function.scheduled.single",
                &[Text::literal(functions[0].id.clone())],
            )
        } else {
            Text::translate(
                "commands.function.scheduled.multiple",
                &[myriade_engine::scoreboard_cmd::format_list(
                    functions
                        .iter()
                        .map(|f| Text::literal(f.id.clone()))
                        .collect(),
                )],
            )
        };
        self.feedback(message);
    }
    /// `FunctionCommand.decorateOutputIfNeeded`: retain body, result and callback delivery.
    fn execute_function_call(&mut self, call: FunctionCall) -> R<i32> {
        let FunctionCall { functions, args } = call;
        let source = self.command_source.unwrap_or_else(|| self.player_source());
        let mut sum: i32 = 0;
        let mut any = false;
        let mut failed = false;
        for f in functions {
            match self.run_function(&f, args.as_ref(), source)? {
                Outcome::NoReturn => {}
                Outcome::Value(v) => {
                    any = true;
                    sum = sum.wrapping_add(v);
                    let message = Text::translate(
                        "commands.function.result",
                        &[Text::literal(f.id.clone()), Text::literal(v.to_string())],
                    );
                    self.feedback(message);
                }
                Outcome::Fail => {
                    any = true;
                    failed = true;
                    self.feedback(Text::translate(
                        "commands.function.result",
                        &[Text::literal(f.id.clone()), Text::literal("0")],
                    ));
                }
            }
        }
        self.exec.callback = Some(if failed && sum == 0 {
            Some((false, 0))
        } else if any {
            Some((true, sum))
        } else {
            None
        });
        Ok(sum)
    }
    /// `FunctionCommand.queueFunctionsNoReturn`: starts are emitted while the
    /// eligible direct plain-function calls are prepared, before their bodies.
    pub(in crate::game) fn prepare_function_batch(
        &mut self,
        ctx: &Context,
        sources: &[Source],
    ) -> Option<Vec<PreparedFunctionCall>> {
        if !self.fns.loaded
            || sources.len() < 2
            || !self.exec.functions.is_empty()
            || self.exec.prepared_function.is_some()
            || sources.iter().any(|s| s.silent)
            || ctx.has("with")
            || ctx.get("arguments").is_some()
        {
            return None;
        }
        let Some(Arg::Resource(name)) = ctx.get("name") else {
            return None;
        };
        if name.starts_with('#') || self.fns.store.functions.get(name)?.is_macro() {
            return None;
        }
        let limit = self
            .session
            .rules
            .int_at(myriade_engine::gamerules::vanilla::MAX_COMMAND_SEQUENCE_LENGTH)
            .max(1) as u32;
        if sources.len() > limit.saturating_sub(self.exec.commands) as usize {
            return None;
        }
        // No start or slot is published until the existing validation succeeds.
        let call = self
            .with_source(sources[0], |g| g.prepare_function_call(ctx))
            .ok()??;
        let call = std::sync::Arc::new(call);
        let tasks: Vec<_> = sources
            .iter()
            .map(|&source| PreparedFunctionCall {
                invocation: ctx as *const Context as usize,
                source,
                call: call.clone(),
            })
            .collect();
        for task in &tasks {
            self.with_source(task.source, |g| g.start_function_call(&task.call.functions));
        }
        Some(tasks)
    }
    /// `return <value>` and `return fail` (`return run` is a chain).
    pub(super) fn cmd_return(&mut self, id: &str, ctx: &Context) -> R<i32> {
        if id == "return.fail" {
            self.finish_return(None);
            self.exec.callback = Some(Some((false, 0)));
            return Ok(0);
        }
        let value = match ctx.get("value") {
            Some(Arg::Int(v)) => *v,
            _ => 0,
        };
        self.finish_return(Some(value));
        Ok(value)
    }
    pub(super) fn cmd_schedule(&mut self, id: &str, ctx: &Context) -> R<i32> {
        if id == "schedule.clear" {
            let Some(Arg::Resource(name)) = ctx.get("function").cloned() else {
                return Ok(0);
            };
            let before = self.fns.schedule.len();
            self.fns.schedule.retain(|e| e.id != name);
            let count = before - self.fns.schedule.len();
            if count == 0 {
                return Err(err("commands.schedule.cleared.failure", &[&name]));
            }
            self.feedback(Text::translate(
                "commands.schedule.cleared.success",
                &[Text::literal(count.to_string()), Text::literal(name)],
            ));
            self.sync_schedule_registry();
            return Ok(count as i32);
        }
        let Some(Arg::Resource(name)) = ctx.get("function").cloned() else {
            return Ok(0);
        };
        let time = match ctx.get("time") {
            Some(Arg::Int(t)) => *t,
            _ => 0,
        };
        if time == 0 {
            return Err(err("commands.schedule.same_tick", &[]));
        }
        let replace = !ctx.has("append");
        let tick_time = self.world.game_time + i64::from(time);
        let schedule_id = match name.strip_prefix('#') {
            // An unknown tag is empty, and scheduling it is allowed.
            Some(_) => name.clone(),
            None => {
                let Some(f) = self.fns.store.functions.get(&name) else {
                    return Err(err("arguments.function.unknown", &[&name]));
                };
                if f.is_macro() {
                    return Err(err("commands.schedule.macro", &[]));
                }
                name.clone()
            }
        };
        if replace {
            self.fns.schedule.retain(|e| e.id != schedule_id);
        }
        if !self
            .fns
            .schedule
            .iter()
            .any(|e| e.id == schedule_id && e.time == tick_time)
        {
            self.fns.next_seq += 1;
            let seq = self.fns.next_seq;
            self.fns.schedule.push(Scheduled {
                id: schedule_id,
                time: tick_time,
                seq,
            });
        }
        let key = if name.starts_with('#') {
            "commands.schedule.created.tag"
        } else {
            "commands.schedule.created.function"
        };
        self.feedback(Text::translate(
            key,
            &[
                Text::literal(name.trim_start_matches('#').to_owned()),
                Text::literal(time.to_string()),
                Text::literal(tick_time.to_string()),
            ],
        ));
        self.sync_schedule_registry();
        Ok((tick_time.rem_euclid(i64::from(i32::MAX))) as i32)
    }
    /// ReloadCommand: the message, then the (asynchronous) reload; returns 0.
    pub(super) fn cmd_reload(&mut self) -> R<i32> {
        self.feedback(Text::translate("commands.reload.success", &[]));
        self.reload_datapacks(true, &self.level.enabled_features.clone())
            .map_err(Self::reload_failure)?;
        Ok(0)
    }
    /// `PackLocationInfo.createChatLink`: the id decorated with its source
    /// (`pack.nameAndSource`, grey) in brackets, green when enabled.
    fn pack_link(&self, id: &str, enabled: bool) -> Text {
        let hint = if enabled { "disable" } else { "enable" };
        let style = text::Style {
            color: Some(if enabled { text::GREEN } else { text::RED }),
            click: Some(text::ClickEvent::SuggestCommand(format!(
                "/datapack {hint} \"{id}\""
            ))),
            ..text::Style::default()
        };
        let description = self
            .fns
            .packs
            .iter()
            .find(|p| p.id == id)
            .map_or_else(|| "Default data pack".to_owned(), |p| p.description());
        let style = text::Style {
            hover: Some(std::sync::Arc::new(Text::literal(format!(
                "{id}\n{description}"
            )))),
            ..style
        };
        let source = if id == "vanilla" {
            "pack.source.builtin"
        } else if self
            .fns
            .packs
            .iter()
            .any(|pack| pack.id == id && pack.is_feature())
        {
            "pack.source.feature"
        } else {
            "pack.source.world"
        };
        let name = Text::translate(
            "pack.nameAndSource",
            &[Text::literal(id), Text::translate(source, &[])],
        )
        .with_color(text::GRAY);
        bracketed(name).with_style(&style)
    }
    /// The selected packs, lowest priority first.
    fn selected_packs(&self) -> Vec<String> {
        let vanilla = (!self.cmds.vanilla_disabled).then(|| "vanilla".to_owned());
        vanilla
            .into_iter()
            .chain(self.fns.enabled.iter().cloned())
            .collect()
    }
    /// `FileUtil.isPathPartPortable`: no trailing dot and no Windows device
    /// name (CON, PRN, AUX, NUL, COM and LPT with a digit from 1 to 9).
    fn is_portable_name(name: &str) -> bool {
        let stem = name.split('.').next().unwrap_or_default();
        let device = match stem.len() {
            3 => matches!(stem, "com" | "con" | "prn" | "aux" | "nul"),
            4 => matches!(&stem[..3], "com" | "lpt") && matches!(stem.as_bytes()[3], b'1'..=b'9'),
            _ => false,
        };
        !device && !name.ends_with('.')
    }
    /// `DataPackCommand.createPack`: a pack folder with its `data` folder and a
    /// `pack.mcmeta` in the world's datapacks folder.
    fn create_pack(&mut self, ctx: &Context) -> R<i32> {
        let id = match ctx.get("id") {
            Some(Arg::Str(s)) => s.clone(),
            _ => String::new(),
        };
        let description = self.component_arg(ctx, "description")?;
        // FileUtil.isValidPathSegment: lowercase letters, digits, '-', '.' and '_'.
        let valid = !id.is_empty()
            && id != "."
            && id != ".."
            && id
                .chars()
                .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '-' | '.' | '_'));
        if !valid {
            return Err(err("commands.datapack.create.invalid_name", &[&id]));
        }
        if !Self::is_portable_name(&id) {
            return Err(err("commands.datapack.create.invalid_full_name", &[&id]));
        }
        // Without a world there is no datapacks folder to write to.
        if self.world_root.as_os_str().is_empty() {
            return Err(err("commands.datapack.create.io_failure", &[&id]));
        }
        let datapacks = self.world_root.join("datapacks");
        let pack = datapacks.join(&id);
        if pack.exists() {
            return Err(err("commands.datapack.create.already_exists", &[&id]));
        }
        // The data pack version (SharedConstants), written as a bare major
        // number for both bounds, as vanilla does.
        let pack_format = horizon_core::version::DATA_PACK_FORMAT.0;
        let mcmeta = serde_json::json!({
            "pack": {
                "description": description.component(),
                "min_format": pack_format,
                "max_format": pack_format,
            }
        });
        let written = std::fs::create_dir_all(&datapacks)
            .and_then(|()| std::fs::create_dir(&pack))
            .and_then(|()| std::fs::create_dir(pack.join("data")))
            .and_then(|()| {
                std::fs::write(
                    pack.join("pack.mcmeta"),
                    serde_json::to_string_pretty(&mcmeta).unwrap_or_default(),
                )
            });
        if written.is_err() {
            return Err(err("commands.datapack.create.io_failure", &[&id]));
        }
        self.feedback(Text::translate(
            "commands.datapack.create.success",
            &[Text::literal(id)],
        ));
        Ok(1)
    }
    fn missing_pack_features(&self, id: &str) -> Vec<String> {
        self.fns
            .packs
            .iter()
            .find(|pack| pack.id == id)
            .map(|pack| {
                pack.features()
                    .into_iter()
                    .filter(|flag| !self.level.enabled_features.contains(flag))
                    .collect()
            })
            .unwrap_or_default()
    }
    pub(super) fn cmd_datapack(&mut self, id: &str, ctx: &Context) -> R<i32> {
        let name = match ctx.get("name") {
            Some(Arg::Str(n)) => Some(n.clone()),
            _ => None,
        };
        let selected = self.selected_packs();
        let previous_enabled = self.fns.enabled.clone();
        let previous_vanilla_disabled = self.cmds.vanilla_disabled;
        // `DataPackCommand.listAvailablePacks`: a feature pack the world's
        // features do not cover is not available.
        let available: Vec<String> = std::iter::once("vanilla".to_owned())
            .chain(self.fns.packs.iter().map(|p| p.id.clone()))
            .filter(|p| !selected.contains(p))
            .filter(|p| self.missing_pack_features(p).is_empty())
            .collect();
        let known = |g: &Self, n: &str| n == "vanilla" || g.fns.packs.iter().any(|p| p.id == n);
        match id {
            "datapack.create" => self.create_pack(ctx),
            "datapack.list" => {
                let want_enabled = ctx.has("enabled") || (!ctx.has("available"));
                let want_available = ctx.has("available") || (!ctx.has("enabled"));
                let mut total = 0;
                if want_enabled {
                    if selected.is_empty() {
                        self.feedback(Text::translate("commands.datapack.list.enabled.none", &[]));
                    } else {
                        let links: Vec<Text> =
                            selected.iter().map(|p| self.pack_link(p, true)).collect();
                        total += links.len();
                        self.feedback(Text::translate(
                            "commands.datapack.list.enabled.success",
                            &[
                                Text::literal(links.len().to_string()),
                                myriade_engine::scoreboard_cmd::format_list(links),
                            ],
                        ));
                    }
                }
                if want_available {
                    if available.is_empty() {
                        self.feedback(Text::translate(
                            "commands.datapack.list.available.none",
                            &[],
                        ));
                    } else {
                        let links: Vec<Text> =
                            available.iter().map(|p| self.pack_link(p, false)).collect();
                        total += links.len();
                        self.feedback(Text::translate(
                            "commands.datapack.list.available.success",
                            &[
                                Text::literal(links.len().to_string()),
                                myriade_engine::scoreboard_cmd::format_list(links),
                            ],
                        ));
                    }
                }
                Ok(total as i32)
            }
            "datapack.enable" => {
                let name = name.unwrap_or_default();
                if !known(self, &name) {
                    return Err(err("commands.datapack.unknown", &[&name]));
                }
                if selected.contains(&name) {
                    return Err(err("commands.datapack.enable.failed", &[&name]));
                }
                let missing = self.missing_pack_features(&name);
                if !missing.is_empty() {
                    // `DataPackCommand`: the pack's requested features must
                    // be a subset of the world's enabled ones.
                    return Err(err(
                        "commands.datapack.enable.failed.no_flags",
                        &[&name, &missing.join(", ")],
                    ));
                }
                if name == "vanilla" {
                    self.cmds.vanilla_disabled = false;
                } else {
                    let existing = match ctx.get("existing") {
                        Some(Arg::Str(e)) => Some(e.clone()),
                        _ => None,
                    };
                    let position = if ctx.has("first") {
                        0
                    } else if let Some(e) = &existing {
                        let Some(i) = self.fns.enabled.iter().position(|p| p == e) else {
                            return Err(err("commands.datapack.unknown", &[e]));
                        };
                        if ctx.has("before") { i } else { i + 1 }
                    } else {
                        self.fns.enabled.len()
                    };
                    self.fns.enabled.insert(position, name.clone());
                }
                self.feedback(Text::translate(
                    "commands.datapack.modify.enable",
                    &[self.pack_link(&name, true)],
                ));
                self.apply_pack_change(previous_enabled, previous_vanilla_disabled)?;
                Ok(self.selected_packs().len() as i32)
            }
            _ => {
                let name = name.unwrap_or_default();
                if !known(self, &name) {
                    return Err(err("commands.datapack.unknown", &[&name]));
                }
                if !selected.contains(&name) {
                    return Err(err("commands.datapack.disable.failed", &[&name]));
                }
                if self
                    .fns
                    .packs
                    .iter()
                    .any(|pack| pack.id == name && pack.required)
                {
                    return Err(err("horizon.datapack.required", &[&name]));
                }
                if self
                    .fns
                    .packs
                    .iter()
                    .any(|pack| pack.id == name && pack.is_feature() && !pack.features().is_empty())
                {
                    // `DataPackCommand`'s ERROR_CANNOT_DISABLE_FEATURE: the
                    // flag stays on for the world's life.
                    return Err(err("commands.datapack.disable.failed.feature", &[&name]));
                }
                let missing = self.missing_pack_features(&name);
                if !missing.is_empty() {
                    return Err(err(
                        "commands.datapack.enable.failed.no_flags",
                        &[&name, &missing.join(", ")],
                    ));
                }
                if name == "vanilla" {
                    self.cmds.vanilla_disabled = true;
                } else {
                    self.fns.enabled.retain(|p| p != &name);
                }
                self.feedback(Text::translate(
                    "commands.datapack.modify.disable",
                    &[self.pack_link(&name, true)],
                ));
                self.apply_pack_change(previous_enabled, previous_vanilla_disabled)?;
                Ok(self.selected_packs().len() as i32)
            }
        }
    }
    /// `ReloadCommand.reloadPacks`: no message of its own. It reloads the
    /// server's data as `/reload` does (`reload_datapacks`), the content
    /// tables with the functions.
    fn apply_pack_change(
        &mut self,
        previous_enabled: Vec<String>,
        previous_vanilla_disabled: bool,
    ) -> R<()> {
        // One resolve for the store and the tables, as `/reload`'s is: the
        // timers already in memory stay, and none come back from disk
        // (`ServerFunctionManager` keeps its queue over a pack change).
        let mut stack = pack_stack(
            &self.world_root,
            &self.options.resources,
            &self.pack.data_source,
            &self.pack.data_changes,
            self.cmds.vanilla_disabled,
            &self.level.enabled_features,
            &[],
        );
        stack.enabled =
            datapack::selected_ids(stack.packs.iter().map(Arc::as_ref), &self.fns.enabled);
        if let Err(error) = stack.validate() {
            log::error!("Failed to execute reload");
            self.fns.enabled = previous_enabled;
            self.cmds.vanilla_disabled = previous_vanilla_disabled;
            return Err(Self::reload_failure(error));
        }
        let prepared = match self.prepare_server_reload(&stack) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.fns.enabled = previous_enabled;
                self.cmds.vanilla_disabled = previous_vanilla_disabled;
                return Err(Self::reload_failure(error));
            }
        };
        let root = self.options.resources.clone();
        let context = self.prepare_loop_context();
        let world = match self.prepare_world_reload(&stack, &root) {
            Ok(world) => world,
            Err(error) => {
                self.fns.enabled = previous_enabled;
                self.cmds.vanilla_disabled = previous_vanilla_disabled;
                return Err(Self::reload_failure(ReloadStageError::DataSource(
                    error.to_string(),
                )));
            }
        };
        self.install_loop_context(context);
        self.fns.packs = stack.packs.clone();
        self.fns.enabled = stack.enabled.clone();
        self.save_datapack_settings();
        self.fns.load_pending = true;
        self.install_world_tables(world);
        self.apply_server_reload(prepared, &stack);
        self.run_native_advancement_grants();
        Ok(())
    }

    /// The loop context of the item data for this side, with no mod ids.
    fn prepare_loop_context(&self) -> Arc<horizon_world::native_load_context::LoopContext> {
        let side = self.options.execution_role.loop_side();
        Arc::new(self.res.core.item_data.context().loop_context(side, []))
    }

    /// Publishes the loop context and its world generation context.
    fn install_loop_context(
        &mut self,
        context: Arc<horizon_world::native_load_context::LoopContext>,
    ) {
        let worldgen = context.worldgen_context();
        self.loop_context = context;
        horizon_world::native_worldgen_hooks::publish_context(Arc::new(worldgen));
    }

    fn reload_failure(error: impl std::fmt::Display) -> myriade_engine::command::CommandError {
        log::warn!("Horizon: {error}");
        myriade_engine::command::CommandError::new(
            Text::translate("commands.reload.failure", &[])
                .append(Text::literal(format!("\n{error}"))),
        )
    }

    // ---- predicates -------------------------------------------------------------------

    /// `ResourceOrIdArgument.getLootPredicate`: accept the inline or named
    /// predicate, then test it with the existing source entity and origin.
    pub(in crate::game) fn check_predicate(&mut self, id: &str) -> R<bool> {
        self.ensure_datapacks();
        // ResourceOrIdArgument retains either an inline value or a named id.
        // Both use the same evaluator and command-source context below.
        let json = if id.starts_with('{') {
            let (tag, _) = horizon_core::nbt::parse_snbt(id)
                .map_err(|e| err("argument.nbt.invalid", &[&e]))?;
            myriade_engine::scoreboard::tag_to_json(&tag)
        } else {
            self.fns.store.predicates.get(id).cloned().ok_or_else(|| {
                let id = if id.contains(':') {
                    id.to_string()
                } else {
                    format!("minecraft:{id}")
                };
                err(
                    "argument.resource_or_id.no_such_element",
                    &[&id, &"minecraft:predicate"],
                )
            })?
        };
        let source = self.command_source.unwrap_or_else(|| self.player_source());
        let view = source.entity.map(|t| {
            let scores = self.holder_name(t).map(|holder| {
                self.score
                    .board
                    .holder_scores(&holder)
                    .into_iter()
                    .map(|(objective, value)| (objective.name.clone(), value))
                    .collect()
            });
            let mut view = self.entity_view_asked(t, &json);
            view.scores = scores;
            view
        });
        Ok(self.command_conditions_hold(&json, view.as_ref(), source.pos))
    }
}

#[cfg(test)]
mod tests {
    use super::*;







    #[test]
    #[ignore = "daily"]
    fn function_pack_rejects_invalid_ordinary_lines_before_execution() {
        use crate::game::{Audio, Options, Settings, load_resources};
        use std::time::Instant;
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            Instant::now(),
            crate::game::execution_role::ExecutionRole::DedicatedServer,
            &crate::game::Progress::new(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.execution_role = crate::game::execution_role::ExecutionRole::DedicatedServer;
        let mut game = Game::new(
            &loaded.resources,
            loaded.pack,
            options,
            settings,
        )
        .unwrap();
        game.build_registries();
        game.set_allow_commands(true);
        let dir =
            std::env::temp_dir().join(format!("horizon-function-admission-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let pack = dir.join("datapacks/admission");
        let functions = pack.join("data/h144/function");
        let predicates = pack.join("data/h144/predicate");
        std::fs::create_dir_all(&functions).unwrap();
        std::fs::create_dir_all(&predicates).unwrap();
        std::fs::write(pack.join("pack.mcmeta"), r#"{"pack":{"description":"Function admission","pack_format":121,"min_format":[121,0],"max_format":[121,0]}}"#).unwrap();
        std::fs::write(
            predicates.join("ready.json"),
            r#"{"type":"minecraft:all_of","terms":[]}"#,
        )
        .unwrap();
        for (name, text) in [
            (
                "good",
                "execute if predicate h144:ready run scoreboard players add good h144 1\n",
            ),
            (
                "bad",
                "scoreboard players set bad h144 1\nh144_unknown_command\nscoreboard players set bad h144 2\n",
            ),
            (
                "forbidden",
                "scoreboard players set forbidden h144 1\ntick freeze\nscoreboard players set forbidden h144 2\n",
            ),
            (
                "macro",
                "$h144_unknown_$(name)\nscoreboard players set macro h144 1\n",
            ),
            ("macro_bad", "$say $(name)\nh144_unknown_command\n"),
            (
                "sequence",
                "scoreboard players \\\nset sequence h144 17\nreturn 5\n",
            ),
            ("slash", "/say invalid\n"),
        ] {
            std::fs::write(functions.join(format!("{name}.mcfunction")), text).unwrap();
        }
        game.world_root = dir.clone();
        let run = |game: &mut Game<'_>, line: &str| {
            game.exec = ExecState::default();
            let source = game.player_source();
            game.execute_line(line, source)
        };
        run(&mut game, "scoreboard objectives add h144 dummy").unwrap();
        for name in ["good", "bad", "forbidden", "macro", "sequence"] {
            run(
                &mut game,
                &format!("scoreboard players set {name} h144 -99"),
            )
            .unwrap();
        }
        let clock = game.world.game_time;
        // This is the actual pack loader. Admission must not run any line.
        game.reload_datapacks(false, &game.level.enabled_features.clone())
            .unwrap();
        for name in ["good", "bad", "forbidden", "macro", "sequence"] {
            assert_eq!(game.score.board.score(name, "h144").unwrap().value, -99);
        }
        // Intended old failure: the invalid file previously ran both valid lines.
        assert!(run(&mut game, "function h144:bad").is_err());
        assert_eq!(game.score.board.score("bad", "h144").unwrap().value, -99);
        for name in ["bad", "forbidden", "macro_bad", "slash"] {
            assert!(
                !game
                    .fns
                    .store
                    .functions
                    .contains_key(&format!("h144:{name}"))
            );
            assert!(
                !game
                    .session
                    .registries
                    .functions
                    .contains(&format!("h144:{name}"))
            );
            assert!(
                game.fns
                    .store
                    .errors
                    .iter()
                    .any(|(id, _)| id == &format!("h144:{name}"))
            );
        }
        assert!(run(&mut game, "function h144:forbidden").is_err());
        assert!(game.fns.store.functions.contains_key("h144:macro"));
        assert!(game.fns.store.functions["h144:macro"].is_macro());
        run(&mut game, "function h144:good").unwrap();
        assert_eq!(game.score.board.score("good", "h144").unwrap().value, -98);
        run(&mut game, "function h144:sequence").unwrap();
        assert_eq!(
            game.score.board.score("sequence", "h144").unwrap().value,
            17
        );
        assert_eq!(game.session.registries.permission, 4);
        // A fresh same-pack predicate must be in the compilation snapshot on reload.
        std::fs::write(
            predicates.join("reloaded.json"),
            r#"{"type":"minecraft:all_of","terms":[]}"#,
        )
        .unwrap();
        std::fs::write(
            functions.join("good.mcfunction"),
            "execute if predicate h144:reloaded run scoreboard players add good h144 1\n",
        )
        .unwrap();
        game.reload_datapacks(true, &game.level.enabled_features.clone())
            .unwrap();
        assert!(game.fns.store.functions.contains_key("h144:good"));
        assert!(!game.fns.store.functions.contains_key("h144:forbidden"));
        assert!(
            !game
                .session
                .registries
                .functions
                .contains(&"h144:forbidden".into())
        );
        assert_eq!(game.score.board.score("good", "h144").unwrap().value, -98);
        run(&mut game, "function h144:good").unwrap();
        assert_eq!(game.score.board.score("good", "h144").unwrap().value, -97);
        assert_eq!(
            game.score.board.score("forbidden", "h144").unwrap().value,
            -99
        );
        assert_eq!(game.world.game_time, clock);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// `StringTemplate.fromString` fails a `$` line with no variable, so the
    /// file is not registered, and `MacroFunction.MacroEntry.instantiate`
    /// parses a substituted line, throwing `commands.function.error.parse`
    /// before any of the call's lines run.
    #[test]
    #[ignore = "daily"]
    fn macro_lines_need_a_variable_and_parse_after_substitution() {
        use crate::game::{Audio, Options, Settings, load_resources};
        use std::time::Instant;
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            Instant::now(),
            crate::game::execution_role::ExecutionRole::DedicatedServer,
            &crate::game::Progress::new(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.execution_role = crate::game::execution_role::ExecutionRole::DedicatedServer;
        let mut game = Game::new(
            &loaded.resources,
            loaded.pack,
            options,
            settings,
        )
        .unwrap();
        game.build_registries();
        game.set_allow_commands(true);
        let dir = std::env::temp_dir().join(format!("horizon-macro-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let functions = dir.join("datapacks/macros/data/h166/function");
        std::fs::create_dir_all(&functions).unwrap();
        let format = horizon_core::version::DATA_PACK_FORMAT;
        std::fs::write(
            dir.join("datapacks/macros/pack.mcmeta"),
            serde_json::json!({"pack":{"description":"Macro lines","pack_format":format.0,"min_format":[format.0,format.1],"max_format":[format.0,format.1]}}).to_string(),
        )
        .unwrap();
        for (name, text) in [
            // No `$(…)`: not a macro at all, so the file does not load.
            ("novar", "$say hello\nscoreboard players add #ran h166 1\n"),
            // Substitutes to a line with a trailing argument; the plain lines
            // around it show that none of the call's lines ran.
            (
                "bad",
                "scoreboard players add #ran h166 1\n$scoreboard players set #seen h166 $(v) bogus\nscoreboard players add #ran h166 1\n",
            ),
            (
                "good",
                "$scoreboard players set #seen h166 $(v)\nscoreboard players add #ran h166 1\n",
            ),
        ] {
            std::fs::write(functions.join(format!("{name}.mcfunction")), text).unwrap();
        }
        game.world_root = dir.clone();
        let run = |game: &mut Game<'_>, line: &str| {
            game.exec = ExecState::default();
            let source = game.player_source();
            game.execute_line(line, source)
        };
        run(&mut game, "scoreboard objectives add h166 dummy").unwrap();
        run(&mut game, "scoreboard players set #ran h166 -99").unwrap();
        game.reload_datapacks(false, &game.level.enabled_features.clone())
            .unwrap();
        assert!(!game.fns.store.functions.contains_key("h166:novar"));
        assert!(
            game.fns
                .store
                .errors
                .iter()
                .any(|(id, e)| id == "h166:novar" && e.contains("No variables in macro"))
        );
        let error = run(&mut game, "function h166:novar").unwrap_err();
        assert_eq!(error.key, "arguments.function.unknown");
        let error = game.check_predicate("h166missing").unwrap_err();
        assert_eq!(error.key, "argument.resource_or_id.no_such_element");
        let text = error.message.plain();
        assert!(text.contains("minecraft:h166missing"), "{text}");
        assert!(text.contains("minecraft:predicate"), "{text}");
        // A substituted line that does not parse fails the call, so no line of
        // it runs, and the reason names the line `MacroEntry.instantiate` got.
        let error = run(&mut game, "function h166:bad {v:1}").unwrap_err();
        assert_eq!(error.key, "commands.function.instantiationFailure");
        let reason = error.message.plain();
        assert!(
            reason.contains("scoreboard players set #seen h166 1 bogus"),
            "{reason}"
        );
        assert_eq!(game.score.board.score("#ran", "h166").unwrap().value, -99);
        assert!(game.score.board.score("#seen", "h166").is_none());
        // A valid substitution still runs every line.
        run(&mut game, "function h166:good {v:7}").unwrap();
        assert_eq!(game.score.board.score("#ran", "h166").unwrap().value, -98);
        assert_eq!(game.score.board.score("#seen", "h166").unwrap().value, 7);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[ignore = "daily"]
    fn unless_function_keeps_a_fallthrough_after_running_it_once() {
        use crate::game::{Audio, Options, Settings, load_resources};
        use myriade_engine::datapack::FunctionLine;
        use std::time::Instant;
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            Instant::now(),
            crate::game::execution_role::ExecutionRole::DedicatedServer,
            &crate::game::Progress::new(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.execution_role = crate::game::execution_role::ExecutionRole::DedicatedServer;
        let mut game = Game::new(
            &loaded.resources,
            loaded.pack,
            options,
            settings,
        )
        .unwrap();
        game.build_registries();
        game.set_allow_commands(true);
        game.fns.loaded = true;
        let run = |game: &mut Game<'_>, line: &str| {
            game.exec = ExecState::default();
            let source = game.player_source();
            game.execute_line(line, source).unwrap();
        };
        run(&mut game, "scoreboard objectives add h154 dummy");
        let clock = game.world.game_time;
        for (name, tail) in [
            ("counter", None),
            ("zero", Some("return 0")),
            ("five", Some("return 5")),
            ("fail", Some("return fail")),
        ] {
            let id = format!("h154:{name}");
            let mut lines = vec![FunctionLine::Command(
                "scoreboard players add #calls h154 1".into(),
            )];
            if let Some(tail) = tail {
                lines.push(FunctionLine::Command(tail.into()));
            }
            game.fns.store.functions.insert(
                id.clone(),
                Function {
                    id,
                    lines,
                    parameters: Vec::new(),
                },
            );
        }
        // Explicit returns establish both signs before the old fallthrough
        // failure. The commands run through the actual function/execute route.
        for name in ["zero", "five", "fail", "counter"] {
            for sign in ["if", "unless"] {
                run(&mut game, "scoreboard players set #calls h154 0");
                run(&mut game, "scoreboard players set #out h154 99");
                run(
                    &mut game,
                    &format!(
                        "execute {sign} function h154:{name} run scoreboard players set #out h154 7"
                    ),
                );
                assert_eq!(game.score.board.score("#calls", "h154").unwrap().value, 1);
                let keep = (name == "five") == (sign == "if");
                assert_eq!(
                    game.score.board.score("#out", "h154").unwrap().value,
                    if keep { 7 } else { 99 },
                    "{sign} {name}"
                );
            }
        }
        assert_eq!(game.world.game_time, clock);
    }

    /// Two packs override the same recipe: the later one in the selected
    /// (`datapacks.json`) order wins, not the later one alphabetically.
    #[test]
    fn content_overlay_follows_enabled_order() {
        for order in [["file/a", "file/b"], ["file/b", "file/a"]] {
            let dir = std::env::temp_dir().join(format!(
                "horizon-overlay-{}-{}",
                std::process::id(),
                order[0].replace('/', "_")
            ));
            let _ = std::fs::remove_dir_all(&dir);
            for name in ["a", "b"] {
                let pack = dir.join("datapacks").join(name);
                std::fs::create_dir_all(pack.join("data/x/recipe")).unwrap();
                std::fs::write(
                    pack.join("pack.mcmeta"),
                    r#"{"pack":{"description":"fixture"}}"#,
                )
                .unwrap();
                std::fs::write(pack.join("data/x/recipe/r.json"), name).unwrap();
            }
            let json = serde_json::json!({"enabled": order, "disabled": []});
            std::fs::write(dir.join("datapacks.json"), json.to_string()).unwrap();
            let stack = pack_stack(
                &dir,
                std::path::Path::new(""),
                &Default::default(),
                &Default::default(),
                true,
                &[],
                &[],
            );
            let files = content_overlay(&stack);
            let (_, versions) = files.iter().find(|(id, _)| id == "recipe/x:r").unwrap();
            let last = versions.last().unwrap();
            assert_eq!(
                last.as_slice(),
                order[1].trim_start_matches("file/").as_bytes()
            );
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// What a native mod contributes (`native_data_pack_files`) joins the load
    /// the tables are built from, a world's first open and `/reload` alike
    /// (`Game::stage_world_tables` is both), and sits below the world's own
    /// packs. Each load asks again, so it is the current contribution that
    /// counts.
    #[test]
    fn contributed_files_join_the_load_below_the_worlds_packs() {
        let dir = std::env::temp_dir().join(format!("horizon-contributed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let pack = dir.join("datapacks").join("pack");
        std::fs::create_dir_all(pack.join("data/pkg/tags/item")).unwrap();
        std::fs::write(
            pack.join("pack.mcmeta"),
            r#"{"pack":{"description":"fixture"}}"#,
        )
        .unwrap();
        std::fs::write(
            pack.join("data/pkg/tags/item/both.json"),
            r#"{"replace": true, "values": ["pkg:from_pack"]}"#,
        )
        .unwrap();
        let settings = serde_json::json!({"enabled": ["file/pack"], "disabled": []});
        std::fs::write(dir.join("datapacks.json"), settings.to_string()).unwrap();
        let tag = |entry: &str| format!(r#"{{"values": ["{entry}"]}}"#).into_bytes();
        std::fs::create_dir_all(pack.join("data/pkg/recipe")).unwrap();
        std::fs::write(
            pack.join("data/pkg/recipe/shared.json"),
            br#"{"result":"world"}"#,
        )
        .unwrap();
        let passive = horizon_assets::resource_source::ResourceSource::indexed(
            ["shared", "native_only"]
                .map(|name| {
                    (
                        format!("data/pkg/recipe/{name}.json"),
                        horizon_assets::resource_source::Resource::bytes(
                            br#"{"result":"passive"}"#.to_vec(),
                        ),
                    )
                })
                .into_iter()
                .collect(),
        );
        let stack = pack_stack(
            &dir,
            std::path::Path::new(""),
            &Default::default(),
            &passive,
            true,
            &[],
            &[],
        );
        let files = overlay_files(
            &stack,
            vec![
                ("tags/pkg:item/both".to_owned(), tag("pkg:from_mod")),
                ("tags/pkg:item/mod_only".to_owned(), tag("pkg:only_mod")),
                (
                    "recipe/pkg:shared".into(),
                    br#"{"result":"native"}"#.to_vec(),
                ),
                (
                    "recipe/pkg:native_only".into(),
                    br#"{"result":"native"}"#.to_vec(),
                ),
            ],
        );
        let recipe_result = |id: &str| {
            let (_, versions) = files.iter().find(|(key, _)| key == id).unwrap();
            serde_json::from_slice::<serde_json::Value>(versions.last().unwrap()).unwrap()["result"]
                .clone()
        };
        assert_eq!(recipe_result("recipe/pkg:shared"), "world");
        assert_eq!(recipe_result("recipe/pkg:native_only"), "native");
        // The pack's file is the path's last (`PackOverlay::top`), and its
        // `replace` drops what the mod put below it.
        let (_, versions) = files
            .iter()
            .find(|(id, _)| id == "tags/pkg:item/both")
            .unwrap();
        assert!(horizon_world::loot::PackOverlay::top(versions).starts_with(b"{\"replace\""));
        let overlay = horizon_world::loot::PackOverlay::new(&files);
        let tags = horizon_items::items::load_tags(&dir, "item", &overlay).unwrap();
        assert!(tags["pkg:both"].contains("pkg:from_pack"));
        assert!(!tags["pkg:both"].contains("pkg:from_mod"));
        // A path only the mod has reaches the tables too.
        assert_eq!(
            tags["pkg:mod_only"].iter().collect::<Vec<_>>(),
            ["pkg:only_mod"]
        );
        // The next load asks again: the new contribution, not the first's.
        let files = overlay_files(
            &stack,
            vec![("tags/pkg:item/mod_only".to_owned(), tag("pkg:second_load"))],
        );
        let overlay = horizon_world::loot::PackOverlay::new(&files);
        let tags = horizon_items::items::load_tags(&dir, "item", &overlay).unwrap();
        assert_eq!(
            tags["pkg:mod_only"].iter().collect::<Vec<_>>(),
            ["pkg:second_load"]
        );
        // A file that isn't JSON is skipped alone, not the whole load.
        let files = overlay_files(
            &stack,
            vec![
                ("tags/pkg:item/mod_only".to_owned(), b"{not json".to_vec()),
                ("tags/pkg:item/good".to_owned(), tag("pkg:good")),
            ],
        );
        let overlay = horizon_world::loot::PackOverlay::new(&files);
        let tags = horizon_items::items::load_tags(&dir, "item", &overlay).unwrap();
        assert!(!tags.contains_key("pkg:mod_only"));
        assert!(tags["pkg:good"].contains("pkg:good"));
        // With nothing contributed the overlay is the packs' own.
        assert_eq!(overlay_files(&stack, Vec::new()), content_overlay(&stack));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod entity_score_tests {
    use super::*;
    use crate::game::{Audio, Options, Progress, Settings, commands::Target, load_resources};
    use horizon_core::nbt::Tag;

    #[test]
    #[ignore = "daily"]
    fn named_scores_follow_the_registered_target_holder_and_mutations() {
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            std::time::Instant::now(),
            crate::game::execution_role::ExecutionRole::DedicatedServer,
            &Progress::new(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.execution_role = crate::game::execution_role::ExecutionRole::DedicatedServer;
        let mut game = Game::new(
            &loaded.resources,
            loaded.pack,
            options,
            settings,
        )
        .unwrap();
        game.build_registries();

        game.set_allow_commands(true);
        game.ensure_datapacks();
        game.fns.store.predicates.insert(
            "h270:min2".into(),
            serde_json::json!({
                "type": "minecraft:entity_scores", "entity": "this", "scores": {"pts": {"min": 2}}
            }),
        );
        assert!(game.run_command("scoreboard objectives add pts dummy"));
        let mut actors = Vec::new();
        for (index, score) in [Some(0), Some(2), None].into_iter().enumerate() {
            let uuid = [270, 1, 0, index as i32 + 1];
            let tag = Tag::compound([
                ("id", Tag::String("minecraft:pig".into())),
                ("UUID", Tag::IntArray(uuid.to_vec())),
                (
                    "Pos",
                    Tag::List(vec![
                        Tag::Double((3.5 + index as f64).to_bits()),
                        Tag::Double(200.0_f64.to_bits()),
                        Tag::Double(4.5_f64.to_bits()),
                    ]),
                ),
                ("Health", Tag::Float(10.0_f32.to_bits())),
                ("NoAI", Tag::Byte(1)),
                ("NoGravity", Tag::Byte(1)),
            ]);
            let id = game.load_mob(tag, None).unwrap();
            assert_eq!(game.persist.uuids.get(&id), Some(&uuid));
            let holder = game.holder_name(Target::Mob(id)).unwrap();
            assert_eq!(holder, myriade_engine::save::uuid_text(uuid));
            if let Some(score) = score {
                assert!(game.run_command(&format!("scoreboard players set {holder} pts {score}")));
            }
            assert_eq!(
                game.score.board.score(&holder, "pts").map(|s| s.value),
                score
            );
            actors.push((id, holder));
        }
        let time = game.world.game_time;
        let matches = |game: &mut Game<'_>, id| {
            let mut source = game.player_source();
            source.entity = Some(Target::Mob(id));
            source.pos = game.target_position(Target::Mob(id)).unwrap();
            game.command_source = Some(source);
            let result = game.check_predicate("h270:min2").unwrap();
            game.command_source = None;
            result
        };
        assert!(!matches(&mut game, actors[0].0));
        assert!(matches(&mut game, actors[1].0));
        assert!(!matches(&mut game, actors[2].0));
        for (index, score, expected) in [(0, 3, true), (1, 1, false)] {
            let (id, holder) = &actors[index];
            assert!(game.run_command(&format!("scoreboard players set {holder} pts {score}")));
            assert_eq!(
                game.score.board.score(holder, "pts").map(|s| s.value),
                Some(score)
            );
            assert_eq!(matches(&mut game, *id), expected);
        }
        assert!(!matches(&mut game, actors[2].0));
        assert!(game.score.board.score(&actors[2].1, "pts").is_none());
        assert_eq!(game.world.game_time, time);
    }
    #[test]
    #[ignore = "daily"]
    fn folder_metadata_rejection_reaches_actual_function_commands() {
        use crate::game::{Audio, Options, Settings, load_resources};
        use std::time::Instant;
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            Instant::now(),
            crate::game::execution_role::ExecutionRole::DedicatedServer,
            &crate::game::Progress::new(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.execution_role = crate::game::execution_role::ExecutionRole::DedicatedServer;
        let mut game = Game::new(
            &loaded.resources,
            loaded.pack,
            options,
            settings,
        )
        .unwrap();
        game.build_registries();
        game.set_allow_commands(true);
        let dir = std::env::temp_dir().join(format!(
            "horizon-game-folder-metadata-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let cases = [
            ("broken", Some("{"), false),
            ("root_empty", Some("{}"), false),
            ("root_array", Some("[]"), false),
            ("pack_empty", Some(r#"{"pack":{}}"#), false),
            ("pack_string", Some(r#"{"pack":"bad"}"#), false),
            ("missing", None, false),
            (
                "full",
                Some(
                    r#"{"pack":{"description":"full","pack_format":121,"min_format":[121,0],"max_format":[121,0]}}"#,
                ),
                true,
            ),
            (
                "description",
                Some(r#"{"pack":{"description":"description only"}}"#),
                true,
            ),
        ];
        for (name, metadata, _) in cases {
            let pack = dir.join("datapacks").join(name);
            std::fs::create_dir_all(pack.join("data/h146/function")).unwrap();
            std::fs::write(
                pack.join(format!("data/h146/function/{name}.mcfunction")),
                format!("scoreboard players set {name} h146 17\n"),
            )
            .unwrap();
            if let Some(metadata) = metadata {
                std::fs::write(pack.join("pack.mcmeta"), metadata).unwrap();
            }
        }
        game.world_root = dir.clone();
        let run = |game: &mut Game<'_>, line: &str| {
            game.exec = ExecState::default();
            let source = game.player_source();
            game.execute_line(line, source)
        };
        run(&mut game, "scoreboard objectives add h146 dummy").unwrap();
        let clock = game.world.game_time;
        for reload in [false, true] {
            for (name, _, _) in cases {
                run(
                    &mut game,
                    &format!("scoreboard players set {name} h146 -99"),
                )
                .unwrap();
            }
            game.reload_datapacks(reload, &game.level.enabled_features.clone())
                .unwrap();
            for (name, _, valid) in cases {
                assert_eq!(game.score.board.score(name, "h146").unwrap().value, -99);
                let result = run(&mut game, &format!("function h146:{name}"));
                // Intended old failure: a malformed pack's valid function executes.
                assert_eq!(result.is_ok(), valid, "{name}, reload={reload}");
                assert_eq!(
                    game.score.board.score(name, "h146").unwrap().value,
                    if valid { 17 } else { -99 }
                );
                assert_eq!(
                    game.fns
                        .store
                        .functions
                        .contains_key(&format!("h146:{name}")),
                    valid
                );
                assert_eq!(
                    game.session
                        .registries
                        .functions
                        .contains(&format!("h146:{name}")),
                    valid
                );
                assert_eq!(
                    game.fns
                        .packs
                        .iter()
                        .any(|p| p.id == format!("file/{name}")),
                    valid
                );
            }
        }
        assert_eq!(game.world.game_time, clock);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod required_tag_tests {
    use super::*;

    #[test]
    #[ignore = "daily"]
    fn real_pack_required_tag_failures_execute_no_partial_functions() {
        use crate::game::{Audio, Options, Settings, load_resources};
        use std::time::{Instant, SystemTime, UNIX_EPOCH};
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            Instant::now(),
            crate::game::execution_role::ExecutionRole::DedicatedServer,
            &crate::game::Progress::new(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.execution_role = crate::game::execution_role::ExecutionRole::DedicatedServer;
        let mut game = Game::new(
            &loaded.resources,
            loaded.pack,
            options,
            settings,
        )
        .unwrap();
        let dir = std::env::temp_dir().join(format!(
            "horizon-required-tags-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        let pack = dir.join("datapacks/h145");
        let write = |path: &str, body: &str| {
            let path = pack.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        };
        let (major, minor) = horizon_core::version::DATA_PACK_FORMAT;
        write(
            "pack.mcmeta",
            &serde_json::json!({"pack": {
                "description": "Required dependency test", "pack_format": major,
                "min_format": [major, minor], "max_format": [major, minor],
            }})
            .to_string(),
        );
        for (i, name) in ["a", "b", "c"].iter().enumerate() {
            write(
                &format!("data/h145/function/{name}.mcfunction"),
                &format!(
                    "scoreboard players add {name} h145 1\nscoreboard players operation order h145 *= ten h145\nscoreboard players add order h145 {}\n",
                    i + 1,
                ),
            );
        }
        let tags = serde_json::json!({
            "empty": [], "ordered": ["h145:a", "h145:b", "h145:a"],
            "nested": ["#h145:empty", "#h145:ordered", "h145:a"],
            "optional_member": ["h145:a", {"id":"h145:missing","required":false}, "h145:b"],
            "optional_child": ["h145:a", {"id":"#h145:missing","required":false}, "h145:b"],
            "missing_first": ["h145:missing", "h145:a"],
            "missing_last": ["h145:a", "h145:missing"],
            "missing_child": ["h145:a", "#h145:missing"],
            "invalid_child": ["h145:c", "h145:missing"],
            "required_invalid_child": ["h145:a", "#h145:invalid_child", "h145:b"],
            "optional_invalid_child": ["h145:a", {"id":"#h145:invalid_child","required":false}, "h145:b"],
        });
        for (name, values) in tags.as_object().unwrap() {
            write(
                &format!("data/h145/tags/function/{name}.json"),
                &serde_json::json!({"values": values}).to_string(),
            );
        }
        game.world_root = dir.clone();
        game.build_registries();
        game.set_allow_commands(true);
        let run = |game: &mut Game<'_>, line: &str| {
            game.exec = ExecState::default();
            let source = game.player_source();
            if let Err(error) = game.execute_line(line, source) {
                assert!(line.contains("function #h145:"), "{line}: {error:?}");
                assert_eq!(error.key, "commands.function.scheduled.no_functions");
            }
        };
        run(&mut game, "scoreboard objectives add h145 dummy");
        run(&mut game, "scoreboard players set ten h145 10");
        run(&mut game, "scoreboard players set sentinel h145 73");
        let clock = game.world.game_time;
        run(&mut game, "reload");
        assert_eq!(
            game.score.board.score("sentinel", "h145").unwrap().value,
            73
        );
        assert!(game.fns.enabled.contains(&"file/h145".to_owned()));
        assert!(
            game.fns.store.errors.is_empty(),
            "{:?}",
            game.fns.store.errors
        );
        for name in ["a", "b", "c"] {
            assert!(
                game.fns
                    .store
                    .functions
                    .contains_key(&format!("h145:{name}"))
            );
        }
        for name in tags.as_object().unwrap().keys() {
            assert!(
                game.fns
                    .store
                    .function_tags
                    .contains_key(&format!("h145:{name}"))
            );
        }
        let score = |game: &Game<'_>, key: &str| game.score.board.score(key, "h145").unwrap().value;
        let reset = |game: &mut Game<'_>| {
            for key in ["a", "b", "c", "order"] {
                run(game, &format!("scoreboard players set {key} h145 0"));
                assert_eq!(score(game, key), 0);
            }
        };
        // Independent actual functions and valid tag controls run before the defect assertions.
        for (target, expected) in [
            ("h145:a", [1, 0, 0, 1]),
            ("h145:b", [0, 1, 0, 2]),
            ("h145:c", [0, 0, 1, 3]),
            ("#h145:empty", [0, 0, 0, 0]),
            ("#h145:ordered", [1, 1, 0, 12]),
            ("#h145:nested", [1, 1, 0, 12]),
            ("#h145:optional_member", [1, 1, 0, 12]),
            ("#h145:optional_child", [1, 1, 0, 12]),
        ] {
            reset(&mut game);
            run(&mut game, &format!("function {target}"));
            assert_eq!(
                [
                    score(&game, "a"),
                    score(&game, "b"),
                    score(&game, "c"),
                    score(&game, "order")
                ],
                expected,
                "{target}"
            );
        }
        println!(
            "H145 actual pack metadata, functions, empty/nested/dedup, optional-missing and reload controls admitted"
        );
        let mut failures = Vec::new();
        for name in [
            "missing_first",
            "missing_last",
            "missing_child",
            "invalid_child",
            "required_invalid_child",
            "optional_invalid_child",
        ] {
            reset(&mut game);
            run(&mut game, "scoreboard players set result h145 31");
            run(&mut game, "scoreboard players set success h145 17");
            assert_eq!((score(&game, "result"), score(&game, "success")), (31, 17));
            run(
                &mut game,
                &format!(
                    "execute store result score result h145 store success score success h145 run function #h145:{name}"
                ),
            );
            let actual = [
                score(&game, "a"),
                score(&game, "b"),
                score(&game, "c"),
                score(&game, "order"),
            ];
            let expected = if name == "optional_invalid_child" {
                [1, 1, 0, 12]
            } else {
                [0, 0, 0, 0]
            };
            if actual != expected {
                failures.push(format!("{name}: {actual:?} expected {expected:?}"));
            }
            let ids = game.fns.store.tag_functions(&format!("h145:{name}"));
            let expected_ids: Vec<String> = if name == "optional_invalid_child" {
                vec!["h145:a".into(), "h145:b".into()]
            } else {
                Vec::new()
            };
            if ids != expected_ids {
                failures.push(format!(
                    "{name}: DataStore {ids:?} expected {expected_ids:?}"
                ));
            }
        }
        assert_eq!(game.world.game_time, clock);
        drop(game);
        std::fs::remove_dir_all(dir).unwrap();
        assert!(
            failures.is_empty(),
            "required tag partial execution: {failures:#?}"
        );
    }

}

