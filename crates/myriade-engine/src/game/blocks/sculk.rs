//! Game events and sculk on the game side: dispatching events to sculk
//! sensors, shriekers and wardens (`GameEventDispatcher`), the vibration
//! ticker, redstone output, resonance, shrieks and warden summoning
//! (`SculkShriekerBlockEntity`, `SpawnUtil`), catalysts feeding the sculk
//! spreader from deaths, and the darkness wardens and shriekers cast.
use super::{Game, Result, commands::R};
use myriade_engine::{
    containers::BlockEntity,
    effects::{EffectInstance, PlayerEffects},
    fast_hash::{FxHashMap, FxHashSet},
    mobs::{Species, Who, warden},
    particles::ParticleOptions,
    sculk::{self, Context, Info, SculkEntity, SculkKind, WardenSpawnTracker},
    text::Text,
};
use myriade_data::who::EventSource;
use horizon_core::{
    geometry::{Aabb, Direction, Vec3},
    position::BlockPos,
    random::RandomSource,
};
use horizon_world::registry::BlockStateId;
use myriade_types::PositionSource;
use crate::game::cosmetic_random::cosmetic_random;
use std::collections::HashSet;

pub const HELP: &[&str] = &[
    "sculk                          sculk listeners, the warning level and the darkness blend",
    "sculk charge X Y Z AMOUNT      a mob dying there with that much experience (the nearest catalyst takes it)",
    "sculk warn LEVEL               set the player's warden warning level",
    "sculk dig                      end the minute of calm of every warden",
];

/// The sculk block kinds by state id, built once.
#[derive(Default)]
struct Tables {
    built: bool,
    /// 0 none, 1 sensor, 2 calibrated sensor, 3 shrieker, 4 catalyst.
    kinds: Vec<u8>,
    occludes: Vec<bool>,
    dampens: Vec<bool>,
    resonator: Vec<bool>,
    /// The `minecraft:climbable` tag, and the two redstone ores: read for
    /// every mob on every tick.
    climbable: Vec<bool>,
    redstone_ore: Vec<bool>,
}
/// `SculkSensorBlock.getAnalogOutputSignal`: the last vibration's frequency
/// while the sensor is active.
pub(super) fn sensor_signal(
    world: &myriade_engine::world::World,
    behaviours: &horizon_world::level::Behaviours,
    state: BlockStateId,
    pos: BlockPos,
) -> Option<i32> {
    match world.block_entities.get(&(pos.x, pos.y, pos.z)) {
        Some(BlockEntity::Sculk(e))
            if matches!(e.kind, SculkKind::Sensor | SculkKind::Calibrated) =>
        {
            Some(if behaviours.sculk_sensor_active(state) {
                e.last_frequency
            } else {
                0
            })
        }
        _ => None,
    }
}
/// `Entity.moveDist` / `nextStep` and the last state, per entity.
#[derive(Clone, Copy, Debug)]
struct Steps {
    move_dist: f32,
    next_step: f32,
    last: [f64; 3],
    on_ground: bool,
    in_water: bool,
}
#[derive(Default)]
pub(super) struct PlayerEvents {
    steps: Option<Steps>,
    last_health: Option<f32>,
    mount: Option<(u32, [f64; 3])>,
}

#[derive(Default)]
pub(super) struct SculkState {
    /// The player's warning level toward a warden.
    pub tracker: WardenSpawnTracker,
    tables: Tables,
    /// The sculk block entities in the order they were added.
    index: Index,
    pub known: HashSet<(i32, i32)>,
    /// The chunk map's membership revision `known` was taken at.
    pub known_revision: Option<u64>,
    // Keyed by mob ids the server gives out: the fast hasher is safe.
    steps: FxHashMap<Who, Steps>,
    /// Items that have fallen since last touching the ground or water.
    falling_items: HashSet<u32>,
    /// The vehicle each rider had last tick and where it stood.
    mounts: FxHashMap<Who, (u32, [f64; 3])>,
    damaged: FxHashMap<u32, i64>,
    last_health: Option<f32>,
    /// The darkness effect's blend factor, previous and current tick.
    pub darkness: (f32, f32),
    /// Who causes the block changes being processed (the player's actions).
    pub actor: Option<Who>,
    ticks: u32,
    pub tracker_loaded: bool,
}

impl SculkState {
    pub(super) fn swap_player(&mut self, other: &mut PlayerEvents) {
        let steps = self.steps.remove(&Who::Player);
        if let Some(guest) = other.steps.take() {
            self.steps.insert(Who::Player, guest);
        }
        other.steps = steps;
        let mount = self.mounts.remove(&Who::Player);
        if let Some(guest) = other.mount.take() {
            self.mounts.insert(Who::Player, guest);
        }
        other.mount = mount;
        std::mem::swap(&mut self.last_health, &mut other.last_health);
    }

    /// Exchange the level-held parts (its sculk block entities and the
    /// chunks scanned for them) with another level's (game/world/levels.rs).
    pub(super) fn swap_level(&mut self, other: &mut Self) {
        std::mem::swap(&mut self.index, &mut other.index);
        std::mem::swap(&mut self.known, &mut other.known);
    }
}
/// An insertion-ordered set of block positions (`Level.blockEntityTickers`
/// keeps the order block entities were added in).
#[derive(Default)]
struct Index {
    order: Vec<(i32, i32, i32)>,
    set: HashSet<(i32, i32, i32)>,
    /// The positions by 32-block cell, each with its rank: ranks only grow,
    /// so sorting by them gives `order`'s order. Every mob's step posts a
    /// game event, and the listeners near it are read from at most eight
    /// cells instead of the whole level's.
    cells: FxHashMap<(i32, i32, i32), Vec<(u64, (i32, i32, i32))>>,
    next: u64,
}
fn index_cell(key: (i32, i32, i32)) -> (i32, i32, i32) {
    (key.0 >> 5, key.1 >> 5, key.2 >> 5)
}
impl Index {
    fn insert(&mut self, key: (i32, i32, i32)) {
        if self.set.insert(key) {
            self.order.push(key);
            self.cells
                .entry(index_cell(key))
                .or_default()
                .push((self.next, key));
            self.next += 1;
        }
    }
    fn remove(&mut self, key: &(i32, i32, i32)) {
        if self.set.remove(key) {
            self.order.retain(|k| k != key);
            let cell = index_cell(*key);
            if let Some(list) = self.cells.get_mut(&cell) {
                list.retain(|(_, k)| k != key);
                if list.is_empty() {
                    self.cells.remove(&cell);
                }
            }
        }
    }
    /// The positions whose `dist_sqr` to `at` is at most `radius` squared,
    /// in `order`'s order: what filtering `iter` gives.
    fn within(&self, at: [i32; 3], radius: i32) -> Vec<(i32, i32, i32)> {
        let limit = i64::from(radius * radius);
        let span = |v: i32| {
            let cell = |w: i64| (w >> 5).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
            cell(i64::from(v) - i64::from(radius))..=cell(i64::from(v) + i64::from(radius))
        };
        let mut found: Vec<(u64, (i32, i32, i32))> = Vec::new();
        for x in span(at[0]) {
            for y in span(at[1]) {
                for z in span(at[2]) {
                    if let Some(list) = self.cells.get(&(x, y, z)) {
                        found.extend(list.iter().filter(|(_, k)| dist_sqr(at, *k) <= limit));
                    }
                }
            }
        }
        found.sort_unstable_by_key(|(rank, _)| *rank);
        found.into_iter().map(|(_, k)| k).collect()
    }
    fn contains(&self, key: &(i32, i32, i32)) -> bool {
        self.set.contains(key)
    }
    fn iter(&self) -> impl Iterator<Item = &(i32, i32, i32)> {
        self.order.iter()
    }
    fn retain(&mut self, mut keep: impl FnMut(&(i32, i32, i32)) -> bool) {
        self.order.retain(|k| keep(k));
        self.set.retain(|k| keep(k));
        self.cells.retain(|_, list| {
            list.retain(|(_, k)| keep(k));
            !list.is_empty()
        });
    }
}
const KIND_SENSOR: u8 = 1;
const KIND_CALIBRATED: u8 = 2;
const KIND_SHRIEKER: u8 = 3;
const KIND_CATALYST: u8 = 4;

fn center(p: (i32, i32, i32)) -> [f64; 3] {
    [
        f64::from(p.0) + 0.5,
        f64::from(p.1) + 0.5,
        f64::from(p.2) + 0.5,
    ]
}
fn block_of(p: [f64; 3]) -> [i32; 3] {
    [
        p[0].floor() as i32,
        p[1].floor() as i32,
        p[2].floor() as i32,
    ]
}
fn dist_sqr(a: [i32; 3], b: (i32, i32, i32)) -> i64 {
    let d = |x: i32, y: i32| i64::from(x) - i64::from(y);
    d(a[0], b.0).pow(2) + d(a[1], b.1).pow(2) + d(a[2], b.2).pow(2)
}

