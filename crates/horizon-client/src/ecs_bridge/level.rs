//! The ECS world of one dimension as the game holds it (`Game.ecs`, `Parked.ecs`): each method of `EcsWorld` in turn, and an
//! inert level on a client (`inner` None), which answers as an empty world: none, false, nothing. Reads and writes go to the
//! world; no state is copied here.
use super::state;
use crate::lan::entity::EntityState;
use crate::mobs::{MobTypes, MobWorld};
use bevy_ecs::{entity::Entity, world::World};
use horizon_core::{geometry::Vec3, nbt::Compound};
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
use horizon_protocol::entity_data::Value;

pub struct EcsLevel {
    inner: Option<Box<EcsWorld>>,
}

impl EcsLevel {
    /// The level of a client, which runs no world.
    pub fn inert() -> Self {
        Self { inner: None }
    }
    pub fn new(setup: &HostSetup, mw: &MobWorld<'_>, types: &'static MobTypes, tags: &Path) -> Self {
        Self { inner: Some(Box::new(EcsWorld::new(setup, mw, types, tags))) }
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
    /// `EcsWorld::spawn_natural`: a creature of the natural spawning pass.
    pub fn spawn_natural(&mut self, mw: &MobWorld<'_>, kind: &str, at: [f64; 3]) -> Option<Entity> {
        self.inner.as_mut()?.spawn_natural(mw, kind, at)
    }
    pub fn despawn(&mut self, e: Entity) {
        if let Some(w) = self.inner.as_mut() {
            w.despawn(e);
        }
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
        self.inner.as_mut()?.load_entity(mw, c)
    }
    pub fn entity_nbt(&self, e: Entity) -> Option<Compound> {
        self.inner.as_ref()?.entity_nbt(e)
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
