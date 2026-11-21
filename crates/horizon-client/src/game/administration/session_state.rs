//! The server session's state (command registries, game rules, world clocks, difficulty)
//! and the methods that keep it: the registries commands read, the clocks the world runs on,
//! and the world's state saved beside level.dat.
use super::command_text::{
    BlockProperties, PLAYER_NAME, SessionDispatcher, block_properties, command_export,
};
use crate::game::Game;
use horizon_client::{command::Registries, gamerules::GameRules};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// The Overworld's world clock.
const OVERWORLD_CLOCK: &str = crate::game::wake_time::OVERWORLD_CLOCK;

/// ServerClockManager.ServerClockInstance (the total ticks of the one the level in place follows
/// are the world's day time).
#[derive(Clone, Copy, Debug)]
pub struct WorldClock {
    pub partial: f32,
    pub rate: f32,
    pub paused: bool,
}
impl Default for WorldClock {
    fn default() -> Self {
        Self {
            partial: 0.0,
            rate: 1.0,
            paused: false,
        }
    }
}

/// The state of the server session a world runs with.
#[derive(Default)]
pub struct SessionState {
    pub registries: Registries,
    pub rules: GameRules,
    /// The server session command tree.
    pub dispatcher: SessionDispatcher,
    pub clock: WorldClock,
    /// The totals and state of the clocks the world has met, the Overworld's total among them
    /// while another clock is the one in `active_clock`.
    pub command_clocks: BTreeMap<String, (i64, WorldClock)>,
    /// The clock `World.day_time` follows; empty means the Overworld's.
    pub active_clock: String,
    pub end_clock: WorldClock,
    /// Time markers and timelines of the overworld clock.
    pub markers: BTreeMap<String, crate::game::wake_time::TimeMarker>,
    pub timelines: BTreeMap<String, Option<i64>>,
    /// 0 peaceful .. 3 hard.
    pub difficulty: u8,
    /// Death message key set by the cause of death (e.g. /kill).
    pub death_cause: Option<String>,
    /// The prepared resources before resource packs are layered on.
    pub vanilla_root: PathBuf,
}

/// The ids `build_registries` lists from the prepared resources' folders.
struct ResourceIds {
    recipes: Vec<String>,
    templates: Vec<String>,
    entity_types: Vec<String>,
    enchantments: Vec<String>,
    structures: Vec<String>,
    biomes: Vec<String>,
    clocks: Vec<String>,
    timelines: Vec<(String, Option<i64>)>,
    markers: Vec<(String, crate::game::wake_time::TimeMarker)>,
}
static RESOURCE_IDS: Mutex<Vec<(PathBuf, Arc<ResourceIds>)>> = Mutex::new(Vec::new());

/// The `minecraft:` ids of the `.json` files directly in `dir`, sorted.
fn json_ids_in(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_suffix(".json")
                .map(|n| format!("minecraft:{n}"))
        })
        .collect();
    out.sort();
    out
}

/// The ids of the files with `extension` under `root`, as `namespace:path`, sorted.
fn ids_in(root: &Path, extension: &str, namespace: &str) -> Vec<String> {
    let suffix = format!(".{extension}");
    if let Some(files) = horizon_core::prepared_files::files_under(root) {
        let mut out: Vec<String> = files
            .iter()
            .filter_map(|file| file.strip_suffix(&suffix))
            .filter(|path| !path.is_empty() && !path.ends_with('/'))
            .map(|path| format!("{namespace}:{path}"))
            .collect();
        out.sort();
        return out;
    }
    let mut out = Vec::new();
    let mut stack = vec![root.to_owned()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            let is_dir = e
                .file_type()
                .is_ok_and(|t| t.is_dir() || (t.is_symlink() && path.is_dir()));
            if is_dir {
                stack.push(path);
            } else if path.extension().and_then(|x| x.to_str()) == Some(extension)
                && let Ok(rel) = path.strip_prefix(root)
            {
                out.push(format!(
                    "{namespace}:{}",
                    rel.with_extension("").to_string_lossy().replace('\\', "/")
                ));
            }
        }
    }
    out.sort();
    out
}