impl Game<'_> {
    /// Run `f` with the player as the cause of the block changes it makes.
    pub(super) fn with_player_actor<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let prior = self.sculk.actor.replace(Who::Player);
        let out = f(self);
        self.sculk.actor = prior;
        out
    }

    /// The `sculk` control command: listeners and their vibrations, or a
    /// test charge for the nearest catalyst.
    pub(super) fn sculk_command(&mut self, words: &[&str]) -> Result<serde_json::Value> {
        match words.get(1).copied() {
            Some("charge") => {
                let num = |i: usize| -> Result<f64> {
                    Ok(words.get(i).ok_or("missing argument")?.parse::<f64>()?)
                };
                let at = [num(2)?, num(3)?, num(4)?];
                let amount = num(5)? as u32;
                let taken = self.catalyst_takes_experience(at, |_| amount);
                Ok(serde_json::json!({ "ok": true, "taken": taken }))
            }
            Some("dig") => {
                // Let every warden's minute of calm run out.
                self.ecs.expire_dig_cooldowns();
                Ok(serde_json::json!({ "ok": true }))
            }
            Some("warn") => {
                let level = words.get(2).ok_or("missing level")?.parse::<i32>()?;
                self.sculk.tracker.set_warning_level(level);
                Ok(serde_json::json!({ "ok": true }))
            }
            _ => Ok(self.sculk_status()),
        }
    }
    /// The `sculk` control command's listing.
    pub(super) fn sculk_status(&self) -> serde_json::Value {
        let listeners: Vec<serde_json::Value> = self
            .sculk
            .index
            .iter()
            .filter_map(|key| match self.world.block_entities.get(key) {
                Some(BlockEntity::Sculk(e)) => Some(serde_json::json!({
                    "pos": [key.0, key.1, key.2],
                    "kind": e.id(),
                    "current": e.vibration.current.map(|i| i.event),
                    "travel": e.vibration.travel,
                    "last_frequency": e.last_frequency,
                    "warning_level": e.warning_level,
                    "cursors": e.spreader.cursors.len(),
                })),
                _ => None,
            })
            .collect();
        serde_json::json!({
            "listeners": listeners,
            "warning_level": self.sculk.tracker.warning_level,
            "cooldown": self.sculk.tracker.cooldown_ticks,
            "darkness": self.sculk.darkness.1,
            "chunks_known": self.sculk.known.len(),
        })
    }
    /// `WardenSpawnTrackerCommand`: the source player's tracker, cleared or set
    /// to a warning level (`getPlayerOrException`: a mob or the server is refused).
    pub(super) fn cmd_warden_spawn_tracker(&mut self, level: Option<i32>) -> R<i32> {
        self.source_player()?;
        self.load_warden_tracker();
        let key = match level {
            Some(level) => {
                self.sculk.tracker.set_warning_level(level);
                "commands.warden_spawn_tracker.set.success.single"
            }
            None => {
                self.sculk.tracker.reset();
                "commands.warden_spawn_tracker.clear.success.single"
            }
        };
        // `sendFeedback` → `sendSuccess(…, broadcast = true)`: a silent source
        // prints nothing, and the other operators hear it.
        let name = self.source_display_name();
        self.feedback(Text::translate(key, &[name]));
        Ok(1)
    }
    /// `ServerPlayer`'s `warden_spawn_tracker` from the player data.
    pub(super) fn load_warden_tracker(&mut self) {
        if self.sculk.tracker_loaded {
            return;
        }
        self.sculk.tracker_loaded = true;
        let saved = self.level.player_extra.as_ref();
        if let Some(tag) = saved.and_then(|p| p.get("warden_spawn_tracker")) {
            self.sculk.tracker = WardenSpawnTracker::from_nbt(tag);
        }
    }
    /// The tracker as saved, once it has been loaded.
    pub(super) fn warden_tracker_tag(&self) -> Option<horizon_core::nbt::Tag> {
        self.sculk
            .tracker_loaded
            .then(|| self.sculk.tracker.to_nbt())
    }
    /// Write the tracker into the player data being saved. With a LAN owner,
    /// `store_owner` rewrites that from the owner's guest, so it puts the
    /// tracker there as well (`playerdata::with_warden`).
    pub(super) fn save_warden_tracker(&mut self) {
        // Vanilla always stores it: read the saved one before writing.
        self.load_warden_tracker();
        let tag = self.sculk.tracker.to_nbt();
        let extra = self
            .level
            .player_extra
            .get_or_insert_with(|| horizon_core::nbt::Tag::Compound(Default::default()));
        if let horizon_core::nbt::Tag::Compound(map) = extra {
            map.insert("warden_spawn_tracker".into(), tag);
        }
    }
    /// Who is causing the block changes being processed.
    pub(super) fn sculk_actor(&self) -> Option<Who> {
        self.sculk.actor
    }

    /// A game event an ECS entity makes while it moves: its own context (`GameEvent.Context.of(entity, state)`, the state being the block it
    /// stands on for a step or a landing), with the discreet walk and the dampening of its type as P reports them.
    pub(in crate::game) fn post_move_event(&mut self, id: u32, event: &'static str, at: [f64; 3], ground: bool, sneaking: bool, dampens: bool) {
        self.sculk_prepare();
        let state = ground.then(|| match self.main_support(Who::Mob(id)) {
            Some(s) => self.world.block(s.x, s.y, s.z),
            None => self.world.block(at[0].floor() as i32, (at[1] - 0.2).floor() as i32, at[2].floor() as i32),
        });
        let mut ctx = self.who_context(Some(Who::Mob(id)), state);
        ctx.stepping_carefully = sneaking;
        ctx.dampens = dampens;
        self.post_game_event(event, at, ctx);
    }
    /// Whether a vibration may be heard: a sculk block is indexed or a mod hears the game events.
    pub(crate) fn vibration_heard(&self) -> bool {
        !self.sculk.index.set.is_empty() || false
    }

    // ---- setup and the block entity index ----

    pub(super) fn sculk_prepare(&mut self) {
        if self.sculk.tables.built {
            return;
        }
        let registry = self.pack.assets.registry;
        let n = registry.len();
        let tag = |name: &str| {
            self.res
                .items()
                .block_tags
                .get(name)
                .cloned()
                .unwrap_or_default()
        };
        let (wool, dampening, resonators, climbable) = (
            tag("minecraft:occludes_vibration_signals"),
            tag("minecraft:dampens_vibrations"),
            tag("minecraft:vibration_resonators"),
            tag("minecraft:climbable"),
        );
        let mut t = Tables {
            built: true,
            kinds: vec![0; n],
            occludes: vec![false; n],
            dampens: vec![false; n],
            resonator: vec![false; n],
            climbable: vec![false; n],
            redstone_ore: vec![false; n],
        };
        for i in 0..n {
            let facts = self.res.facts().get(BlockStateId(i as u32));
            let name: &str = &facts.name;
            t.kinds[i] = match name {
                "minecraft:sculk_sensor" => KIND_SENSOR,
                "minecraft:calibrated_sculk_sensor" => KIND_CALIBRATED,
                "minecraft:sculk_shrieker" => KIND_SHRIEKER,
                "minecraft:sculk_catalyst" => KIND_CATALYST,
                _ => 0,
            };
            t.occludes[i] = wool.contains(name);
            t.dampens[i] = dampening.contains(name);
            t.resonator[i] = resonators.contains(name);
            t.climbable[i] = climbable.contains(name);
            t.redstone_ore[i] = matches!(
                name,
                "minecraft:redstone_ore" | "minecraft:deepslate_redstone_ore"
            );
        }
        self.world.watched = t.kinds.iter().map(|k| *k != 0).collect();
        self.sculk.tables = t;
    }
    fn kind_of_state(&self, state: u32) -> u8 {
        self.sculk
            .tables
            .kinds
            .get(state as usize)
            .copied()
            .unwrap_or(0)
    }
    fn state_is(&self, table: &[bool], state: BlockStateId) -> bool {
        table.get(state.0 as usize).copied().unwrap_or(false)
    }
    fn entity_for_kind(kind: u8) -> Option<SculkEntity> {
        Some(SculkEntity::new(match kind {
            KIND_SENSOR => SculkKind::Sensor,
            KIND_CALIBRATED => SculkKind::Calibrated,
            KIND_SHRIEKER => SculkKind::Shrieker,
            KIND_CATALYST => SculkKind::Catalyst,
            _ => return None,
        }))
    }
    /// Find the sculk blocks of chunks that just became resident, and apply
    /// the block writes recorded since the last tick.
    pub(super) fn sculk_sync_index(&mut self) {
        // Chunks that arrived or left (every half second is plenty).
        // Nothing to do while no chunk came or went.
        let revision = self.world.chunks.membership_revision();
        if self.sculk.ticks.is_multiple_of(10) && self.sculk.known_revision != Some(revision) {
            self.sculk.known_revision = Some(revision);
            let loaded: HashSet<(i32, i32)> = self.world.chunks.keys().copied().collect();
            let arrived: Vec<(i32, i32)> = loaded.difference(&self.sculk.known).copied().collect();
            for key in arrived {
                self.sculk_scan_chunk(key);
            }
            // The blocks of every chunk that left, in one pass: a player
            // leaving unloads thousands of chunks at once.
            let gone: HashSet<(i32, i32)> = self.sculk.known.difference(&loaded).copied().collect();
            if !gone.is_empty() {
                self.sculk
                    .index
                    .retain(|p| !gone.contains(&(p.0 >> 4, p.2 >> 4)));
            }
            self.sculk.known = loaded;
        }
        self.sculk_sync_writes();
    }

    /// `LevelChunk.setBlockState`: finish sculk placement before a command
    /// or `BlockItem.updateCustomBlockEntityTag` loads saved entity fields.
    pub(super) fn sculk_place_entity(&mut self, state: BlockStateId) -> bool {
        self.sculk_prepare();
        if self.kind_of_state(state.0) == 0 {
            return false;
        }
        self.sculk_sync_writes();
        true
    }

    fn sculk_sync_writes(&mut self) {
        let writes = std::mem::take(&mut self.world.watched_writes);
        for (key, old, new) in writes {
            let (old_kind, new_kind) = (self.kind_of_state(old), self.kind_of_state(new));
            if old_kind == new_kind {
                continue;
            }
            // SculkShriekerBlockEntity.preRemoveSideEffects.
            if old_kind == KIND_SHRIEKER
                && let Some(BlockEntity::Sculk(e)) = self.world.block_entities.get(&key)
                && self.shrieker_shrieking(BlockStateId(old))
            {
                let warning = e.warning_level;
                self.shrieker_respond(key, BlockStateId(old), warning);
            }
            if matches!(
                self.world.block_entities.get(&key),
                Some(BlockEntity::Sculk(_))
            ) {
                self.world.block_entities.remove(&key);
            }
            self.sculk.index.remove(&key);
            if let Some(e) = Self::entity_for_kind(new_kind) {
                self.world
                    .block_entities
                    .insert(key, BlockEntity::Sculk(Box::new(e)));
                self.sculk.index.insert(key);
            }
        }
    }
    fn sculk_scan_chunk(&mut self, key: (i32, i32)) {
        let Some(chunk) = self.world.chunks.get(&key).cloned() else {
            return;
        };
        let mut found = Vec::new();
        for (s, section) in chunk.sections.iter().enumerate() {
            let y0 = self.world.bottom_y() + s as i32 * 16;
            // Without a sculk state in its palette, no block of the section
            // is one: most sections are passed over without reading a block.
            if !section.blocks.maybe_has(|v| self.kind_of_state(v) != 0) {
                continue;
            }
            if section.blocks.uniform().is_some() {
                // A uniform section of sculk blocks does not exist.
                continue;
            }
            let mut a = [0u32; 4096];
            section.unpack(&mut a);
            for (i, v) in a.iter().enumerate() {
                if self.kind_of_state(*v) != 0 {
                    let (x, z, y) = (i & 15, (i >> 4) & 15, i >> 8);
                    found.push((
                        key.0 * 16 + x as i32,
                        y0 + y as i32,
                        key.1 * 16 + z as i32,
                        *v,
                    ));
                }
            }
        }
        for (x, y, z, state) in found {
            let pos = (x, y, z);
            if !matches!(
                self.world.block_entities.get(&pos),
                Some(BlockEntity::Sculk(_))
            ) && let Some(e) = Self::entity_for_kind(self.kind_of_state(state))
            {
                self.world
                    .block_entities
                    .insert(pos, BlockEntity::Sculk(Box::new(e)));
            }
            self.sculk.index.insert(pos);
        }
    }
    fn sculk_entity(&mut self, key: (i32, i32, i32)) -> Option<&mut SculkEntity> {
        match self.world.block_entities.get_mut(&key) {
            Some(BlockEntity::Sculk(e)) => Some(e),
            _ => None,
        }
    }

    // ---- contexts ----

    fn state_dampens(&self, state: Option<BlockStateId>) -> bool {
        state.is_some_and(|s| self.state_is(&self.sculk.tables.dampens, s))
    }
    /// `GameEvent.Context.of(player, state)`.
    pub(super) fn player_context(&self, state: Option<BlockStateId>) -> Context {
        let (dead, creative) = self
            .player
            .as_ref()
            .map_or((true, true), |p| (p.health.dead_or_dying(), self.creative));
        Context {
            source: Some(Who::Player.into()),
            owner: None,
            affected_state: state,
            source_uuid: None,
            owner_uuid: None,
            dampening_state: self.state_dampens(state),
            spectator: self.spectator(),
            stepping_carefully: self.sneak_held,
            dampens: false,
            targetable: !dead && !creative && !self.spectator(),
            living: true,
            player: true,
        }
    }
    /// `GameEvent.Context.of(mob, state)`.
    pub(in crate::game) fn mob_context(&mut self, id: u32, state: Option<BlockStateId>) -> Context {
        self.ecs_mob_context(id, state)
    }
    /// `GameEvent.Context.of(mob, state)` for an ECS mob: its rider and its listening are read from P's components.
    fn ecs_mob_context(&self, id: u32, state: Option<BlockStateId>) -> Context {
        let listens = self.ecs_is_listener(id);
        Context {
            source: Some(Who::Mob(id).into()),
            owner: None,
            affected_state: state,
            source_uuid: None,
            owner_uuid: None,
            dampening_state: self.state_dampens(state),
            spectator: false,
            stepping_carefully: false,
            dampens: listens,
            targetable: self.ecs_view(id).is_some(),
            living: true,
            player: self.ecs_has_rider(id),
        }
    }
    pub(in crate::game) fn who_context(
        &mut self,
        who: Option<Who>,
        state: Option<BlockStateId>,
    ) -> Context {
        match who {
            Some(Who::Player) => self.player_context(state),
            Some(Who::Mob(id)) => self.mob_context(id, state),
            None => Context {
                affected_state: state,
                dampening_state: self.state_dampens(state),
                ..Context::default()
            },
        }
    }

    /// `GameEvent.Context.of(entity, state)` for a non-living entity.
    pub(in crate::game) fn entity_event_context(
        &self,
        id: u32,
        state: Option<BlockStateId>,
    ) -> Context {
        let dampens = self
            .ecs.item(id)
            .is_some_and(|item| {
                world_items!(self)
                    .item_tags
                    .get("minecraft:dampens_vibrations")
                    .is_some_and(|tag| tag.contains(item.stack.item.as_ref()))
            });
        Context {
            source: Some(EventSource::Entity(id)),
            affected_state: state,
            dampening_state: self.state_dampens(state),
            dampens,
            ..Context::default()
        }
    }

    fn event_source_context(
        &mut self,
        source: Option<EventSource>,
        state: Option<BlockStateId>,
    ) -> Context {
        match source {
            Some(EventSource::Entity(id)) => self.entity_event_context(id, state),
            _ => self.who_context(source.and_then(EventSource::actor), state),
        }
    }
    /// `LivingEntity.onEquipItem`'s game event for a mob: EQUIP when the new
    /// stack is equippable in that slot, UNEQUIP when the slot is emptied.
    pub(super) fn mob_equip_event(&mut self, id: u32, at: [f64; 3], equip: bool) {
        let event = if equip {
            "minecraft:equip"
        } else {
            "minecraft:unequip"
        };
        let ctx = self.who_context(Some(Who::Mob(id)), None);
        self.post_game_event(event, at, ctx);
    }

    // ---- posting ----

    /// `Level.gameEvent`: tell the listeners within reach.
    pub(in crate::game) fn post_game_event(
        &mut self,
        event: &'static str,
        pos: [f64; 3],
        ctx: Context,
    ) {
        #[cfg(test)]
        TEST_POSTED_EVENTS.with(|events| {
            if let Some(events) = events.borrow_mut().as_mut() {
                events.push((event, pos, ctx.source.and_then(|source| source.actor())));
            }
        });
        self.post_game_event_with(event, pos, |_| ctx);
    }
    /// [`Self::post_game_event`] with the context worked out only when some
    /// listener may hear the event (`GameEventDispatcher.post` visits only the
    /// listeners registered in the sections around it): most of a server's
    /// mobs are near none. A subscribed mod hook still hears every event first.
    fn post_game_event_with(
        &mut self,
        event: &'static str,
        pos: [f64; 3],
        mut make_ctx: impl FnMut(&mut Self) -> Context,
    ) {
        let ctx = {
            None
        };
        if !self.sculk.tables.built {
            return;
        }
        let at = block_of(pos);
        let radius = sculk::notification_radius(event).min(16);
        let near = self.sculk.index.within(at, radius);
        // The ECS listeners hear the vibration from P's ears: P ranges them by its index, so nothing here scans the mobs.
        let ctx = ctx.unwrap_or_else(|| make_ctx(self));
        let ctx = self.with_vibration_uuids(ctx);
        for key in near {
            self.sculk_listen(key, event, pos, &ctx);
        }
        self.ecs_post_vibration(pos, ctx.source.and_then(|s| s.actor()), ctx.owner, false);
        self.ecs_allays_hear(event, pos);
    }
    /// The context with the saved identities of its source and owner, as
    /// `VibrationInfo`'s constructor takes them from the entities when the
    /// event is heard. Only what the context already names is filled.
    fn with_vibration_uuids(&self, mut ctx: Context) -> Context {
        if ctx.source_uuid.is_none() {
            ctx.source_uuid = self.vibration_uuid(ctx.source.and_then(|source| source.actor()));
        }
        if ctx.owner_uuid.is_none() {
            ctx.owner_uuid = self.vibration_uuid(ctx.owner);
        }
        ctx
    }
    /// The saved UUID of a vibration's mob through the save layer's
    /// `save_who_uuid`. `Who::Player` stays `None`: on a server it can be the
    /// acting guest as well as the host, so it names no single entity here.
    fn vibration_uuid(&self, who: Option<Who>) -> Option<[i32; 4]> {
        match who {
            Some(who @ Who::Mob(_)) => self.save_who_uuid(who),
            _ => None,
        }
    }
    /// A game event at a block (`Level.gameEvent(source, event, pos)`).
    pub(super) fn post_block_game_event(
        &mut self,
        event: &'static str,
        pos: [i32; 3],
        who: Option<Who>,
        state: Option<BlockStateId>,
    ) {
        let ctx = self.who_context(who, state);
        self.post_game_event(event, center((pos[0], pos[1], pos[2])), ctx);
    }
    /// `LivingEntity.actuallyHurt`'s damage vibration, heard as the hit lands;
    /// the end-of-tick scan then knows this damage.
    pub(super) fn mob_damage_event(&mut self, id: u32) {
        // The hit is asked of P this tick: its time is now.
        self.sculk.damaged.insert(id, self.world.game_time);
        self.post_entity_event("minecraft:entity_damage", Who::Mob(id));
    }
    /// A game event of an entity at its position.
    pub(super) fn post_entity_event(&mut self, event: &'static str, who: Who) {
        let at = match who {
            Who::Player => self
                .player
                .as_ref()
                .map(|p| [p.position.x, p.position.y, p.position.z]),
            Who::Mob(id) => self.ecs_view(id).map(|v| v.position),
        };
        if let Some(at) = at {
            self.post_game_event_with(event, at, |g| g.who_context(Some(who), None));
        }
    }
    /// `LevelEvent::GameEvent` from the block behaviour library.
    pub(super) fn sculk_level_event(
        &mut self,
        pos: BlockPos,
        event: &'static str,
        state: Option<BlockStateId>,
    ) {
        let who = self.sculk.actor;
        self.post_block_game_event(event, [pos.x, pos.y, pos.z], who, state);
    }

    fn occluded(&self, origin: [f64; 3], dest: [f64; 3]) -> bool {
        sculk::is_occluded(origin, dest, |p| {
            self.state_is(&self.sculk.tables.occludes, self.world.block(p.x, p.y, p.z))
        })
    }
    /// `VibrationSystem.Listener.handleGameEvent` for a sculk block.
    fn sculk_listen(
        &mut self,
        key: (i32, i32, i32),
        event: &'static str,
        pos: [f64; 3],
        ctx: &Context,
    ) {
        let Some(entity) = self.sculk_entity(key) else {
            self.sculk.index.remove(&key);
            return;
        };
        let (kind, radius, busy) = (
            entity.kind,
            entity.listener_radius(),
            entity.vibration.current.is_some(),
        );
        if kind == SculkKind::Catalyst || busy {
            return;
        }
        let source_block = block_of(pos);
        if dist_sqr(source_block, key) > i64::from(radius * radius) {
            return;
        }
        let listens = match kind {
            SculkKind::Shrieker => sculk::shrieker_can_listen(event),
            _ => sculk::is_vibration(event),
        };
        // VibrationSystem.User.isValidVibration: a player sneaking past a
        // sensor, whose step it ignores, is AVOID_VIBRATION (sensors only;
        // a shrieker cannot trigger it).
        if listens
            && matches!(kind, SculkKind::Sensor | SculkKind::Calibrated)
            && ctx.source == Some(Who::Player.into())
            && !ctx.spectator
            && ctx.stepping_carefully
            && sculk::ignored_when_sneaking(event)
        {
            self.trigger_player("minecraft:avoid_vibration");
        }
        if !sculk::valid_vibration(listens, event, ctx) {
            return;
        }
        let state = self.world.block(key.0, key.1, key.2);
        let can_receive = match kind {
            SculkKind::Sensor | SculkKind::Calibrated => {
                // The sensor does not hear its own placement or removal.
                let own = source_block == [key.0, key.1, key.2]
                    && matches!(event, "minecraft:block_destroy" | "minecraft:block_place");
                let frequency = self.mob_maps().vibration_frequency(event);
                let calibrated_out = kind == SculkKind::Calibrated && {
                    let back = self.calibrated_signal(key);
                    back != 0 && frequency != back
                };
                !own && !calibrated_out
                    && frequency != 0
                    && self.res.behaviours().sculk_sensor_can_activate(state)
            }
            SculkKind::Shrieker => !self.shrieker_shrieking(state) && ctx.player,
            SculkKind::Catalyst => false,
        };
        if !can_receive {
            return;
        }
        let dest = center(key);
        if self.occluded(pos, dest) {
            return;
        }
        let distance = {
            let d = [pos[0] - dest[0], pos[1] - dest[1], pos[2] - dest[2]];
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() as f32
        };
        let time = self.world.game_time;
        if let Some(entity) = self.sculk_entity(key) {
            entity.vibration.schedule(
                Info {
                    event,
                    distance,
                    pos,
                    source: ctx.source,
                    owner: ctx.owner,
                    source_uuid: ctx.source_uuid,
                    owner_uuid: ctx.owner_uuid,
                },
                time,
            );
        }
    }
    fn calibrated_signal(&mut self, key: (i32, i32, i32)) -> i32 {
        let pos = BlockPos::new(key.0, key.1, key.2);
        let mut value = 0;
        self.with_level(|b, level, ticks| {
            value = b.calibrated_back_signal(level, ticks, pos);
        });
        value
    }
    fn shrieker_shrieking(&self, state: BlockStateId) -> bool {
        self.state_property(state, "shrieking") == Some("true")
    }
    fn state_property(&self, state: BlockStateId, key: &str) -> Option<&str> {
        self.pack
            .assets
            .registry
            .state(state)
            .and_then(|s| s.properties.get(key))
            .map(String::as_str)
    }

    // ---- ticking ----

    /// The entity phase of a tick for sculk: the player's warden spawn
    /// tracker (`ServerPlayer.tick`), and the events of entities standing on
    /// and moving over blocks.
    pub(super) fn tick_sculk_entities(&mut self) {
        self.sculk.ticks = self.sculk.ticks.wrapping_add(1);
        // Its steps as spans for `horizon-report` (nothing is read while spans are off).
        let mut laps = myriade_engine::spans::laps();
        self.sculk_prepare();
        laps.lap("sculk_prepare");
        self.load_warden_tracker();
        self.sculk.tracker.tick();
        self.tick_darkness_blend();
        laps.lap("sculk_wardens");
        self.sculk_step_on();
        laps.lap("sculk_step_on");
        self.tick_entity_events();
        laps.lap("sculk_entity_events");
    }
    /// The block entity phase for sculk: vibration tickers and catalysts, in
    /// the order the block entities were added (`Level.tickBlockEntities`).
    pub(super) fn tick_sculk_blocks(&mut self) -> Result<()> {
        self.sculk_prepare();
        self.sculk_sync_index();
        // `ServerLevel.shouldTickBlocksAt` in a block-ticking chunk, from
        // every player's tickets (`update_chunk_tickets`), once the chunk is
        // lit: asked once per chunk, as the sensors share few chunks. Nothing
        // a sculk tick does loads, unloads or lights a chunk or moves a ticket.
        let levels = self.tickets.levels();
        let chunks = &self.world.chunks;
        let pending = &self.world.entities_pending;
        let mut ticking = FxHashMap::<(i32, i32), bool>::default();
        let mut ticks = |cx: i32, cz: i32| {
            *ticking.entry((cx, cz)).or_insert_with(|| {
                levels.block_ticking(cx, cz) && chunks.get(&(cx, cz)).is_some_and(|c| c.lit)
            })
        };
        // The ticking ones in the index's order, with whether the chunks
        // around them tick too. Most sensors and catalysts wait for a
        // vibration or a death: their tick would change nothing, so they
        // are passed over here (a vibration scheduled during these ticks is
        // chosen on the next tick at the earliest, as in vanilla).
        let entities = &self.world.block_entities;
        let due: Vec<((i32, i32, i32), bool)> = self
            .sculk
            .index
            .iter()
            .filter_map(|&key| {
                let (cx, cz) = (key.0 >> 4, key.2 >> 4);
                // `LevelChunk.isTicking`: its own chunk's entities loaded too.
                if !ticks(cx, cz)
                    || pending.contains(&(cx, cz))
                    || matches!(entities.get(&key), Some(BlockEntity::Sculk(e)) if e.idle())
                {
                    return None;
                }
                let adjacent = (-1..=1).all(|dx| (-1..=1).all(|dz| ticks(cx + dx, cz + dz)));
                Some((key, adjacent))
            })
            .collect();
        for (key, adjacent) in due {
            self.tick_sculk_block(key, adjacent)?;
        }
        Ok(())
    }
    fn tick_sculk_block(&mut self, key: (i32, i32, i32), adjacent: bool) -> Result<()> {
        // A mod block's sculk template (a catalyst spreading, a sensor's vibrations) is the
        // template's own: it waits while the block's mod is off.
        let state = self.world.block(key.0, key.1, key.2);
        if crate::game::code_block_owners::suppressed(self.pack.assets.registry, state) {
            return Ok(());
        }
        let time = self.world.game_time;
        let Some(entity) = self.sculk_entity(key) else {
            self.sculk.index.remove(&key);
            return Ok(());
        };
        match entity.kind {
            SculkKind::Catalyst => {
                // SculkCatalystBlockEntity.serverTick.
                let mut spreader = std::mem::take(&mut entity.spreader);
                if !spreader.cursors.is_empty() {
                    let origin = BlockPos::new(key.0, key.1, key.2);
                    self.with_level(|b, level, ticks| {
                        b.sculk_update_cursors(level, ticks, &mut spreader, origin, true);
                    });
                    self.process_level_events()?;
                }
                if let Some(e) = self.sculk_entity(key) {
                    // Cursors added while spreading keep their place.
                    let added = std::mem::take(&mut e.spreader);
                    e.spreader = spreader;
                    for c in added.cursors {
                        e.spreader.add_cursor(c);
                    }
                }
            }
            SculkKind::Sensor | SculkKind::Calibrated | SculkKind::Shrieker => {
                let tick = entity
                    .vibration
                    .tick(time, adjacent, true, |d| d.floor() as i32);
                if let Some((info, travel)) = tick.started {
                    let source = PositionSource::Block([key.0, key.1, key.2]);
                    self.vibration_particle(source, info.pos, center(key), travel);
                }
                if let Some((info, travel)) = tick.resumed {
                    let source = PositionSource::Block([key.0, key.1, key.2]);
                    let dest = center(key);
                    let initial = info.distance.floor() as i32;
                    // `tryReloadVibrationParticle` works in doubles.
                    let alpha = if initial > 0 {
                        1.0 - f64::from(travel) / f64::from(initial)
                    } else {
                        0.0
                    };
                    let current_pos = [
                        info.pos[0] + (dest[0] - info.pos[0]) * alpha,
                        info.pos[1] + (dest[1] - info.pos[1]) * alpha,
                        info.pos[2] + (dest[2] - info.pos[2]) * alpha,
                    ];
                    self.vibration_particle(source, current_pos, dest, travel);
                }
                if let Some(info) = tick.received {
                    self.sculk_receive(key, info)?;
                }
            }
        }
        Ok(())
    }

    // ---- sensors ----

    /// `onReceiveVibration` of a sensor or a shrieker.
    fn sculk_receive(&mut self, key: (i32, i32, i32), info: Info) -> Result<()> {
        let Some(entity) = self.sculk_entity(key) else {
            return Ok(());
        };
        let (kind, radius) = (entity.kind, entity.listener_radius());
        let origin = block_of(info.pos);
        let distance = (dist_sqr(origin, key) as f64).sqrt() as f32;
        let block = match kind {
            SculkKind::Sensor => "minecraft:sculk_sensor",
            SculkKind::Calibrated => "minecraft:calibrated_sculk_sensor",
            SculkKind::Shrieker => "minecraft:sculk_shrieker",
            SculkKind::Catalyst => "minecraft:sculk_catalyst",
        };
        let mut cancel = false;
        if cancel {
            return Ok(());
        }
        match kind {
            SculkKind::Sensor | SculkKind::Calibrated => {
                let state = self.world.block(key.0, key.1, key.2);
                if self.res.behaviours().sculk_sensor_can_activate(state) {
                    let frequency = self.mob_maps().vibration_frequency(info.event);
                    if let Some(e) = self.sculk_entity(key) {
                        e.last_frequency = frequency;
                    }
                    let power = sculk::redstone_strength(distance, radius);
                    self.activate_sensor(key, info.source, power, frequency)?;
                }
            }
            SculkKind::Shrieker => {
                let by_player = info.owner.or(info.source.and_then(EventSource::actor));
                self.try_shriek(
                    key,
                    by_player == Some(Who::Player) || self.rides_player(by_player),
                )?;
            }
            SculkKind::Catalyst => {}
        }
        Ok(())
    }
    fn rides_player(&self, who: Option<Who>) -> bool {
        matches!(who, Some(Who::Mob(id)) if self.ecs_has_rider(id))
    }
    /// `SculkSensorBlock.activate`.
    fn activate_sensor(
        &mut self,
        key: (i32, i32, i32),
        source: Option<EventSource>,
        power: i32,
        frequency: i32,
    ) -> Result<()> {
        let pos = BlockPos::new(key.0, key.1, key.2);
        self.with_level(|b, level, ticks| b.sculk_sensor_activate(level, ticks, pos, power));
        self.process_level_events()?;
        // tryResonateVibration.
        if (1..=15).contains(&frequency) {
            for direction in Direction::ALL {
                let n = direction.normal();
                let relative = (key.0 + n[0], key.1 + n[1], key.2 + n[2]);
                let state = self.world.block(relative.0, relative.1, relative.2);
                if self.state_is(&self.sculk.tables.resonator, state) {
                    let ctx = self.event_source_context(source, Some(state));
                    self.post_game_event(
                        sculk::RESONANCE[frequency as usize - 1],
                        center(relative),
                        ctx,
                    );
                    self.audio.play(
                        "minecraft:block.amethyst_block.resonate",
                        Some(center(relative)),
                        1.0,
                        sculk::resonance_pitch(frequency),
                    );
                }
            }
        }
        let ctx = self.event_source_context(source, None);
        self.post_game_event(sculk::TENDRILS_CLICKING, center(key), ctx);
        self.with_level(|b, level, ticks| b.sculk_sensor_click(level, ticks, pos));
        self.process_level_events()
    }
    /// `SculkSensorBlock.stepOn` and `SculkShriekerBlock.stepOn`: standing on
    /// them is heard (or seen) every tick.
    fn sculk_step_on(&mut self) {
        // No mob is walked here: P emits no step vibration yet.
        self.sculk_step_on_player();
    }
    /// `stepOn` for the player in place: the host, or a guest swapped in
    /// (`guest_server_tick`), as a server has no player of its own.
    pub(super) fn sculk_step_on_player(&mut self) {
        if let Some(p) = self.player.as_ref()
            && p.ground.on_ground
        {
            let at = [p.position.x, p.position.y, p.position.z];
            self.sculk_stand_on(vec![(Who::Player, at)]);
        }
    }
    pub(in crate::game) fn sculk_stand_on(&mut self, standing: Vec<(Who, [f64; 3])>) {
        for (who, at) in standing {
            // getOnPosLegacy: 0.2 below the feet.
            let key = (
                at[0].floor() as i32,
                (at[1] - 0.2).floor() as i32,
                at[2].floor() as i32,
            );
            if !self.sculk.index.contains(&key) {
                continue;
            }
            let state = self.world.block(key.0, key.1, key.2);
            let is_warden = matches!(who, Who::Mob(id) if self.ecs_is_listener(id));
            match self.kind_of_state(state.0) {
                KIND_SENSOR | KIND_CALIBRATED => {
                    if is_warden || !self.res.behaviours().sculk_sensor_can_activate(state) {
                        continue;
                    }
                    let calibrated = self.kind_of_state(state.0) == KIND_CALIBRATED;
                    if calibrated {
                        let back = self.calibrated_signal(key);
                        if back != 0 && back != 1 {
                            continue;
                        }
                    }
                    let dest = center(key);
                    let d = [at[0] - dest[0], at[1] - dest[1], at[2] - dest[2]];
                    let distance = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() as f32;
                    let time = self.world.game_time;
                    let source_uuid = self.vibration_uuid(Some(who));
                    if let Some(e) = self.sculk_entity(key) {
                        e.vibration.schedule(
                            Info {
                                event: "minecraft:step",
                                distance,
                                pos: at,
                                source: Some(who.into()),
                                owner: None,
                                source_uuid,
                                owner_uuid: None,
                            },
                            time,
                        );
                    }
                }
                KIND_SHRIEKER => {
                    let by_player = who == Who::Player || self.rides_player(Some(who));
                    if by_player {
                        let _ = self.try_shriek(key, true);
                    }
                }
                _ => {}
            }
        }
    }

    // ---- shriekers ----

    fn can_respond(&self, state: BlockStateId) -> bool {
        use horizon_world::food::Difficulty;
        self.state_property(state, "can_summon") == Some("true")
            && self.world.health_rules.difficulty != Difficulty::Peaceful
            && self
                .session
                .rules
                .bool_at(myriade_engine::gamerules::vanilla::SPAWN_WARDENS)
    }
    /// `SculkShriekerBlockEntity.tryShriek`.
    fn try_shriek(&mut self, key: (i32, i32, i32), by_player: bool) -> Result<()> {
        if !by_player {
            return Ok(());
        }
        let state = self.world.block(key.0, key.1, key.2);
        if self.shrieker_shrieking(state) {
            return Ok(());
        }
        if let Some(e) = self.sculk_entity(key) {
            e.warning_level = 0;
        }
        if !self.can_respond(state) || self.try_to_warn(key) {
            self.shriek(key)?;
        }
        Ok(())
    }
    /// `WardenSpawnTracker.tryWarn` for the one player.
    fn try_to_warn(&mut self, key: (i32, i32, i32)) -> bool {
        let c = center(key);
        // The wardens within the reach of the 24-block box (its corner), from the index, then the box itself.
        let warden_nearby = self.ecs.ids_near(c, 24.0 * 3f64.sqrt()).into_iter().any(|id| {
            self.ecs_kind(id).as_deref() == Some("minecraft:warden")
                && self.ecs_mob_bounds(id).is_some_and(|b| {
                    let centre = b.center();
                    (centre.x - c[0]).abs() < 24.0
                        && (centre.y - c[1]).abs() < 24.0
                        && (centre.z - c[2]).abs() < 24.0
                })
        });
        // The only player is the one who set it off.
        let mut trackers = [self.sculk.tracker];
        let level = WardenSpawnTracker::try_warn(&mut trackers, warden_nearby);
        self.sculk.tracker = trackers[0];
        match level {
            Some(level) => {
                if let Some(e) = self.sculk_entity(key) {
                    e.warning_level = level;
                }
                true
            }
            None => false,
        }
    }
    /// `SculkShriekerBlockEntity.shriek`.
    fn shriek(&mut self, key: (i32, i32, i32)) -> Result<()> {
        let pos = BlockPos::new(key.0, key.1, key.2);
        self.with_level(|b, level, ticks| b.sculk_shrieker_shriek(level, ticks, pos));
        self.process_level_events()?;
        let ctx = self.player_context(None);
        self.post_game_event(sculk::SHRIEK, center(key), ctx);
        Ok(())
    }
    /// The shriek ended (or the block went away while shrieking).
    pub(super) fn shrieker_respond(
        &mut self,
        key: (i32, i32, i32),
        state: BlockStateId,
        warning: i32,
    ) {
        if !(self.can_respond(state) && warning > 0) {
            return;
        }
        if !self.try_summon_warden(key, warning) {
            self.warden_reply_sound(key, warning);
        }
        self.apply_darkness(center(key), 40.0);
    }
    /// A `shrieker_respond` block event.
    pub(super) fn shrieker_block_event(&mut self, pos: BlockPos, state: BlockStateId) {
        let key = (pos.x, pos.y, pos.z);
        let warning = self.sculk_entity(key).map_or(0, |e| e.warning_level);
        self.shrieker_respond(key, state, warning);
    }
    fn warden_reply_sound(&mut self, key: (i32, i32, i32), warning: i32) {
        let sound = match warning {
            1 => "minecraft:entity.warden.nearby_close",
            2 => "minecraft:entity.warden.nearby_closer",
            3 => "minecraft:entity.warden.nearby_closest",
            4 => "minecraft:entity.warden.listening_angry",
            _ => return,
        };
        let r = &mut self.world.random;
        let mut offset = |v: i32| v + r.next_int_bound(21).unwrap_or(0) - 10;
        let at = [
            f64::from(offset(key.0)),
            f64::from(offset(key.1)),
            f64::from(offset(key.2)),
        ];
        self.audio.play(sound, Some(at), 5.0, 1.0);
    }
    /// `SpawnUtil.trySpawnMob(WARDEN, TRIGGERED, level, pos, 20, 5, 6,
    /// ON_TOP_OF_COLLIDER, false)`: every candidate spot creates a warden
    /// (`EntityType.create` draws its yaw from the level random and
    /// `finalizeSpawn` plays its agitated sound), which is discarded when
    /// `checkSpawnObstruction` fails.
    fn try_summon_warden(&mut self, key: (i32, i32, i32), warning: i32) -> bool {
        if warning < 4 {
            return false;
        }
        self.ecs_try_summon_warden(key)
    }
    /// `try_summon_warden` on an ECS level: the same twenty spots, the first whose box is free gets a warden of P (its finalize is P's).
    fn ecs_try_summon_warden(&mut self, key: (i32, i32, i32)) -> bool {
        for _ in 0..20 {
            let r = &mut self.world.random;
            let dx = r.next_int_bound(11).unwrap_or(0) - 5;
            let dz = r.next_int_bound(11).unwrap_or(0) - 5;
            let Some(at) = self.spawn_position(key.0 + dx, key.1 + 6, key.2 + dz, 6) else {
                continue;
            };
            let spot = [f64::from(at.0) + 0.5, f64::from(at.1), f64::from(at.2) + 0.5];
            // The yaw is drawn from the level random as `EntityType.create` draws it, its value unused here.
            self.world.random.next_float();
            // The warden's box, 0.9 wide and 2.9 high.
            let bounds = Aabb::new(
                Vec3::new(spot[0] - 0.45, spot[1], spot[2] - 0.45),
                Vec3::new(spot[0] + 0.45, spot[1] + 2.9, spot[2] + 0.45),
            );
            if !self.ecs_warden_fits(bounds) {
                continue;
            }
            if self.ecs_with(|ecs, mw| ecs.spawn(mw, "minecraft:warden", spot, None)).is_some() {
                return true;
            }
        }
        false
    }
    /// `SpawnUtil.moveToPossibleSpawnPosition` with `ON_TOP_OF_COLLIDER`.
    fn spawn_position(&self, x: i32, y: i32, z: i32, range: i32) -> Option<(i32, i32, i32)> {
        use myriade_engine::mobs::data;
        let flags = |b: BlockStateId| self.res.mob_data().state(b);
        let mut search = y;
        let mut above = self.world.block(x, search, z);
        for _ in (-range..=range).rev() {
            search -= 1;
            let current = self.world.block(x, search, z);
            let empty_above = flags(above).top == 0.0;
            let full_top = flags(current).flags & data::STURDY_TOP != 0;
            if empty_above && full_top {
                return Some((x, search + 1, z));
            }
            above = current;
        }
        None
    }
    /// The space of an ECS warden's box: the same checks as `warden_fits`, with the ECS entities in the box in place of the mobs.
    fn ecs_warden_fits(&self, bounds: Aabb) -> bool {
        self.ecs_in_box(bounds).is_empty() && self.warden_box_clear(bounds)
    }
    /// The checks of `warden_fits` that every mob shares: no player, no liquid and no block in the box.
    fn warden_box_clear(&self, bounds: Aabb) -> bool {
        let player_blocks = self
            .level_players()
            .iter()
            .any(|p| p.alive && !p.spectator && p.bounds.intersects(bounds));
        if player_blocks {
            return false;
        }
        let others: Vec<myriade_engine::mobs::Seen> = Vec::new();
        let w = self.mob_world_base(&others);
        let min = bounds.min_position();
        let max = bounds.max_position();
        // Level.containsAnyLiquid: floor(min) up to ceil(max), exclusive.
        for x in min.x.floor() as i32..max.x.ceil() as i32 {
            for y in min.y.floor() as i32..max.y.ceil() as i32 {
                for z in min.z.floor() as i32..max.z.ceil() as i32 {
                    if w.fluid(x, y, z) != myriade_engine::blocks::Fluid::None {
                        return false;
                    }
                }
            }
        }
        w.no_collision(bounds)
    }

    // ---- darkness ----

    /// The darkness effect's blend, where this game ticks the player in its
    /// own level (a server's client ticks it in `tick_client_effects`).
    fn tick_darkness_blend(&mut self) {
        self.sculk.darkness = darkness_blend_tick(&self.potions.effects, self.sculk.darkness);
    }
    /// The darkness effect's blend factor at the partial tick.
    pub(super) fn darkness_blend(&self, partial: f32) -> f32 {
        let (previous, current) = self.sculk.darkness;
        previous + (current - previous) * partial
    }
    /// `LightmapRenderStateExtractor`: the night vision effect's intensity
    /// (`GameRenderer.nightVisionScale`), or conduit power's water vision.
    pub(super) fn night_vision_intensity(&self, partial: f32) -> f32 {
        if let Some(e) = self.potions.effects.get("night_vision") {
            // MobEffectInstance.endsWithin(200); -1 is infinite.
            return if e.duration < 0 || e.duration > 200 {
                1.0
            } else {
                0.7 + ((e.duration as f32 - partial) * std::f32::consts::PI * 0.2).sin() * 0.3
            };
        }
        0.0
    }

    /// `Warden.applyDarknessAround` / `MobEffectUtil.addEffectToPlayersAround`.
    pub(in crate::game) fn apply_darkness(&mut self, at: [f64; 3], radius: f64) {
        self.for_each_player(|g| g.darkness_on_player(at, radius));
    }
    fn darkness_on_player(&mut self, at: [f64; 3], radius: f64) {
        if self.creative || self.spectator() {
            return;
        }
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let d = [
            p.position.x - at[0],
            p.position.y - at[1],
            p.position.z - at[2],
        ];
        if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] >= radius * radius {
            return;
        }
        if let Some(active) = self.potions.effects.get("darkness")
            && active.duration > warden::DARKNESS_REAPPLY - 1
        {
            return;
        }
        // `new MobEffectInstance(DARKNESS, 260, 0, false, false)`: no
        // particles and no icon (#11385).
        let effect =
            myriade_engine::effects::EffectInstance::new("darkness", warden::DARKNESS_DURATION, 0)
                .with_flags(false, false);
        self.potions.effects.add(effect);
    }

    // ---- catalysts ----

    /// `SculkCatalystBlockEntity.CatalystListener.handleGameEvent` for the
    /// death of a mob: the nearest catalyst within 8 blocks takes its
    /// experience. Returns whether it was consumed.
    pub(super) fn catalyst_takes_experience(
        &mut self,
        at: [f64; 3],
        experience: impl FnOnce(&mut Self) -> u32,
    ) -> bool {
        // GameEventDispatcher: catalysts are BY_DISTANCE listeners, handled
        // nearest first (exact distance to the block centre); a tie keeps the
        // order they were visited in: chunk x, chunk z, section y, then the
        // order they registered in. The first one takes the experience.
        let source = block_of(at);
        let mut candidates: Vec<(f64, (i32, i32, i32), usize, (i32, i32, i32))> = Vec::new();
        for (order, key) in self.sculk.index.iter().copied().enumerate() {
            if dist_sqr(source, key) > 64 {
                continue;
            }
            let is_catalyst = matches!(
                self.world.block_entities.get(&key),
                Some(BlockEntity::Sculk(e)) if e.kind == SculkKind::Catalyst
            );
            if is_catalyst {
                let c = center(key);
                let d = (at[0] - c[0]).powi(2) + (at[1] - c[1]).powi(2) + (at[2] - c[2]).powi(2);
                candidates.push((d, (key.0 >> 4, key.2 >> 4, key.1 >> 4), order, key));
            }
        }
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        let nearest = candidates.first().map(|c| (c.3, ()));
        let Some((key, _)) = nearest else {
            return false;
        };
        let xp = experience(self);
        if xp > 0 {
            // sourcePosition.relative(UP, 0.5)
            let start = BlockPos::new(source[0], (at[1] + 0.5).floor() as i32, source[2]);
            if let Some(e) = self.sculk_entity(key) {
                e.spreader.add_cursors(start, xp as i32);
            }
        }
        self.catalyst_bloom(key);
        true
    }
    fn catalyst_bloom(&mut self, key: (i32, i32, i32)) {
        let pos = BlockPos::new(key.0, key.1, key.2);
        self.with_level(|b, level, ticks| b.sculk_catalyst_bloom(level, ticks, pos));
        let _ = self.process_level_events();
        let c = center(key);
        let pitch = 0.6 + self.world.random.next_float() * 0.4;
        self.audio
            .play("minecraft:block.sculk_catalyst.bloom", Some(c), 2.0, pitch);
    }

    // ---- particles ----

    /// `LevelEvent` 3006: charge cursors moving over sculk, or popping.
    pub(super) fn sculk_charge_event(&mut self, pos: BlockPos, data: i32) {
        let c = center((pos.x, pos.y, pos.z));
        let count = data >> 6;
        if count > 0 {
            // Client-side (LevelEventHandler / particles): the client random.
            let mut r = cosmetic_random();
            if r.next_float() < 0.3 + count as f32 * 0.1 {
                let volume = 0.15 + 0.02 * (count * count) as f32 * r.next_float();
                let pitch = 0.4 + 0.3 * count as f32 * r.next_float();
                self.audio
                    .play("minecraft:block.sculk.charge", Some(c), volume, pitch);
            }
        } else {
            self.audio
                .play("minecraft:block.sculk.charge", Some(c), 1.0, 1.0);
        }
    }
    /// `ServerLevel.sendParticles(new VibrationParticleOption(...), origin,
    /// 1, 0, 0, 0, 0)` from `VibrationSystem.Ticker.trySelectAndScheduleVibration`:
    /// the window draws it when it is a client, and every player within 32
    /// blocks is sent it.
    fn vibration_particle(
        &mut self,
        source: PositionSource,
        from: [f64; 3],
        to: [f64; 3],
        ticks: i32,
    ) {
        let options = ParticleOptions::vibration(source, to, ticks);
        self.lan_send_particles(super::lan_particles::QueuedParticle {
            dimension: self.dim.id(),
            options,
            state: None,
            at: from,
            spread: [0.0; 3],
            speed: [0.0; 3],
            count: 1,
            randomization: 0,
        });
    }
    /// A vibration travelling to a warden or allay (`MobEvent::Vibration`).
    pub(super) fn mob_vibration_particle(
        &mut self,
        source: horizon_protocol::entity_data::PositionSource,
        from: [f64; 3],
        to: [f64; 3],
        ticks: i32,
    ) {
        let source = super::lan_particles::particle_position_source(&source);
        self.vibration_particle(source, from, to, ticks);
    }
    // ---- entity events ----

    /// `Projectile.shoot` / `onHit`: a projectile of `owner` at a position.
    pub(super) fn projectile_event(
        &mut self,
        event: &'static str,
        owner: Option<Who>,
        at: [f64; 3],
    ) {
        let mut ctx = Context::default();
        if event == "minecraft:projectile_shoot" {
            // The shooter is the source of the shot.
            ctx = self.who_context(owner, None);
        } else {
            ctx.owner = owner;
            ctx.player = owner == Some(Who::Player) || self.rides_player(owner);
        }
        self.post_game_event(event, at, ctx);
    }
    /// Retain the projectile as the landing event source and the
    /// shooter as its owner, even when the projectile is held out for ticking.
    pub(in crate::game) fn projectile_landed_event(
        &mut self,
        id: u32,
        owner: Option<Who>,
        at: [f64; 3],
    ) {
        let ctx = Context {
            source: (id != 0).then_some(EventSource::Entity(id)),
            owner,
            player: owner == Some(Who::Player) || self.rides_player(owner),
            ..Context::default()
        };
        self.post_game_event("minecraft:projectile_land", at, ctx);
    }
    /// A projectile's own event (`gameEvent(event)`, `Context.of(this, state)`):
    /// its owner as the projectile's owner, `state` the affected state. A
    /// guest's projectile posts as theirs, or with no owner once they have left
    /// (`Projectile.getOwner`), as its shot does.
    pub(super) fn projectile_own_event(
        &mut self,
        event: &'static str,
        owner: Option<Who>,
        guest: Option<u32>,
        at: [f64; 3],
        state: Option<BlockStateId>,
    ) {
        if let Some(conn) = guest.filter(|conn| owner.is_some() && self.lan_acting != Some(*conn)) {
            let posted = self
                .as_guest(conn, |g| {
                    g.projectile_own_event(event, owner, None, at, state);
                })
                .is_some();
            if !posted {
                self.projectile_own_event(event, None, None, at, state);
            }
            return;
        }
        let mut ctx = Context {
            dampening_state: self.state_dampens(state),
            ..Context::default()
        };
        ctx.owner = owner;
        ctx.player = owner == Some(Who::Player) || self.rides_player(owner);
        self.post_game_event(event, at, ctx);
    }
    /// The state HIT_GROUND names for an entity of `bounds` landing at
    /// `position` (`Entity.checkFallDamage`): its supporting block's
    /// (`mainSupportingBlockPos`), else the block at `getOnPosLegacy`.
    pub(super) fn landing_state(&self, position: Vec3, bounds: Aabb) -> BlockStateId {
        let p = self.landing_pos(position, bounds);
        self.world.block(p.x, p.y, p.z)
    }
    /// Where [`Self::landing_state`] reads its block: the supporting block,
    /// else `getOnPosLegacy`.
    pub(super) fn landing_pos(&self, position: Vec3, bounds: Aabb) -> BlockPos {
        self.supporting_block(position, bounds).unwrap_or_else(|| {
            BlockPos::new(
                position.x.floor() as i32,
                (position.y - 0.2).floor() as i32,
                position.z.floor() as i32,
            )
        })
    }
    /// The player shot or threw something.
    pub(super) fn player_projectile_shot(&mut self) {
        if let Some(p) = self.player.as_ref() {
            let at = [
                p.position.x,
                p.position.y + f64::from(self.eye),
                p.position.z,
            ];
            self.projectile_event("minecraft:projectile_shoot", Some(Who::Player), at);
        }
    }

    /// Events from entities moving, getting hurt and dying, derived from what
    /// changed this tick.
    fn tick_entity_events(&mut self) {
        self.tick_player_sculk_events();
        if !self.sculk.falling_items.is_empty() {
            let present: HashSet<u32> = self.ecs.item_views().iter().map(|item| item.id).collect();
            self.sculk.falling_items.retain(|id| present.contains(id));
        }
        // Dropped items are entities too: Entity.applyEffectsFromBlocks, from
        // ItemEntity.tick, so only for the items in entity-ticking chunks.
        let ticking = self.entity_ticking_area();
        let items: Vec<[f64; 3]> = self
            .ecs.item_views()
            .iter()
            .filter(|i| {
                i.on_ground && ticking(i.position.x.floor() as i32, i.position.z.floor() as i32)
            })
            .map(|i| [i.position.x, i.position.y, i.position.z])
            .collect();
        for at in items {
            self.step_on_redstone_ore(at, false, false);
        }
    }

    /// `Entity.checkFallDamage` for item movement: retain a fall across an
    /// exact-height contact, without changing the saved fall-distance getter.
    pub(super) fn item_landing_event(
        &mut self,
        id: u32,
        previous: Vec3,
        position: Vec3,
        on_ground: bool,
    ) {
        let at = [position.x, position.y, position.z];
        if self.in_water_at(at) {
            self.sculk.falling_items.remove(&id);
            return;
        }
        if position.y < previous.y {
            self.sculk.falling_items.insert(id);
        }
        if on_ground && self.sculk.falling_items.remove(&id) {
            // EntityType.ITEM: 0.25 by 0.25.
            let bounds = Aabb::new(
                Vec3::new(position.x - 0.125, position.y, position.z - 0.125),
                Vec3::new(position.x + 0.125, position.y + 0.25, position.z + 0.125),
            );
            self.post_landing_event(position, bounds, false);
        }
    }

    /// `Entity.checkFallDamage` emits HIT_GROUND after the block's fallOn.
    pub(in crate::game) fn post_landing_event(
        &mut self,
        position: Vec3,
        bounds: Aabb,
        living: bool,
    ) {
        self.sculk_prepare();
        let state = self.landing_state(position, bounds);
        let mut context = self.who_context(None, Some(state));
        context.living = living;
        self.post_game_event(
            "minecraft:hit_ground",
            [position.x, position.y, position.z],
            context,
        );
    }

    /// Entity.move emits on the authoritative server too, for each ServerPlayer.
    pub(super) fn tick_player_sculk_events(&mut self) {
        // The player.
        let player = self.player.as_ref().map(|p| {
            (
                [p.position.x, p.position.y, p.position.z],
                p.ground.on_ground,
                p.health.health(),
                p.flying,
            )
        });
        if let Some((at, on_ground, health, flying)) = player {
            let crouch = self.sneak_held;
            if on_ground && !self.spectator() {
                self.step_on_redstone_ore(at, crouch, true);
            }
            self.mount_events(Who::Player, self.riding, true);
            let in_water = self.in_water_at(at);
            self.entity_motion_events(
                Who::Player,
                at,
                on_ground,
                in_water,
                crouch && on_ground,
                flying,
                false,
            );
            if let Some(last) = self.sculk.last_health
                && health < last
            {
                self.post_entity_event("minecraft:entity_damage", Who::Player);
            }
            self.sculk.last_health = Some(health);
        }
    }

    /// `RedStoneOreBlock.stepOn`: standing on the ore lights it, unless the
    /// entity is stepping carefully (`attack_block` only lights an unlit ore).
    fn step_on_redstone_ore(&mut self, at: [f64; 3], carefully: bool, _burst: bool) {
        if carefully {
            return;
        }
        // getOnPosLegacy: 0.2 below the feet.
        let pos = [
            at[0].floor() as i32,
            (at[1] - 0.2).floor() as i32,
            at[2].floor() as i32,
        ];
        self.sculk_prepare();
        let state = self.world.block(pos[0], pos[1], pos[2]);
        if self.state_is(&self.sculk.tables.redstone_ore, state) {
            let _ = self.attack_block(pos);
        }
    }

    /// `RedStoneOreBlock.useItemOn`: a right-click lights the ore whatever the
    /// hand holds. PASS when the hand holds a block item, which goes on
    /// to be placed; SUCCESS otherwise.
    pub(super) fn use_redstone_ore(
        &mut self,
        pos: [i32; 3],
        held: Option<&myriade_engine::items::ItemStack>,
    ) -> Result<Option<myriade_types::InteractionResult>> {
        use myriade_types::InteractionResult as Answer;
        if !matches!(
            &*self.block_name(pos),
            "minecraft:redstone_ore" | "minecraft:deepslate_redstone_ore"
        ) {
            return Ok(None);
        }
        self.attack_block(pos)?;
        let block_item = held.is_some_and(|s| {
            self.res
                .items()
                .get(&s.item)
                .is_some_and(|i| i.block.is_some())
        });
        Ok(Some(if block_item {
            Answer::Pass
        } else {
            Answer::Success
        }))
    }

    /// Where the vehicle `id` stands: a mob, boat, minecart or cushion.
    fn vehicle_position(&mut self, id: u32) -> Option<[f64; 3]> {
        if let Some(p) = self.ecs_view(id).map(|v| v.position) {
            return Some(p);
        }
        if let Some(p) = self.ecs.position(id) {
            return Some(p);
        }
        let p = self.cushion_saved(id)?.position;
        Some([p.x, p.y, p.z])
    }
    /// `Entity.startRiding` and `removeVehicle`: `entity_mount` at the new
    /// vehicle and `entity_dismount` at the old one, derived from the vehicle
    /// the rider has now and the one it had last tick. `announce` is false
    /// for a rider first seen riding.
    fn mount_events(&mut self, who: Who, vehicle: Option<u32>, announce: bool) {
        let last = self.sculk.mounts.get(&who).copied();
        // Riding nothing, as last tick: nothing to tell or keep.
        if vehicle.is_none() && last.is_none() {
            return;
        }
        let now = vehicle.and_then(|id| self.vehicle_position(id));
        if last.map(|l| l.0) != vehicle {
            if let Some((_, at)) = last {
                let ctx = self.who_context(Some(who), None);
                self.post_game_event("minecraft:entity_dismount", at, ctx);
            }
            if let (true, Some(at)) = (announce, now) {
                let ctx = self.who_context(Some(who), None);
                self.post_game_event("minecraft:entity_mount", at, ctx);
            }
        }
        // A vehicle that is gone keeps the spot it was last seen at.
        let kept = last.filter(|l| Some(l.0) == vehicle).map(|l| l.1);
        match (vehicle, now.or(kept)) {
            (Some(id), Some(at)) => {
                self.sculk.mounts.insert(who, (id, at));
            }
            _ => {
                self.sculk.mounts.remove(&who);
            }
        }
    }
    fn in_water_at(&self, at: [f64; 3]) -> bool {
        let (x, y, z) = (
            at[0].floor() as i32,
            at[1].floor() as i32,
            at[2].floor() as i32,
        );
        self.res.facts().get(self.world.block(x, y, z)).fluid
            == myriade_engine::blocks::Fluid::Water
    }
    /// `Entity.checkSupportingBlock`: the block under this box closest to the
    /// entity. Everything but the player works it out when it asks (Breeze ground dust too).
    pub(in crate::game) fn supporting_block(
        &self,
        position: Vec3,
        bounds: Aabb,
    ) -> Option<BlockPos> {
        let (min, max) = (bounds.min_position(), bounds.max_position());
        let foot = Aabb::new(
            Vec3::new(min.x, min.y - 1.0e-6, min.z),
            Vec3::new(max.x, min.y, max.z),
        );
        let mut scene = myriade_engine::collisions::SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        horizon_world::ground_support::find_supporting_block(
            &mut scene,
            horizon_world::collision_context::CollisionContext::position(position.y),
            position,
            foot,
            1_000_000,
        )
        .ok()
        .flatten()
    }
    /// `Entity.mainSupportingBlockPos` of a player or mob on the ground.
    fn main_support(&mut self, who: Who) -> Option<BlockPos> {
        match who {
            Who::Player => self.player.as_ref()?.ground.supporting_block,
            Who::Mob(id) => {
                let p = self.ecs_view(id)?.position;
                let bounds = self.ecs_mob_bounds(id)?;
                self.supporting_block(Vec3::new(p[0], p[1], p[2]), bounds)
            }
        }
    }
    /// `Entity.move`'s emission: footsteps, swimming, landing and splashes.
    fn entity_motion_events(
        &mut self,
        who: Who,
        at: [f64; 3],
        on_ground: bool,
        in_water: bool,
        discrete: bool,
        flying: bool,
        warden: bool,
    ) {
        self.sculk_prepare();
        // Standing as it stood with no step due (a mob outside the simulation
        // distance, or one at rest): nothing below would change or be heard.
        if let Some(e) = self.sculk.steps.get(&who)
            && e.last == at
            && e.on_ground == on_ground
            && e.in_water == in_water
            && e.move_dist <= e.next_step
        {
            return;
        }
        let entry = self.sculk.steps.entry(who).or_insert(Steps {
            move_dist: 0.0,
            next_step: 1.0,
            last: at,
            on_ground,
            in_water,
        });
        let previous = *entry;
        entry.last = at;
        entry.on_ground = on_ground;
        entry.in_water = in_water;
        let (dx, dy, dz) = (
            at[0] - previous.last[0],
            at[1] - previous.last[1],
            at[2] - previous.last[2],
        );
        // Entity.checkFallDamage: touching down after falling, with the state
        // of mainSupportingBlockPos (the block's state at getOnPosLegacy without one).
        if on_ground && (!previous.on_ground || dy < -1.0e-6) && dy < 0.0 && !in_water {
            let state = match self.main_support(who) {
                Some(support) => self.world.block(support.x, support.y, support.z),
                None => self.world.block(
                    at[0].floor() as i32,
                    (at[1] - 0.2).floor() as i32,
                    at[2].floor() as i32,
                ),
            };
            self.post_game_event_with("minecraft:hit_ground", at, |g| {
                g.who_context(Some(who), Some(state))
            });
        }
        // Entity.updateInWaterStateAndDoFluidPushing: entering water splashes.
        if in_water && !previous.in_water {
            self.post_entity_event("minecraft:splash", who);
        }
        // Player.getMovementEmission.
        if flying || discrete {
            return;
        }
        let horizontal = ((dx * dx + dz * dz).sqrt() * f64::from(0.6f32)) as f32;
        // Entity.getOnPos without a supporting block. Finding a mob's support
        // waits for a step that is due: this runs for every mob on every tick.
        let unsupported = self.world.block(
            at[0].floor() as i32,
            (at[1] - 1.0e-5).floor() as i32,
            at[2].floor() as i32,
        );
        let climbable = self.state_is(&self.sculk.tables.climbable, unsupported);
        let travelled = if climbable {
            ((dx * dx + dy * dy + dz * dz).sqrt() * f64::from(0.6f32)) as f32
        } else {
            horizontal
        };
        let Some(entry) = self.sculk.steps.get_mut(&who) else {
            return;
        };
        entry.move_dist += travelled;
        let move_dist = entry.move_dist;
        if move_dist <= entry.next_step {
            return;
        }
        // Entity.applyMovementEmissionAndPlaySound: the state at getOnPos, so a
        // carpet and not the block under it.
        let state = match on_ground.then(|| self.main_support(who)).flatten() {
            Some(support) => self.world.block(support.x, support.y, support.z),
            None => unsupported,
        };
        let air = self.res.facts().is_air(state);
        let Some(entry) = self.sculk.steps.get_mut(&who) else {
            return;
        };
        if !air {
            let steps_on_block = on_ground || climbable;
            if steps_on_block {
                entry.next_step = if warden {
                    // Warden.nextStep
                    move_dist + 0.55
                } else {
                    (move_dist as i32).wrapping_add(1) as f32
                };
                self.post_game_event_with("minecraft:step", at, |g| {
                    g.who_context(Some(who), Some(state))
                });
            } else if in_water {
                entry.next_step = (move_dist as i32).wrapping_add(1) as f32;
                self.post_entity_event("minecraft:swim", who);
            }
        }
    }
    /// `Entity.kill` for an entity that is no mob (`BlockAttachedEntity.kill`,
    /// `ArmorStand.kill`, `VehicleEntity.destroy`): removed, then
    /// `gameEvent(ENTITY_DIE, attributedTo != null ? attributedTo : this)` at
    /// the entity's position. `by_player`: the player's hit killed it, so the
    /// player is the source. Nonliving self sources have no Who handle;
    /// their listener facts are represented by the default context.
    pub(super) fn entity_killed_event(&mut self, at: [f64; 3], by_player: bool) {
        self.entity_killed_event_by(at, by_player.then_some(super::commands::Target::Player));
    }
    /// `BlockAttachedEntity.kill` / `ArmorStand.kill` with DamageSource.getEntity.
    pub(super) fn entity_killed_event_by(
        &mut self,
        at: [f64; 3],
        source: Option<super::commands::Target>,
    ) {
        self.entity_event_by("minecraft:entity_die", at, source);
    }
    /// `Entity.gameEvent(event, attributedTo)` at `at`, attributed to `source`
    /// (a LAN guest posts as that player), else to the entity itself.
    pub(super) fn entity_event_by(
        &mut self,
        event: &'static str,
        at: [f64; 3],
        source: Option<super::commands::Target>,
    ) {
        use super::commands::Target;
        if let Some(Target::Guest(conn)) = source {
            if self.lan_acting == Some(conn) {
                let ctx = self.player_context(None);
                self.post_game_event(event, at, ctx);
                return;
            }
            let posted = self
                .as_guest(conn, |g| {
                    let ctx = g.player_context(None);
                    g.post_game_event(event, at, ctx);
                })
                .is_some();
            if posted {
                return;
            }
        }
        let who = match source {
            Some(Target::Player) => Some(Who::Player),
            Some(Target::Mob(id)) => Some(Who::Mob(id)),
            _ => None,
        };
        let mut ctx = self.who_context(who, None);
        // A stand is living, but Warden.canTargetEntity explicitly excludes it.
        ctx.living |= matches!(source, Some(Target::Stand(_)));
        self.post_game_event(event, at, ctx);
    }
}

