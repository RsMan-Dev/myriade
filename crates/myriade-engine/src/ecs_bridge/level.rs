//! The ECS world of one dimension as the game holds it (`Game.ecs`, `Parked.ecs`): each method of `EcsWorld` in turn, and an
//! inert level on a client (`inner` None), which answers as an empty world: none, false, nothing. Reads and writes go to the
//! world; no state is copied here.
use super::state;
use crate::lan::entity::EntityState;
use crate::mobs::{MobTypes, MobWorld, spawn_place::{StructureAt, StructureSpawns}};
use horizon_ecs_proto::spawn_structures::{StructureHost, StructureSource};
use horizon_core::bounding_box::BoundingBox;
use horizon_core::fast_hash::FxHashMap;
use bevy_ecs::{entity::Entity, world::World};
use horizon_core::{geometry::Vec3, nbt::{Compound, Tag}};
use horizon_ecs_proto::{
    behaviours::ender_teleport,
    bridge::{
        host::{EcsWorld, HostSetup, HurtSource, PlayerFact, Select},
        sync,
        view::view,
    },
    goals,
    goals::{Player, Slot},
    host_out::HostEvent,
    mount,
    projectile::Blast,
    targeting::Who,
};
use std::path::Path;
use crate::mobs::entity_sounds::{Selection, Sounds};
use horizon_ecs_proto::sound_slots::{self, Overrides, Slot};

/// The sounds the mods declare per type (`MobType.sound_overrides`), as the slots the ECS reads: one table per load, shared by all entities.
/// One row per type: the declared slots, and the species' own hurt voice where the declaration inherits it (`sounds[1]`). The death
/// is left inherited: `Mob.die` plays no sound on the server (the client plays `getDeathSound` on the death event).
fn mod_sound_table(types: &'static MobTypes) -> std::collections::HashMap<&'static str, Overrides> {
    types
        .types
        .iter()
        .map(|(id, t)| {
            let mut o = overrides(&t.sound_overrides);
            if o.hurt == Slot::Inherit && !t.sounds[1].is_empty() {
                o.hurt = Slot::Id(t.sounds[1]);
            }
            (id.as_str(), o)
        })
        .collect()
}

fn overrides(s: &Sounds) -> Overrides {
    let slot = |sel: &Selection| match *sel {
        Selection::Inherit => Slot::Inherit,
        Selection::Mute => Slot::Mute,
        Selection::Id(id) => Slot::Id(id),
    };
    Overrides { ambient: slot(&s.ambient), hurt: slot(&s.hurt), death: slot(&s.death), step: slot(&s.step) }
}

#[cfg(test)]
mod sound_table_tests {
    use super::*;

    #[test]
    fn a_declared_slot_maps_to_its_id_or_mute_and_the_others_inherit() {
        let row = serde_json::json!({ "sounds": { "hurt": "bird:hurt", "death": "none" } });
        let s = Sounds::load("bird:call", &row).unwrap();
        let o = overrides(&s);
        assert_eq!(o.hurt, Slot::Id("bird:hurt"));
        assert_eq!(o.death, Slot::Mute);
        assert_eq!(o.ambient, Slot::Inherit, "an undeclared slot keeps the species' own");
        assert_eq!(o.step, Slot::Inherit);
    }

    #[test]
    fn a_vanilla_type_without_a_declared_hurt_plays_its_own_hurt_and_keeps_the_death_inherited() {
        let (_, types, _) = myriade_engine::mobs::world::leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
        let table = mod_sound_table(types);
        let cow = types.types.get("minecraft:cow").expect("the cow type");
        assert_eq!(table["minecraft:cow"].hurt, Slot::Id(cow.sounds[1]), "the species' own hurt voice");
        assert_eq!(table["minecraft:cow"].death, Slot::Inherit, "the death voice is the client's");
    }
}

/// The flags a saved mob carries that are components: the AI pause (`ai_paused`) and `CanPickUpLoot`.
fn restore_mob_flags(w: &mut EcsWorld, e: Entity, c: &Compound) {
    w.set_ai_paused(e, crate::entity_nbt::ai_paused_saved(c));
    if let Some(Tag::Byte(on)) = c.get(&"CanPickUpLoot".into()) {
        w.set_can_pick_up_loot(e, *on != 0);
    }
}
/// The mod goals of a loaded entity, by the species saved in its `id` (`mod_goals::attach`).
fn attach_saved_goals(w: &mut EcsWorld, e: Entity, c: &Compound) {
    if let Some(Tag::String(id)) = c.get(&"id".into()) {
        super::mod_goals::attach(w.world_mut(), e, &id.to_string_lossy());
    }
}
/// `c` and its riders (`Passengers`, depth first, root first): the compounds in the order `load_entity_tree` makes their entities.
fn ride_compounds<'a>(c: &'a Compound, out: &mut Vec<&'a Compound>) {
    out.push(c);
    if let Some(Tag::List(riders)) = c.get(&"Passengers".into()) {
        for t in riders {
            if let Tag::Compound(p) = t {
                ride_compounds(p, out);
            }
        }
    }
}
use horizon_protocol::entity_data::Value;