/// The overworld clock's timelines (with their periods) and their time
/// markers, read from `data/minecraft/timeline`.
fn overworld_timelines(
    root: &Path,
) -> (
    Vec<(String, Option<i64>)>,
    Vec<(String, crate::game::wake_time::TimeMarker)>,
) {
    let dir = root.join("data/minecraft/timeline");
    let (mut timelines, mut markers) = (Vec::new(), Vec::new());
    for name in json_ids_in(&dir) {
        let path = dir.join(format!("{}.json", name.trim_start_matches("minecraft:")));
        let Ok(v) =
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(path).unwrap_or_default())
        else {
            continue;
        };
        if v["clock"].as_str() != Some("minecraft:overworld") {
            continue;
        }
        let period = v["period_ticks"].as_i64();
        timelines.push((name.clone(), period));
        for (marker, def) in v["time_markers"].as_object().into_iter().flatten() {
            let (ticks, show) = match def {
                serde_json::Value::Number(n) => (n.as_i64().unwrap_or(0), false),
                other => (
                    other["ticks"].as_i64().unwrap_or(0),
                    other["show_in_commands"].as_bool().unwrap_or(false),
                ),
            };
            markers.push((
                marker.clone(),
                crate::game::wake_time::TimeMarker {
                    ticks,
                    period,
                    show,
                },
            ));
        }
    }
    (timelines, markers)
}

/// The ids of the `.json` files in `data/<namespace>/<kind>` of every namespace of `root`, sorted.
fn json_ids_of_every_namespace(root: &Path, kind: &str) -> Vec<String> {
    let data = root.join("data");
    let mut namespaces: std::collections::BTreeSet<String> = std::fs::read_dir(&data)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    if let Some(files) = horizon_core::prepared_files::files_under(&data) {
        namespaces.extend(
            files
                .iter()
                .filter_map(|f| Some(f.split_once('/')?.0.to_owned())),
        );
    }
    let mut out: Vec<String> = namespaces
        .iter()
        .flat_map(|ns| ids_in(&data.join(ns).join(kind), "json", ns))
        .collect();
    out.sort();
    out
}

/// The templates of every namespace folder under `root`, as `namespace:path`, sorted.
fn templates_under(root: &Path) -> Vec<String> {
    let Ok(namespaces) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<String> = namespaces
        .flatten()
        .filter(|e| e.path().is_dir())
        .flat_map(|e| ids_in(&e.path(), "nbt", &e.file_name().to_string_lossy()))
        .collect();
    out.sort();
    out
}

fn resource_ids(root: &Path) -> Arc<ResourceIds> {
    let mut cache = RESOURCE_IDS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, ids)) = cache.iter().find(|(cached, _)| cached == root) {
        return ids.clone();
    }
    let data = root.join("data/minecraft");
    let entity_types = crate::game::entity_type_rows::entity_type_rows(root)
        .ids
        .clone();
    let (timelines, markers) = overworld_timelines(root);
    let ids = Arc::new(ResourceIds {
        recipes: json_ids_of_every_namespace(root, "recipe"),
        templates: templates_under(&root.join("worldgen/structure")),
        entity_types,
        enchantments: json_ids_of_every_namespace(root, "enchantment"),
        structures: json_ids_in(&data.join("worldgen/structure")),
        biomes: json_ids_in(&data.join("worldgen/biome")),
        clocks: json_ids_in(&data.join("world_clock")),
        timelines,
        markers,
    });
    cache.push((root.to_owned(), ids.clone()));
    ids
}

/// `IntegratedServer.getProfilePermissions` for the owner: `OWNER` (4) when
/// commands are allowed, else `ALL` (0).
fn owner_permission(allow_commands: bool) -> u8 {
    if allow_commands { 4 } else { 0 }
}

