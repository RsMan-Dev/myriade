//! The boats of a dimension's ECS level by network id (`boat_hurt.rs`): the hit, the use, the chest's slots and its loot. An inert level
//! answers as an empty world: `Ignored`, `Pass`, none.
use super::level::EcsLevel;
use crate::mobs::MobWorld;
use bevy_ecs::entity::Entity;
use myriade_engine_lib::items::ItemStack;
use horizon_ecs_proto::boat_hurt::{HitResult, UseResult};
use horizon_ecs_proto::targeting::Who;

impl EcsLevel {
    /// The game rule `ENTITY_DROPS` for the vehicles a blast breaks; a no-op when inert.
    pub fn set_entity_drops(&mut self, on: bool) {
        if let Some(b) = self.ecs_mut() {
            b.set_entity_drops(on);
        }
    }
    /// A blast leaves its blows on the boats for `take_boat_blows` (the host plays them through its ordinary hit); no-op when inert.
    pub fn route_boat_blows(&mut self, on: bool) {
        if let Some(b) = self.ecs_mut() {
            b.route_boat_blows(on);
        }
    }
    /// The blows queued by a blast: boat id, damage, attacker.
    pub fn take_boat_blows(&mut self) -> Vec<(u32, f32, Option<Who>)> {
        let Some(blows) = self.ecs_mut().map(|b| b.take_boat_blows()) else { return Vec::new() };
        blows.into_iter().filter_map(|(e, d, by)| Some((self.id_of(e)?, d, by))).collect()
    }
    /// The stack of the item entity `id`; None when it is no item.
    pub fn item_stack(&self, id: u32) -> Option<ItemStack> {
        self.ecs()?.item_stack(self.entity_of(id)?)
    }
    /// The item entities the next blast hurts not (their stack resists it): they are pushed all the same.
    pub fn spare_from_blast(&mut self, ids: &[u32]) {
        let spared = ids.iter().filter_map(|&id| self.entity_of(id)).collect();
        if let Some(b) = self.ecs_mut() {
            b.spare_from_blast(spared);
        }
    }
    /// A hit on the boat `id` by `attacker`; `creative` is the attacker's mode, `entity_drops` the rule. `Ignored` when inert or unknown.
    pub fn boat_hit(&mut self, mw: &MobWorld<'_>, id: u32, amount: f32, attacker: Option<Who>, creative: bool, entity_drops: bool) -> HitResult {
        let Some(e) = self.entity_of(id) else { return HitResult::Ignored };
        self.ecs_mut().map_or(HitResult::Ignored, |b| b.boat_hit(mw, e, amount, attacker, creative, entity_drops))
    }
    /// The right click of `player` on the boat `id` (`hand`: 0 main, else off). `Pass` when inert or unknown.
    pub fn boat_use(&mut self, mw: &MobWorld<'_>, id: u32, player: u32, hand: u8, sneaking: bool, out_of_control: bool, ridden_by_guest: bool) -> UseResult {
        let Some(e) = self.entity_of(id) else { return UseResult::Pass };
        self.ecs_mut().map_or(UseResult::Pass, |b| b.boat_use(mw, e, player, hand, sneaking, out_of_control, ridden_by_guest))
    }
    /// A copy of the chest's slots of the boat `id`; None without a chest or when inert.
    pub fn boat_items(&self, id: u32) -> Option<Vec<Option<ItemStack>>> {
        let e = self.entity_of(id)?;
        Some(self.ecs()?.boat_items(e)?.to_vec())
    }
    pub fn boat_set_slot(&mut self, id: u32, slot: usize, stack: Option<ItemStack>) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(b) = self.ecs_mut() {
            b.boat_set_slot(e, slot, stack);
        }
    }
    /// The loot table and seed of the chest boat `id`, once; None when inert or nothing is pending.
    pub fn boat_take_loot(&mut self, id: u32) -> Option<(String, i64)> {
        let e: Entity = self.entity_of(id)?;
        self.ecs_mut()?.boat_take_loot(e)
    }
}