pub struct EcsLevel {
    inner: Option<Box<EcsWorld>>,
    /// The calendar date (month, day) last read for `SpecialDates`, and when it was read; `refresh_date` rereads it once a minute.
    pub(super) date: Option<(std::time::Instant, [u32; 2])>,
    /// The structures whose references reach each loaded chunk, as the game pushed them (`set_chunk_structures`).
    structures: ChunkStructures,
}

/// A structure start a loaded chunk refers to, owned: its id, its box and its pieces' boxes as placed (`reaches_in` reads the same).
#[derive(Clone, Debug)]
pub struct ChunkStart {
    pub id: String,
    pub bounds: BoundingBox,
    pub pieces: Vec<BoundingBox>,
}

/// The structure starts of the loaded chunks, owned: the structure host reads them during a step without the game.
#[derive(Clone, Default)]
pub struct ChunkStructures(std::sync::Arc<std::sync::RwLock<FxHashMap<(i32, i32), Vec<ChunkStart>>>>);

impl StructureSource for ChunkStructures {
    /// `reaches_in`: a start reaches the position by its box or one of its pieces; the reach merges by id.
    fn structures_at(&self, pos: [i32; 3], wanted: &dyn Fn(&str) -> bool) -> Vec<StructureAt> {
        let chunks = self.0.read().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut out: Vec<StructureAt> = Vec::new();
        for start in chunks.get(&(pos[0] >> 4, pos[2] >> 4)).into_iter().flatten().filter(|s| wanted(&s.id)) {
            let in_bounds = start.bounds.is_inside(pos);
            let in_piece = start.pieces.iter().any(|p| p.is_inside(pos));
            if !in_bounds && !in_piece {
                continue;
            }
            match out.iter_mut().find(|r| r.id == start.id) {
                Some(r) => {
                    r.in_bounds |= in_bounds;
                    r.in_piece |= in_piece;
                }
                None => out.push(StructureAt { id: start.id.clone(), in_piece, in_bounds }),
            }
        }
        out
    }
}