/// `MobEffects.DARKNESS`'s `setBlendDuration(22)`: its fade in, its fade out,
/// and how long before the end the fade out starts.
const DARKNESS_BLEND: i32 = 22;

/// `MobEffectInstance.BlendState.hasEffect` for the darkness effect.
fn darkness_target(e: &EffectInstance) -> f32 {
    if e.ends_within(DARKNESS_BLEND) {
        0.0
    } else {
        1.0
    }
}

/// `MobEffectInstance.BlendState.tick` for the darkness effect: from the
/// (previous, current) factors, the next ones. It fades in and out over 22
/// ticks; with no darkness it is gone.
pub(in crate::game) fn darkness_blend_tick(
    effects: &PlayerEffects,
    (_, current): (f32, f32),
) -> (f32, f32) {
    let next = match effects.get("darkness") {
        Some(e) => {
            let step = 1.0 / DARKNESS_BLEND as f32;
            current + (darkness_target(e) - current).clamp(-step, step)
        }
        None => 0.0,
    };
    (current, next)
}

/// `Bee.isFlapping` (flying, every other tick) and `Bat.isFlapping`
/// (awake, every tenth tick). Bird distance thresholds run in mob movement.
fn flaps(species: Species, resting: bool, on_ground: bool, age: u32) -> bool {
    match species {
        Species::Bee => !on_ground && age.is_multiple_of(2),
        Species::Bat => !resting && age.is_multiple_of(10),
        _ => false,
    }
}

