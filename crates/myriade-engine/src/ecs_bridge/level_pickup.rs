//! The pickups of the players, the throwers and the end of life of the stacks and orbs, by network id, through the bridge
//! (`horizon-ecs-proto` `bridge/host_pickup.rs`). An inert level answers as an empty world.
use super::level::EcsLevel;
use crate::mobs::MobWorld;
use bevy_ecs::entity::Entity;
use myriade_engine::items::ItemStack;
use horizon_ecs_proto::targeting::Who;

impl EcsLevel {
    /// The stacks in the player's box at `feet`, taken as `room` accepts: (network id, stack taken, place); 0 for an entity no player follows.
    pub fn pickup_items(
        &mut self,
        mw: &MobWorld<'_>,
        who: Who,
        uuid: [i32; 4],
        feet: [f64; 3],
        reach: f64,
        room: &mut dyn FnMut(&ItemStack) -> u32,
    ) -> Vec<(u32, ItemStack, [f64; 3])> {
        let Some(ecs) = self.ecs_mut() else {
            return Vec::new();
        };
        ecs.pickup_items(mw, who, uuid, feet, reach, room)
    }
    /// The stacks in the player's box at `feet` it may take now, by network id, not taken.
    pub fn pickup_candidates(&mut self, mw: &MobWorld<'_>, who: Who, uuid: [i32; 4], feet: [f64; 3], reach: f64) -> Vec<(u32, ItemStack)> {
        let Some(ecs) = self.ecs_mut() else {
            return Vec::new();
        };
        ecs.pickup_candidates(mw, who, uuid, feet, reach)
    }
    /// The stack `id` taken in the player's box at `feet`, as `room` accepts: the stack and its place, `None` when it is not there to take.
    pub fn pickup_item(&mut self, mw: &MobWorld<'_>, who: Who, uuid: [i32; 4], id: u32, feet: [f64; 3], reach: f64, room: &mut dyn FnMut(&ItemStack) -> u32) -> Option<(ItemStack, [f64; 3])> {
        let ecs = self.ecs_mut()?;
        ecs.pickup_item(mw, who, uuid, id, feet, reach, room)
    }
    /// The orbs in the player's box at `feet`, by network id (bound with `alloc` when the orb has none).
    pub fn pickup_orbs(&mut self, mw: &MobWorld<'_>, feet: [f64; 3], reach: f64, alloc: &mut dyn FnMut() -> u32) -> Vec<u32> {
        let Some(ecs) = self.ecs_mut() else {
            return Vec::new();
        };
        ecs.pickup_orbs(mw, feet, reach, alloc).into_iter().map(|(id, _)| id).collect()
    }
    /// A player collects one of the orb `orb` (its network id): its value, or `None` when there is no such orb.
    pub fn collect_orb(&mut self, mw: &MobWorld<'_>, orb: u32) -> Option<u32> {
        let ecs = self.ecs_mut()?;
        let e = ecs.entity_of(orb)?;
        ecs.collect_orb(mw, e)
    }
    pub fn set_thrower(&mut self, e: Entity, who: [i32; 4]) {
        if let Some(ecs) = self.ecs_mut() {
            ecs.set_thrower(e, who);
        }
    }
    pub fn extend_item_life(&mut self, e: Entity, ticks: u32) {
        if let Some(ecs) = self.ecs_mut() {
            ecs.extend_item_life(e, ticks);
        }
    }
    pub fn set_item_expiry_hooked(&mut self, on: bool) {
        if let Some(ecs) = self.ecs_mut() {
            ecs.set_item_expiry_hooked(on);
        }
    }
    pub fn set_orb_expiry_hooked(&mut self, on: bool) {
        if let Some(ecs) = self.ecs_mut() {
            ecs.set_orb_expiry_hooked(on);
        }
    }
    pub fn resolve_item_expiry(&mut self, e: Entity, extend: Option<u32>) {
        if let Some(ecs) = self.ecs_mut() {
            ecs.resolve_item_expiry(e, extend);
        }
    }
}