/// `SuggestionProviders.SUMMONABLE_ENTITIES`: Entity types available for
/// `/summon` autocomplete, filtering out types that cannot be summoned.
fn summonable_entities(
    entity_types: &[String],
    mob_types: impl IntoIterator<Item = impl AsRef<str>>,
) -> Vec<String> {
    let mut summonable: Vec<String> = if !entity_types.is_empty() {
        entity_types
            .iter()
            .filter(|id| !matches!(id.as_str(), "minecraft:player" | "minecraft:fishing_bobber"))
            .cloned()
            .collect()
    } else {
        let mut list: Vec<String> = mob_types
            .into_iter()
            .map(|k| horizon_client::command::args::normalize(k.as_ref()))
            .filter(|id| !matches!(id.as_str(), "minecraft:player" | "minecraft:fishing_bobber"))
            .collect();
        list.extend(
            [
                "minecraft:item",
                "minecraft:experience_orb",
                "minecraft:falling_block",
                "minecraft:arrow",
                "minecraft:lightning_bolt",
                "minecraft:minecart",
                "minecraft:chest_minecart",
                "minecraft:hopper_minecart",
                "minecraft:furnace_minecart",
                "minecraft:tnt_minecart",
                "minecraft:wind_charge",
                "minecraft:breeze_wind_charge",
            ]
            .map(String::from),
        );
        list
    };
    let thrown = horizon_client::projectiles::thrown_types().iter();
    summonable.extend(thrown.map(|t| t.id.to_owned()));
    summonable.sort();
    summonable.dedup();
    summonable
}

