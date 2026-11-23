//! The boats through the bridge (`boat_hurt.rs`): what the host calls when a boat is hit, used or opened, and what it reads of its chest.
//! The calls that may run vanilla's code take the `MobWorld` the same way `seat` does.
use crate::bridge::host::EcsWorld;
use crate::boat_hurt::{self, HitResult, HurtFlags, UseResult};
use crate::targeting::Who;
use bevy_ecs::entity::Entity;
use myriade_engine_lib::items::ItemStack;
use myriade_engine_lib::mobs::MobWorld;

impl EcsWorld {
    /// The game rule `ENTITY_DROPS` for the vehicles a blast breaks (`boat_hurt::EntityDrops`).
    pub fn set_entity_drops(&mut self, on: bool) {
        self.world_mut().insert_resource(boat_hurt::EntityDrops(on));
    }
    /// With `on`, a blast leaves its blows on the boats in `take_boat_blows` for the host to play through its own hit; off, P hits them itself.
    pub fn route_boat_blows(&mut self, on: bool) {
        self.world_mut().get_resource_or_insert_with(boat_hurt::BoatBlows::default).host = on;
    }
    /// The blows queued since the last call: entity, damage, attacker.
    pub fn take_boat_blows(&mut self) -> Vec<(Entity, f32, Option<Who>)> {
        self.world_mut().get_resource_mut::<boat_hurt::BoatBlows>().map_or_else(Vec::new, |mut q| std::mem::take(&mut q.queue))
    }
    /// The item entities a blast hurts not (`explosion::ExplosionSpared`), set before the blast and cleared by it.
    pub fn spare_from_blast(&mut self, spared: Vec<Entity>) {
        self.world_mut().insert_resource(crate::explosion::ExplosionSpared(spared));
    }
    /// The stack of the item entity `e` (None when it is no item).
    pub fn item_stack(&self, e: Entity) -> Option<ItemStack> {
        self.world().get::<crate::item::Item>(e).map(|i| i.stack.clone())
    }
    /// A hit on boat `e` by `attacker` (the host has run the invulnerability checks); `entity_drops` is `ENTITY_DROPS`. See `boat_hurt::hit`.
    pub fn boat_hit(&mut self, mw: &MobWorld<'_>, e: Entity, amount: f32, attacker: Option<Who>, creative: bool, entity_drops: bool) -> HitResult {
        self.lent(mw, |w| boat_hurt::hit(w, e, amount, attacker, creative, HurtFlags { invulnerable: false, entity_drops }))
    }
    /// A right click of `player` on boat `e` with the item of `hand`: a lead, a seat or the chest (`sneaking` opens it). `out_of_control`
    /// and `ridden_by_guest` are the host's checks on the seats.
    pub fn boat_use(&mut self, mw: &MobWorld<'_>, e: Entity, player: u32, hand: u8, sneaking: bool, out_of_control: bool, ridden_by_guest: bool) -> UseResult {
        self.lent(mw, |w| boat_hurt::boat_use(w, e, player, hand, sneaking, out_of_control, ridden_by_guest))
    }
    pub fn boat_items(&self, e: Entity) -> Option<&[Option<ItemStack>]> {
        boat_hurt::items(self.world(), e)
    }
    pub fn boat_set_slot(&mut self, e: Entity, slot: usize, stack: Option<ItemStack>) {
        boat_hurt::set_slot(self.world_mut(), e, slot, stack);
    }
    /// The loot table and seed of the chest boat, once; the host rolls them.
    pub fn boat_take_loot(&mut self, e: Entity) -> Option<(String, i64)> {
        boat_hurt::take_loot(self.world_mut(), e)
    }
}