impl EcsLevel {
    /// The level of a client, which runs no world.
    pub fn inert() -> Self {
        Self { inner: None, date: None, structures: ChunkStructures::default() }
    }
    pub fn new(setup: &HostSetup, mw: &MobWorld<'_>, types: &'static MobTypes, tags: &Path) -> Self {
        let mut level = Self { inner: Some(Box::new(EcsWorld::new(setup, mw, types, tags))), date: None, structures: ChunkStructures::default() };
        if let Some(w) = level.world_mut() {
            sound_slots::replace(w, mod_sound_table(types));
        }
        level
    }
    /// The natural spawning of this level (`install_spawning`): the biome tables of `mw` with `seed`, the rules, and the structures' lists
    /// and source. Nothing when inert.
    pub fn enable_spawning(&mut self, mw: &MobWorld<'_>, seed: i64, rules: (bool, bool), spawns: &StructureSpawns) {
        if let Some(w) = self.inner.as_mut() {
            let host = StructureHost::new(std::sync::Arc::new(self.structures.clone()), spawns);
            horizon_ecs_proto::cow::herd::install_spawning(w.world_mut(), mw, seed, rules, Some(host));
        }
    }
    /// The mods' natural spawning policy of this level (`spawning::Policy`). Nothing when inert.
    pub fn set_spawn_policy(&mut self, policy: horizon_ecs_proto::spawning::Policy) {
        if let Some(w) = self.inner.as_mut() {
            w.world_mut().insert_resource(policy);
        }
    }
    /// The structure starts of chunk `chunk`, pushed when it loads (`[]` when it unloads). Stored without a step.
    pub fn set_chunk_structures(&mut self, chunk: [i32; 2], refs: Vec<ChunkStart>) {
        let mut chunks = self.structures.0.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        if refs.is_empty() {
            chunks.remove(&(chunk[0], chunk[1]));
        } else {
            chunks.insert((chunk[0], chunk[1]), refs);
        }
    }
    pub fn is_active(&self) -> bool {
        self.inner.is_some()
    }
    pub fn ecs(&self) -> Option<&EcsWorld> {
        self.inner.as_deref()
    }
    pub fn ecs_mut(&mut self) -> Option<&mut EcsWorld> {
        self.inner.as_deref_mut()
    }
    /// The world, for reads of components; None when inert.
    pub fn world(&self) -> Option<&World> {
        self.inner.as_ref().map(|w| w.world())
    }
    pub fn world_mut(&mut self) -> Option<&mut World> {
        self.inner.as_mut().map(|w| w.world_mut())
    }
    pub fn spawn(&mut self, mw: &MobWorld<'_>, kind: &str, at: [f64; 3], nbt: Option<&Compound>) -> Option<Entity> {
        self.inner.as_mut()?.spawn(mw, kind, at, nbt)
    }
    /// `EcsWorld::spawn_for`: a creature spawned for `reason` (`EntitySpawnReason`).
    pub fn spawn_for(&mut self, mw: &MobWorld<'_>, kind: &str, at: [f64; 3], nbt: Option<&Compound>, reason: horizon_ecs_proto::spawn_reason::SpawnReason) -> Option<Entity> {
        self.inner.as_mut()?.spawn_for(mw, kind, at, nbt, reason)
    }
    /// `EcsWorld::spawn_prepared`: a creature made from `sheet`, its edits applied, not finalized yet (`finalize_prepared` does it).
    pub fn spawn_prepared(
        &mut self,
        mw: &MobWorld<'_>,
        kind: &str,
        at: [f64; 3],
        finalize_at: Option<[f64; 3]>,
        reason: horizon_ecs_proto::spawn_reason::SpawnReason,
        sheet: &horizon_ecs_proto::spawn_sheet::SpawnSheet,
    ) -> Option<Entity> {
        self.inner.as_mut()?.spawn_prepared(mw, kind, at, finalize_at, reason, sheet)
    }
    /// `EcsWorld::finalize_prepared`: the species' `finalizeSpawn` of `e` for `reason`, with the pack's `group`; None when inert.
    pub fn finalize_prepared(
        &mut self,
        mw: &MobWorld<'_>,
        e: Entity,
        reason: horizon_ecs_proto::spawn_reason::SpawnReason,
        special: f32,
        group: Option<horizon_ecs_proto::spawn_finalize::Group>,
    ) -> Option<horizon_ecs_proto::spawn_finalize::Outcome> {
        Some(self.inner.as_mut()?.finalize_prepared(mw, e, reason, special, group))
    }
    /// `EcsWorld::announce_prepared`: `Spawned` for `e`, once it is finalized; nothing when inert.
    pub fn announce_prepared(&mut self, mw: &MobWorld<'_>, e: Entity, reason: horizon_ecs_proto::spawn_reason::SpawnReason) {
        if let Some(w) = self.inner.as_mut() {
            w.announce_prepared(mw, e, reason);
        }
    }
    /// `spawn_chunk_packs`: the creature packs of a generated chunk, from the world's spawn tables; `random` is the region's. Nothing when inert.
    pub fn spawn_chunk_packs(&mut self, mw: &MobWorld<'_>, cx: i32, cz: i32, random: &mut impl horizon_core::random::RandomSource) {
        if let Some(w) = self.world_mut() {
            horizon_ecs_proto::spawn_generation::spawn_chunk_packs(w, mw, cx, cz, random);
        }
    }
    pub fn despawn(&mut self, e: Entity) {
        if let Some(w) = self.inner.as_mut() {
            w.despawn(e);
        }
    }
    /// `EcsWorld::set_difficulty`: the level's difficulty; peaceful removes the creatures not allowed there. Nothing when inert.
    pub fn set_difficulty(&mut self, id: u8) {
        if let Some(w) = self.inner.as_mut() {
            w.set_difficulty(id);
        }
    }
    /// `EcsWorld::set_spawn_rules`: `SPAWN_MOBS` and `SPAWN_MONSTERS`. Nothing when inert.
    pub fn set_spawn_rules(&mut self, spawn_mobs: bool, monsters: bool) {
        if let Some(w) = self.inner.as_mut() {
            w.set_spawn_rules(spawn_mobs, monsters);
        }
    }
    /// `EcsWorld::set_chunk_spawnable`: a chunk enters or leaves the chunks that may take a creature. Nothing when inert.
    pub fn set_chunk_spawnable(&mut self, cx: i32, cz: i32, on: bool) {
        if let Some(w) = self.inner.as_mut() {
            w.set_chunk_spawnable(cx, cz, on);
        }
    }
    /// `EcsWorld::set_world_border`: `[min_x, min_z, max_x, max_z]`, or none. Nothing when inert.
    pub fn set_world_border(&mut self, border: Option<[f64; 4]>) {
        if let Some(w) = self.inner.as_mut() {
            w.set_world_border(border);
        }
    }
    /// `EcsWorld::set_world_spawn`: the world spawn. Nothing when inert.
    pub fn set_world_spawn(&mut self, at: [i32; 3]) {
        if let Some(w) = self.inner.as_mut() {
            w.set_world_spawn(at);
        }
    }
    /// `EcsWorld::alert_guards`: the guards within reach of `player` that are at rest turn on it (`PiglinAi.angerNearbyPiglins`). 0 when inert.
    pub fn alert_guards(&mut self, player: u32, only_if_seen: bool) -> usize {
        self.inner.as_mut().map_or(0, |w| w.alert_guards(player, only_if_seen))
    }
    /// `EcsWorld::set_trap`: arms or disarms the trap of a mount (`SkeletonHorse.setTrap`). False when inert or when it springs none.
    pub fn set_trap(&mut self, e: Entity, on: bool) -> bool {
        self.inner.as_mut().is_some_and(|w| w.set_trap(e, on))
    }
    pub fn entity_of(&self, id: u32) -> Option<Entity> {
        self.inner.as_ref()?.entity_of(id)
    }
    /// The network id of `e`, linked by `alloc` when it has none; 0 when inert.
    pub fn id_for(&mut self, e: Entity, alloc: &mut dyn FnMut() -> u32) -> u32 {
        self.inner.as_mut().map_or(0, |w| w.id_for(e, alloc))
    }
    pub fn drain_modified(&mut self) -> Vec<Entity> {
        self.inner.as_mut().map_or_else(Vec::new, |w| w.drain_modified())
    }
    pub fn set_player(&mut self, p: Player, state: PlayerFact) {
        if let Some(w) = self.inner.as_mut() {
            w.set_player(p, state);
        }
    }
    /// The network ids of the followed entities removed since the last call (`EcsWorld::drain_removed`, a P function still missing).
    pub fn drain_removed(&mut self) -> Vec<i32> {
        self.inner.as_mut().map_or_else(Vec::new, |w| w.drain_removed().into_iter().map(|id| id as i32).collect())
    }
    pub fn remove_player(&mut self, id: u32) {
        if let Some(w) = self.inner.as_mut() {
            w.remove_player(id);
        }
    }
    pub fn hurt(&mut self, mw: &MobWorld<'_>, e: Entity, amount: f32, source: HurtSource) {
        if let Some(w) = self.inner.as_mut() {
            w.hurt(mw, e, amount, source);
        }
    }
    pub fn interact(&mut self, mw: &MobWorld<'_>, e: Entity, player: u32, hand: u8) -> bool {
        self.inner.as_mut().is_some_and(|w| w.interact(mw, e, player, hand))
    }
    /// `interact` with the whole stack used (its components and wear) in the rules' reach.
    pub fn interact_with(&mut self, mw: &MobWorld<'_>, e: Entity, player: u32, hand: u8, stack: myriade_engine::items::ItemStack, creative: bool) -> bool {
        self.inner.as_mut().is_some_and(|w| w.interact_with(mw, e, player, hand, stack, creative))
    }
    pub fn teleport(&mut self, mw: &MobWorld<'_>, e: Entity, at: [f64; 3]) {
        if let Some(w) = self.inner.as_mut() {
            w.teleport(mw, e, at);
        }
    }
    pub fn set_block_event(&mut self, pos: [i32; 3]) {
        if let Some(w) = self.inner.as_mut() {
            w.set_block_event(pos);
        }
    }
    pub fn take_player_hits(&mut self) -> Vec<(u32, f32, [f64; 3], Option<Entity>)> {
        self.inner.as_mut().map_or_else(Vec::new, |w| w.take_player_hits())
    }
    pub fn take_player_pushes(&mut self) -> Vec<(u32, [f64; 3], (f64, f64, f64), Entity)> {
        self.inner.as_mut().map_or_else(Vec::new, |w| w.take_player_pushes())
    }
    pub fn chunk_rewritten(&mut self, cx: i32, cz: i32) {
        if let Some(w) = self.inner.as_mut() {
            w.chunk_rewritten(cx, cz);
        }
    }
    pub fn chunk_loaded(&mut self, mw: &MobWorld<'_>, cx: i32, cz: i32) {
        if let Some(w) = self.inner.as_mut() {
            w.chunk_loaded(mw, cx, cz);
        }
    }
    pub fn chunk_unloaded(&mut self, mw: &MobWorld<'_>, cx: i32, cz: i32) {
        if let Some(w) = self.inner.as_mut() {
            w.chunk_unloaded(mw, cx, cz);
        }
    }
    pub fn save_chunk(&mut self, cx: i32, cz: i32) -> Vec<Compound> {
        self.inner.as_mut().map_or_else(Vec::new, |w| w.save_chunk(cx, cz))
    }
    pub fn load_entity(&mut self, mw: &MobWorld<'_>, c: &Compound) -> Option<Entity> {
        let w = self.inner.as_mut()?;
        let e = w.load_entity(mw, c)?;
        restore_mob_flags(w, e, c);
        attach_saved_goals(w, e, c);
        Some(e)
    }
    /// `load_entity` for a mount and its riders: the entity of each compound of the NBT, in its order (None: not made). Only the root's
    /// saved flags are restored here.
    pub fn load_entity_tree(&mut self, mw: &MobWorld<'_>, c: &Compound) -> Vec<Option<Entity>> {
        let Some(w) = self.inner.as_mut() else { return Vec::new() };
        let made = w.load_entity_tree(mw, c);
        if let Some(Some(e)) = made.first() {
            restore_mob_flags(w, *e, c);
        }
        let mut compounds = Vec::new();
        ride_compounds(c, &mut compounds);
        for (c, e) in compounds.into_iter().zip(&made) {
            if let Some(e) = e {
                attach_saved_goals(w, *e, c);
            }
        }
        made
    }
    /// The ids of `root` and its riders all the way down, in the order `Passengers` is written (root first, depth first).
    pub fn ride_tree(&self, root: u32) -> Vec<u32> {
        let (Some(e), Some(w)) = (self.entity_of(root), self.ecs()) else { return Vec::new() };
        w.ride_tree(e).into_iter().filter_map(|r| w.id_of(r)).collect()
    }
    pub fn entity_nbt(&self, e: Entity) -> Option<Compound> {
        let w = self.inner.as_ref()?;
        let mut tag = Tag::Compound(w.entity_nbt(e)?);
        crate::entity_nbt::write_ai_paused(w.ai_paused(e), &mut tag);
        let Tag::Compound(mut c) = tag else { return None };
        if let Some(on) = w.can_pick_up_loot(e) {
            c.insert("CanPickUpLoot".into(), Tag::Byte(i8::from(on)));
        }
        Some(c)
    }
    pub fn merge_nbt(&mut self, e: Entity, patch: &Compound) -> bool {
        self.inner.as_mut().is_some_and(|w| w.merge_nbt(e, patch))
    }
    pub fn select(&self, q: &Select) -> Vec<Entity> {
        self.inner.as_ref().map_or_else(Vec::new, |w| w.select(q))
    }
    /// Runs the schedule once with `mw` lent for the call; the host events it wrote come back.
    pub fn step(&mut self, mw: &MobWorld<'_>) -> Vec<HostEvent> {
        self.inner.as_mut().map_or_else(Vec::new, |w| w.step(mw))
    }
    /// `step` with the mods' goal and behaviour hooks answered by `goals`, and the natural pass's spawn hooks by `spawn` (None: no mod listens).
    pub fn step_with(&mut self, mw: &MobWorld<'_>, goals: &dyn horizon_ecs_proto::mod_goals::ModGoalHost, spawn: Option<&dyn horizon_ecs_proto::spawn_mod_host::SpawnModHost>) -> Vec<HostEvent> {
        self.inner.as_mut().map_or_else(Vec::new, |w| w.step_with(mw, goals, spawn))
    }
    /// Writes the tracker's state of `e` under the network id `id`; false when inert, or when `e` is asleep or has no position.
    pub fn view_state(&self, e: Entity, id: i32, out: &mut EntityState) -> bool {
        let Some(w) = self.world() else { return false };
        let Some(v) = view(w, e) else { return false };
        state::fill(&v, id, w, out);
        true
    }
    pub fn set_persistent(&mut self, e: Entity) {
        if let Some(w) = self.inner.as_mut() {
            w.set_persistent(e);
        }
    }
    pub fn equip(&mut self, e: Entity, slot: Slot, item: u32) {
        if let Some(w) = self.inner.as_mut() {
            w.equip(e, slot, item);
        }
    }
    pub fn mount(&mut self, mw: &MobWorld<'_>, rider: Entity, vehicle: Entity) -> bool {
        self.inner.as_mut().is_some_and(|w| w.mount(mw, rider, vehicle))
    }
    pub fn drop_equipment(&mut self, mw: &MobWorld<'_>, e: Entity, seed: i64) -> Vec<(Slot, u32, i32)> {
        self.inner.as_mut().map_or_else(Vec::new, |w| w.drop_equipment(mw, e, seed))
    }
    /// `goals::set_velocity` (goals.rs:1587), on the world.
    pub fn set_velocity(&mut self, e: Entity, v: Vec3) {
        if let Some(w) = self.inner.as_mut() {
            goals::set_velocity(w.world_mut(), e, v);
        }
    }
    /// `ender_teleport::land` (behaviours/ender_teleport.rs:104), on the world.
    pub fn land(&mut self, e: Entity, at: Vec3) {
        if let Some(w) = self.inner.as_mut() {
            ender_teleport::land(w.world_mut(), e, at);
        }
    }
    /// `EcsWorld::explode` (bridge/host_blast.rs): the blast of `explosion::explode` (explosion.rs:130) over the lent mob world.
    pub fn explode(&mut self, mw: &MobWorld<'_>, b: Blast) {
        if let Some(w) = self.inner.as_mut() {
            w.explode(mw, b);
        }
    }
    /// `EcsWorld::explode_with` (bridge/host_blast.rs): as `explode`, the blocks stay when `breaks_blocks` is false.
    pub fn explode_with(&mut self, mw: &MobWorld<'_>, b: Blast, breaks_blocks: bool) {
        if let Some(w) = self.inner.as_mut() {
            w.explode_with(mw, b, breaks_blocks);
        }
    }
    /// `EcsWorld::splashed` (bridge/host_blast.rs): water reaches `e`.
    pub fn splashed(&mut self, mw: &MobWorld<'_>, e: Entity) {
        if let Some(w) = self.inner.as_mut() {
            w.splashed(mw, e);
        }
    }
    /// `mount::ride` (mount.rs:140): `who` takes a seat of `vehicle`.
    pub fn ride(&mut self, vehicle: Entity, who: Who) -> bool {
        self.inner.as_mut().is_some_and(|w| mount::ride(w.world_mut(), vehicle, who))
    }
    /// `mount::dismount` (mount.rs:195).
    pub fn dismount(&mut self, vehicle: Entity) -> Option<Who> {
        self.inner.as_mut().and_then(|w| mount::dismount(w.world_mut(), vehicle))
    }
    /// `mount::boost` (mount.rs:249).
    pub fn boost(&mut self, e: Entity) -> bool {
        self.inner.as_mut().is_some_and(|w| mount::boost(w.world_mut(), e))
    }
    /// `mount::modify_temper` (mount.rs:242).
    pub fn modify_temper(&mut self, e: Entity, amount: i32) {
        if let Some(w) = self.inner.as_mut() {
            mount::modify_temper(w.world_mut(), e, amount);
        }
    }
    /// `mount::eject_passengers` (mount.rs:441).
    pub fn eject_passengers(&mut self, vehicle: Entity) {
        if let Some(w) = self.inner.as_mut() {
            mount::eject_passengers(w.world_mut(), vehicle);
        }
    }
    /// The entity data of `e` as the tracker sends it: (index, serializer id, value).
    pub fn metadata(&self, e: Entity, out: &mut Vec<(u8, i32, Value)>) {
        let Some(w) = self.world() else { return };
        let mut raw = Vec::new();
        sync::sync_entries(w, e, &mut raw);
        out.extend(raw.into_iter().map(|(i, v)| {
            let (serializer, value) = sync::to_entry(&v);
            (i, serializer, value)
        }));
    }
}

#[cfg(test)]
mod structure_tests {
    use super::*;

    /// A position of the chunk outside the piece reads the start as its bounds only (the biome list); inside the piece, as the piece (the structure list).
    #[test]
    fn position_outside_piece_is_bounds_only_inside_piece_is_in_piece() {
        let store = ChunkStructures::default();
        let bounds = BoundingBox::new([0, 60, 0], [31, 80, 31]);
        let piece = BoundingBox::new([0, 60, 0], [7, 70, 7]);
        store.0.write().unwrap().insert((0, 0), vec![ChunkStart { id: "minecraft:village".into(), bounds, pieces: vec![piece] }]);
        let any = |_: &str| true;
        let outside = store.structures_at([10, 64, 10], &any);
        assert_eq!(outside.len(), 1);
        assert!(outside[0].in_bounds && !outside[0].in_piece);
        let inside = store.structures_at([3, 64, 3], &any);
        assert!(inside[0].in_piece);
        assert!(store.structures_at([40, 64, 40], &any).is_empty());
    }
}