impl Game<'_> {
    /// `Commands` run as the console: the owner's level whatever Allow Commands says.
    pub(in crate::game) fn as_console<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let level = std::mem::replace(&mut self.session.registries.permission, 4);
        let out = f(self);
        self.session.registries.permission = level;
        out
    }
    /// `PlayerList.isOp` for the single-player owner: the world's Allow Commands.
    pub(in crate::game) fn allow_commands(&self) -> bool {
        self.level.allow_commands == Some(true)
    }
    /// `ResourceOrIdArgument`'s loot tables are the server's: every table the
    /// world plays with, the game's, the content mods' and its packs'.
    pub(in crate::game) fn sync_loot_table_ids(&mut self) {
        let mut ids: Vec<String> = self.items().loot_table_ids().map(str::to_owned).collect();
        ids.sort();
        self.session.registries.loot_tables = ids;
    }
    /// Registry contents for argument parsing (built when a world opens).
    pub(in crate::game) fn build_registries(&mut self) {
        let permission = owner_permission(self.allow_commands());
        let reg = &mut self.session.registries;
        reg.permission = permission;
        let ids = resource_ids(&self.options.resources);
        self.session.registries.recipes = ids.recipes.clone();
        self.session.registries.damage_types = self
            .health_resources
            .damage
            .entries()
            .map(|(_, d)| d.name.clone())
            .collect();
        self.sync_loot_table_ids();
        self.session.registries.inline_feature = self.res.behaviours().live_features().map(|features| {
            horizon_client_core::registries::InlineFeatureCheck::new(move |tag| {
                let json = horizon_client::scoreboard::tag_to_json(tag).to_string();
                features
                    .decode_inline(&json)
                    .map(drop)
                    .map_err(|e| e.to_string())
            })
        });
        self.session.registries.templates = ids.templates.clone();
        self.session.registries.attributes = [
            "armor",
            "armor_toughness",
            "attack_damage",
            "attack_knockback",
            "attack_speed",
            "block_break_speed",
            "block_interaction_range",
            "burning_time",
            "camera_distance",
            "explosion_knockback_resistance",
            "entity_interaction_range",
            "fall_damage_multiplier",
            "flying_speed",
            "follow_range",
            "gravity",
            "jump_strength",
            "knockback_resistance",
            "luck",
            "max_absorption",
            "max_health",
            "mining_efficiency",
            "movement_efficiency",
            "movement_speed",
            "oxygen_bonus",
            "safe_fall_distance",
            "scale",
            "sneaking_speed",
            "spawn_reinforcements",
            "step_height",
            "submerged_mining_speed",
            "sweeping_damage_ratio",
            "tempt_range",
            "water_movement_efficiency",
        ]
        .iter()
        .map(|a| format!("minecraft:{a}"))
        .collect();
        let reg = &mut self.session.registries;
        let mut items: Vec<String> = self
            .res
            .items()
            .order
            .iter()
            .map(|s| s.to_string())
            .collect();
        items.sort();
        reg.items = items;
        reg.item_tags = self
            .res
            .items()
            .item_tags
            .keys()
            .map(|k| horizon_client::command::args::normalize(k))
            .collect();
        reg.item_tags.sort();
        reg.block_tags = self
            .res
            .items()
            .block_tags
            .keys()
            .map(|k| horizon_client::command::args::normalize(k))
            .collect();
        reg.block_tags.sort();
        reg.blocks = BlockProperties::clone(&block_properties(
            self.pack.assets.registry,
        ));
        reg.summonable = summonable_entities(&ids.entity_types, self.res.mob_types().keys());
        let mut types = ids.entity_types.clone();
        if types.is_empty() {
            types = reg.summonable.clone();
            types.push("minecraft:player".into());
            types.sort();
        }
        reg.entity_types = types;
        reg.effects = horizon_client::brewing::all_effects()
            .map(|e| horizon_client::brewing::namespaced(e.id).to_owned())
            .collect();
        reg.effects.sort();
        reg.enchantments = ids.enchantments.clone();
        reg.structures = ids.structures.clone();
        reg.biomes = ids.biomes.clone();
        reg.clocks = ids.clocks.clone();
        reg.dimensions = vec![
            "minecraft:overworld".into(),
            "minecraft:the_nether".into(),
            "minecraft:the_end".into(),
        ];
        reg.players = vec![PLAYER_NAME.into()];
        self.session.timelines = ids.timelines.iter().cloned().collect();
        self.session.markers = ids.markers.iter().cloned().collect();
        let mut sounds = Vec::new();
        for dir in [&self.options.resources, &self.session.vanilla_root] {
            if let Some(export) = command_export(dir, self.options.dedicated.is_none()) {
                self.session.registries.fill_from_exported(&export);
                sounds = export.ids("minecraft:sound_event").to_vec();
                sounds.sort();
                break;
            }
        }
        self.session.registries.sounds = sounds;
        let reg = &mut self.session.registries;
        for (_, name) in horizon_protocol::item::custom::entries() {
            if !reg.persistent_components.iter().any(|entry| entry == name) {
                reg.persistent_components.push(name.to_owned());
            }
        }
        reg.timelines = self.session.timelines.keys().cloned().collect();
        reg.time_markers = self
            .session
            .markers
            .iter()
            .filter(|(_, m)| m.show)
            .map(|(k, _)| k.clone())
            .collect();
        self.sync_enchantment_registries();
        self.session.registries.selector_tokens =
            crate::game::native_entity_selectors::selector_tokens();
        self.sync_stat_registries();
        self.sync_pack_tag_ids();
    }
    /// A server message: the console's log, the players' copy being sent by the caller.
    pub(in crate::game) fn system_message(&mut self, text: horizon_client::text::Text) {
        log::info!("System chat: {}", text.plain());
    }
    /// A player's line on the server: the console's log.
    pub(in crate::game) fn player_message(&mut self, text: horizon_client::text::Text) {
        log::info!("Player chat: {}", text.plain());
    }
    /// Command suggestions include the world's currently named random sequences.
    pub(in crate::game) fn suggestion_registries(&mut self) -> &mut Registries {
        self.session.registries.random_sequences = self.rand_sequences.ids();
        &mut self.session.registries
    }
    /// Advance every clock the world has met (ServerClockManager.tick) when `advance_time` allows
    /// it: each at its own rate, unless paused. The active clock's total is World.day_time.
    pub(in crate::game) fn advance_clock(&mut self) {
        if !self
            .session
            .rules
            .bool_at(horizon_client::gamerules::vanilla::ADVANCE_TIME)
        {
            self.sync_hub_clocks();
            return;
        }
        let mut names: Vec<String> = self.session.command_clocks.keys().cloned().collect();
        let active = self.active_clock().to_owned();
        for name in [
            OVERWORLD_CLOCK.to_owned(),
            "minecraft:the_end".to_owned(),
            active,
        ] {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        for name in names {
            let state = self.command_clock_state(&name);
            if state.paused {
                continue;
            }
            state.partial += state.rate;
            let whole = state.partial.floor();
            state.partial -= whole;
            let total = self.command_clock_time(&name) + whole as i64;
            self.set_command_clock_time(&name, total);
        }
        self.sync_hub_clocks();
    }
    /// The clock `World.day_time` follows (`SessionState::active_clock`).
    pub(in crate::game) fn active_clock(&self) -> &str {
        if self.session.active_clock.is_empty() {
            OVERWORLD_CLOCK
        } else {
            &self.session.active_clock
        }
    }
    /// `ServerClockManager.getTotalTicks` of `clock`: the active one's is the world's day time,
    /// another's the table's (0 for a clock the world has not met).
    pub(in crate::game) fn command_clock_time(&self, clock: &str) -> i64 {
        if clock == self.active_clock() {
            self.world.day_time
        } else if clock == "minecraft:the_end" {
            self.world.end_clock
        } else {
            self.session.command_clocks.get(clock).map_or(0, |c| c.0)
        }
    }
    pub(in crate::game) fn set_command_clock_time(&mut self, clock: &str, total: i64) {
        if clock == "minecraft:the_end" {
            self.world.end_clock = total;
        }
        if clock == self.active_clock() {
            self.world.day_time = total;
        } else {
            self.session
                .command_clocks
                .entry(clock.to_owned())
                .or_default()
                .0 = total;
        }
    }
    /// `ServerClockManager.getInstance`: the rate, partial tick and pause of `clock`.
    pub(in crate::game) fn command_clock_state(&mut self, clock: &str) -> &mut WorldClock {
        if clock == OVERWORLD_CLOCK {
            &mut self.session.clock
        } else if clock == "minecraft:the_end" {
            &mut self.session.end_clock
        } else {
            &mut self
                .session
                .command_clocks
                .entry(clock.to_owned())
                .or_default()
                .1
        }
    }
    /// The clock the day time of `dim`'s level follows: its dimension type's default clock, else
    /// the Overworld's.
    pub(in crate::game) fn level_clock_name(&self, dim: crate::game::Dim) -> String {
        self.dimension_type_of(dim)
            .default_clock
            .clone()
            .unwrap_or_else(|| OVERWORLD_CLOCK.to_owned())
    }
    /// What the LAN hub's time packets need: the Overworld's total while `World.day_time` follows
    /// another clock, and that clock with its rate.
    pub(in crate::game) fn sync_hub_clocks(&mut self) {
        let active = self.active_clock().to_owned();
        let overworld = self.command_clock_time(OVERWORLD_CLOCK);
        let advancing = self
            .session
            .rules
            .bool_at(horizon_client::gamerules::vanilla::ADVANCE_TIME);
        let state = *self.command_clock_state(&active);
        let rate = if advancing && !state.paused {
            state.rate
        } else {
            0.0
        };
        if let Some(host) = self.lan.as_mut() {
            if active == OVERWORLD_CLOCK {
                host.hub.overworld_day = None;
                host.hub.level_clock = None;
            } else {
                host.hub.overworld_day = Some(overworld);
                host.hub.level_clock = Some((active, rate));
            }
        }
    }
    /// Makes `to` the clock `World.day_time` follows, as the level entered has it as its default
    /// clock: the clock that was followed keeps its total in the table. Returns the total
    /// `World.day_time` takes.
    pub(in crate::game) fn switch_clock(&mut self, to: &str) -> i64 {
        let from = self.active_clock().to_owned();
        if from == to {
            return self.world.day_time;
        }
        let total = self.world.day_time;
        if from == "minecraft:the_end" {
            self.world.end_clock = total;
        }
        self.session.command_clocks.entry(from).or_default().0 = total;
        self.session.active_clock = to.to_owned();
        if to == "minecraft:the_end" {
            self.world.end_clock
        } else {
            self.session.command_clocks.entry(to.to_owned()).or_default().0
        }
    }
    /// `ServerClockInstance.packNetworkState`: the rate players' clients
    /// advance the overworld clock at, 0 while it is paused or `advance_time` is off.
    pub(in crate::game) fn network_clock_rate(&self) -> f32 {
        if self
            .session
            .rules
            .bool_at(horizon_client::gamerules::vanilla::ADVANCE_TIME)
            && !self.session.clock.paused
        {
            self.session.clock.rate
        } else {
            0.0
        }
    }
    /// The engine-side rule values (block behaviour and health).
    pub(in crate::game) fn level_rules(&self) -> horizon_world::level::GameRules {
        let r = &self.session.rules;
        horizon_world::level::GameRules {
            water_source_conversion: r
                .bool_at(horizon_client::gamerules::vanilla::WATER_SOURCE_CONVERSION),
            lava_source_conversion: r
                .bool_at(horizon_client::gamerules::vanilla::LAVA_SOURCE_CONVERSION),
            random_tick_speed: r.int_at(horizon_client::gamerules::vanilla::RANDOM_TICK_SPEED),
            block_drops: r.bool_at(horizon_client::gamerules::vanilla::BLOCK_DROPS),
            mob_griefing: r.bool_at(horizon_client::gamerules::vanilla::MOB_GRIEFING),
            max_snow_accumulation_height: r
                .int_at(horizon_client::gamerules::vanilla::MAX_SNOW_ACCUMULATION_HEIGHT),
            spawn_mobs: r.bool_at(horizon_client::gamerules::vanilla::SPAWN_MOBS),
            spawn_monsters: r.bool_at(horizon_client::gamerules::vanilla::SPAWN_MONSTERS),
            fire_spread_radius_around_player: r
                .int_at(horizon_client::gamerules::vanilla::FIRE_SPREAD_RADIUS_AROUND_PLAYER),
            spread_vines: r.bool_at(horizon_client::gamerules::vanilla::SPREAD_VINES),
        }
    }
    /// Apply rules mirrored in other state (natural regeneration, difficulty).
    pub(in crate::game) fn sync_rules(&mut self) {
        let rules = &self.session.rules;
        self.world.health_rules.natural_regeneration =
            rules.bool_at(horizon_client::gamerules::vanilla::NATURAL_HEALTH_REGENERATION);
        // Player.isInvulnerableTo: the damage-type rules.
        self.world.health_rules.drowning_damage =
            rules.bool_at(horizon_client::gamerules::vanilla::DROWNING_DAMAGE);
        self.world.health_rules.fall_damage =
            rules.bool_at(horizon_client::gamerules::vanilla::FALL_DAMAGE);
        self.world.health_rules.fire_damage =
            rules.bool_at(horizon_client::gamerules::vanilla::FIRE_DAMAGE);
        self.world.health_rules.freeze_damage =
            rules.bool_at(horizon_client::gamerules::vanilla::FREEZE_DAMAGE);
        horizon_client::mobs::sulfur_cube::set_tnt_explodes(
            rules.bool_at(horizon_client::gamerules::vanilla::TNT_EXPLODES),
        );
        use horizon_world::food::Difficulty;
        self.world.health_rules.difficulty = match self.session.difficulty {
            0 => Difficulty::Peaceful,
            1 => Difficulty::Easy,
            3 => Difficulty::Hard,
            _ => Difficulty::Normal,
        };
        // The ECS reads the difficulty (raids, targeting) and drops what peaceful forbids.
        self.ecs.set_difficulty(self.session.difficulty);
        // `ServerLevel.isSpawningMonsters`, read by the reinforcements a hurt creature calls.
        let rules = &self.session.rules;
        self.ecs.set_monster_spawning(
            rules.bool_at(horizon_client::gamerules::vanilla::SPAWN_MOBS)
                && rules.bool_at(horizon_client::gamerules::vanilla::SPAWN_MONSTERS),
        );
    }
    /// `ServerClockManager.ServerClockInstance.loadFrom`: restore the existing
    /// Overworld state when a world opens; clear the other command-state cache.
    /// The carrier store's open-time state settles here too, before the
    /// first tick and any player: a refusal fails the open.
    pub(in crate::game) fn load_world_state(&mut self) -> crate::Result<()> {
        self.session.dispatcher = SessionDispatcher(Arc::new(
            horizon_client::command::vanilla::session_dispatcher(
                &self.level.rule_registry,
                &self.level.enabled_features,
            ),
        ));
        self.restore_native_commands();
        self.session.clock = WorldClock::default();
        self.session.end_clock = WorldClock::default();
        self.session.command_clocks.clear();
        self.session.difficulty = self.level.difficulty_id();
        self.session.active_clock = OVERWORLD_CLOCK.to_owned();
        if let Ok(bytes) = std::fs::read(self.world_root.join("data/minecraft/world_clocks.dat"))
            && let Ok(raw) = horizon_world::compression::Compression::Gzip.decode(&bytes, 1 << 20)
            && let Ok((root, _)) =
                horizon_core::nbt::read_named(&raw, horizon_core::nbt::Limits::default())
            && let Some(horizon_core::nbt::Tag::Compound(clocks)) = root.tag.get("data")
        {
            use horizon_core::nbt::Tag;
            for (name, c) in clocks {
                let mut state = WorldClock::default();
                if let Some(Tag::Float(r)) = c.get("rate") {
                    state.rate = f32::from_bits(*r);
                }
                if let Some(Tag::Float(p)) = c.get("partial_tick") {
                    state.partial = f32::from_bits(*p);
                }
                if let Some(Tag::Byte(p)) = c.get("paused") {
                    state.paused = *p != 0;
                }
                let total = match c.get("total_ticks") {
                    Some(Tag::Long(t)) => *t,
                    _ => 0,
                };
                let name = name.to_string_lossy();
                if name == OVERWORLD_CLOCK {
                    self.session.clock = state;
                } else if name == "minecraft:the_end" {
                    self.session.end_clock = state;
                } else {
                    self.session.command_clocks.insert(name, (total, state));
                }
            }
        }
        let clock = self.level_clock_name(self.dim);
        self.world.day_time = self.switch_clock(&clock);
        self.build_registries();
        self.snapshot_test_defs();
        self.sync_rules();
        self.carrier_world_opened()
    }
    /// Save the clock beside level.dat (Level.write saves the game rules: a
    /// second writer of game_rules.dat could leave it with the rules loaded
    /// when the world opened).
    pub(in crate::game) fn save_world_state(&mut self) {
        if self.world_root.as_os_str().is_empty() {
            return;
        }
        use horizon_core::nbt::Tag;
        // Every clock the world has met is saved; the fields of a clock Horizon does not model
        // stay as saved, and rate, partial tick and pause are omitted while they have their defaults.
        let path = self.world_root.join("data/minecraft/world_clocks.dat");
        let existing = horizon_client::save::read_dat(&path);
        let mut data = match existing.as_ref().and_then(|r| r.get("data")) {
            Some(d @ Tag::Compound(_)) => d.clone(),
            _ => Tag::Compound(Default::default()),
        };
        let mut clocks = vec![(
            OVERWORLD_CLOCK.to_owned(),
            self.command_clock_time(OVERWORLD_CLOCK),
            self.session.clock,
        )];
        clocks.push((
            "minecraft:the_end".to_owned(),
            self.command_clock_time("minecraft:the_end"),
            self.session.end_clock,
        ));
        for (name, (_, state)) in &self.session.command_clocks {
            if name != OVERWORLD_CLOCK && name != "minecraft:the_end" {
                clocks.push((name.clone(), self.command_clock_time(name), *state));
            }
        }
        for (name, total, c) in clocks {
            let mut clock = match data.get(&name) {
                Some(c @ Tag::Compound(_)) => c.clone(),
                _ => Tag::Compound(Default::default()),
            };
            if let Tag::Compound(map) = &mut clock {
                map.insert("total_ticks".into(), Tag::Long(total));
                for (key, value, default) in [
                    (
                        "partial_tick",
                        Tag::Float(c.partial.to_bits()),
                        c.partial == 0.0,
                    ),
                    ("rate", Tag::Float(c.rate.to_bits()), c.rate == 1.0),
                    ("paused", Tag::Byte(i8::from(c.paused)), !c.paused),
                ] {
                    if default {
                        map.remove(&key.into());
                    } else {
                        map.insert(key.into(), value);
                    }
                }
            }
            if let Tag::Compound(map) = &mut data {
                map.insert(name.as_str().into(), clock);
            }
        }
        let root = Tag::compound([
            ("data", data),
            ("DataVersion", Tag::Int(horizon_world::DATA_VERSION)),
        ]);
        if let Err(e) = horizon_client::save::write_dat(&path, root) {
            log::warn!("the world clocks were not written: {e}");
        }
    }
}