fn event_cause(source: Option<EventSource>, acting_player: Option<u32>) -> Option<u32> {
    match source {
        Some(EventSource::Player) => acting_player,
        Some(EventSource::Mob(id) | EventSource::Entity(id)) => Some(id),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_event_cause_uses_acting_guest() {
        assert_eq!(event_cause(Some(Who::Player.into()), Some(42)), Some(42));
        assert_eq!(event_cause(Some(Who::Player.into()), None), None);
        assert_eq!(event_cause(Some(Who::Mob(7).into()), Some(42)), Some(7));
        assert_eq!(
            event_cause(Some(EventSource::Entity(99)), Some(42)),
            Some(99)
        );
        assert_eq!(event_cause(None, Some(42)), None);
    }

    #[test]
    #[ignore = "daily"]
    fn dropped_wool_dampens_its_own_vibrations() {
        use crate::game::native_entity_refs::tests::with_game;
        use myriade_engine::{chunk::Chunk, items::ItemStack};
        use std::sync::Arc;

        with_game(|game| {
            game.sculk_prepare();
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            let sensor = game
                .pack
                .assets
                .registry
                .default_state("minecraft:sculk_sensor")
                .unwrap();
            game.world.set(1, 64, 1, sensor).unwrap();
            game.sculk_sync_index();
            game.spawn_item(
                ItemStack::new("minecraft:white_wool", 1),
                [3.5, 64.0, 1.5],
                [0.0; 3],
                0,
            );
            let id = game.ecs.item_ids()[0];
            let entity = game.entity(id).unwrap();
            assert!(game.entity_game_event(entity, "minecraft:step"));
            assert!(
                game.sculk_entity((1, 64, 1))
                    .unwrap()
                    .vibration
                    .selector
                    .current
                    .is_none()
            );
            let id = game.ecs.item_ids()[0];
        game.ecs.set_item_stack(id, ItemStack::new("minecraft:stone", 1));
            assert!(game.entity_game_event(entity, "minecraft:step"));
            let (info, _) = game
                .sculk_entity((1, 64, 1))
                .unwrap()
                .vibration
                .selector
                .current
                .unwrap();
            assert_eq!(info.source, Some(EventSource::Entity(id)));
            game.sculk_entity((1, 64, 1))
                .unwrap()
                .vibration
                .selector
                .current = None;
            game.projectile_landed_event(123, Some(Who::Player), [3.5, 64.0, 1.5]);
            let (info, _) = game
                .sculk_entity((1, 64, 1))
                .unwrap()
                .vibration
                .selector
                .current
                .unwrap();
            assert_eq!(info.source, Some(EventSource::Entity(123)));
            assert_eq!(info.owner, Some(Who::Player));
        });
    }

    /// Retaining keeps the insertion order and the membership in step.
    #[test]
    fn the_index_retains_in_order() {
        let mut index = Index::default();
        for key in [(5, 0, 0), (40, 1, 0), (6, 2, 17), (-3, 3, 0), (41, 4, 0)] {
            index.insert(key);
        }
        index.retain(|p| p.0 >> 4 != 2 && p.2 >> 4 != 1);
        assert_eq!(index.order, [(5, 0, 0), (-3, 3, 0)]);
        assert!(index.contains(&(5, 0, 0)) && !index.contains(&(41, 4, 0)));
        assert_eq!(index.set.len(), 2);
        index.insert((41, 4, 0));
        assert_eq!(index.iter().last(), Some(&(41, 4, 0)));
    }

    /// The listeners near a point come out as filtering the whole index in
    /// order would give them, through inserts, removals and retains.
    #[test]
    fn listeners_within_a_radius_are_the_filtered_index_in_order() {
        let mut index = Index::default();
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = move |n: i32| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % (2 * n as u64 + 1)) as i32 - n
        };
        for step in 0..3000 {
            let key = (next(70), next(40), next(70));
            match step % 7 {
                0..=3 => index.insert(key),
                4..=5 => {
                    let picked = index
                        .order
                        .get(next(1000).unsigned_abs() as usize % index.order.len().max(1))
                        .copied();
                    if let Some(k) = picked {
                        index.remove(&k);
                    }
                }
                _ if step % 500 == 6 => index.retain(|p| (p.0 ^ p.2) & 3 != 0),
                _ => {}
            }
            let at = [next(80), next(50), next(80)];
            let radius = next(16).abs();
            let expected: Vec<_> = index
                .iter()
                .copied()
                .filter(|p| dist_sqr(at, *p) <= i64::from(radius * radius))
                .collect();
            assert_eq!(
                index.within(at, radius),
                expected,
                "step {step} at {at:?} r {radius}"
            );
        }
        assert!(index.cells.values().map(Vec::len).sum::<usize>() == index.order.len());
    }

    /// A bat flaps every tenth tick while it is not resting, whether it is on
    /// the ground or not; a resting bat never flaps and nothing else does.
    #[test]
    fn a_bat_flaps_every_tenth_tick_unless_it_is_resting() {
        assert!(flaps(Species::Bat, false, false, 0));
        assert!(flaps(Species::Bat, false, true, 10));
        assert!(!flaps(Species::Bat, false, false, 11));
        assert!(!flaps(Species::Bat, true, false, 20), "resting");
        assert!(flaps(Species::Bee, false, false, 4));
        assert!(!flaps(Species::Bee, false, true, 4), "a bee on the ground");
        assert!(!flaps(Species::Bat, false, false, 5));
        assert!(!flaps(Species::Cow, false, false, 0));
    }

    #[test]
    fn centres_and_blocks_agree() {
        assert_eq!(center((1, 2, 3)), [1.5, 2.5, 3.5]);
        assert_eq!(block_of([1.5, 2.5, -0.5]), [1, 2, -1]);
        assert_eq!(dist_sqr([0, 0, 0], (3, 4, 0)), 25);
    }
}

#[cfg(test)]
thread_local! {
    pub(super) static TEST_POSTED_EVENTS: std::cell::RefCell<Option<Vec<(&'static str, [f64; 3], Option<Who>)>>> = const { std::cell::RefCell::new(None) };
}


