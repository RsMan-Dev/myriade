//! Dropped items by network id: `ItemView`, a value built on demand from the stack's components, and the named writes the host makes to
//! an item (its stack, its pickup delay). Nothing is kept on the host side.
use super::level::EcsLevel;
use bevy_ecs::{entity::Entity, world::World};
use myriade_engine::items::ItemStack;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::{
    cow::{Burning, Contact, Health, Pos, Vel, now_of},
    item::{Item, Thrower},
    rules::item::pickup_ready,
};

/// A dropped item as the host reads it: its stack, its clocks, who may take it and its body.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemView {
    pub id: u32,
    pub stack: ItemStack,
    /// `ItemEntity.age`: the ticks since it was made or last merged into.
    pub age: u32,
    /// `Entity.tickCount`: the ticks since it entered the level.
    pub ticks: u32,
    /// `ItemEntity.pickupDelay`: the ticks left before it can be taken (0: now).
    pub pickup_delay: i64,
    /// `ItemEntity.target`: the only player who may pick it up.
    pub target: Option<[i32; 4]>,
    /// `ItemEntity.thrower`.
    pub thrower: Option<[i32; 4]>,
    pub position: Vec3,
    pub velocity: Vec3,
    pub on_ground: bool,
    pub fall_distance: f64,
    pub health: f32,
    /// `Entity.remainingFireTicks`: 0 when it is not on fire.
    pub fire_ticks: i32,
}

/// The view of the item `e`; None when it is not an item.
fn view_of(w: &World, e: Entity, id: u32) -> Option<ItemView> {
    let it = w.get::<Item>(e)?;
    let now = now_of(w, e);
    let contact = w.get::<Contact>(e);
    Some(ItemView {
        id,
        stack: it.stack.clone(),
        age: horizon_ecs_proto::item::age_of(it, now).max(0) as u32,
        ticks: (now as i64 - it.born).max(0) as u32,
        pickup_delay: horizon_ecs_proto::item::delay_of(it, now),
        target: it.target,
        thrower: w.get::<Thrower>(e).map(|t| t.0),
        position: w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0),
        velocity: w.get::<Vel>(e).map_or(Vec3::ZERO, |v| v.0),
        on_ground: contact.is_some_and(|c| c.on_ground),
        fall_distance: contact.map_or(0.0, |c| c.fall_distance),
        health: w.get::<Health>(e).map_or(0.0, |h| h.0),
        fire_ticks: w.get::<Burning>(e).map_or(0, |b| (b.until - now as i64) as i32),
    })
}

impl EcsLevel {
    /// The view of the item `id`; None when it is absent, asleep or the level is inert.
    pub fn item(&self, id: u32) -> Option<ItemView> {
        let w = self.world()?;
        view_of(w, self.entity_of(id)?, id)
    }
    /// The ids of every dropped item of the level, in order.
    pub fn item_ids(&self) -> Vec<u32> {
        let mut ids = self.species_ids("minecraft:item");
        ids.sort_unstable();
        ids
    }
    /// The views of every dropped item of the level, in the order of their ids.
    pub fn item_views(&self) -> Vec<ItemView> {
        self.item_ids().into_iter().filter_map(|id| self.item(id)).collect()
    }
    /// The id and box of every dropped item, for the block columns the hoppers skip by.
    pub fn item_boxes(&self) -> Vec<(u32, Aabb)> {
        self.item_ids().into_iter().filter_map(|id| self.bounds(id).map(|b| (id, b))).collect()
    }
    /// The thrower of the item `id`, replaced by `who` (None: no thrower); false when it is not an item.
    pub fn set_item_thrower(&mut self, id: u32, who: Option<[i32; 4]>) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(w) = self.world_mut() else { return false };
        if w.get::<Item>(e).is_none() {
            return false;
        }
        match who {
            Some(who) => drop(w.entity_mut(e).insert(Thrower(who))),
            None => drop(w.entity_mut(e).remove::<Thrower>()),
        }
        true
    }
    /// The player who may take the item `id`, replaced by `who`; false when it is not an item.
    pub fn set_item_target(&mut self, id: u32, who: Option<[i32; 4]>) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(w) = self.world_mut() else { return false };
        w.get_mut::<Item>(e).map(|mut it| it.target = who).is_some()
    }
    /// The ticks of the item `id` since it entered the level, replaced by `ticks`; false when it is not an item.
    pub fn set_item_ticks(&mut self, id: u32, ticks: u32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(w) = self.world_mut() else { return false };
        let now = now_of(w, e);
        w.get_mut::<Item>(e).map(|mut it| it.born = now as i64 - i64::from(ticks)).is_some()
    }
    /// The age of the item `id`, replaced by `age`; false when it is not an item.
    pub fn set_item_age(&mut self, id: u32, age: u32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(w) = self.world_mut() else { return false };
        let now = now_of(w, e);
        w.get_mut::<Item>(e).map(|mut it| it.age_base = horizon_ecs_proto::item::base_for(now, age as i32)).is_some()
    }
    /// The position of the item `id`, replaced by `at`; false when it is not an item.
    pub fn set_item_position(&mut self, id: u32, at: Vec3) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(w) = self.world_mut() else { return false };
        w.get_mut::<Pos>(e).map(|mut p| p.0 = at).is_some()
    }
    /// The health of the item `id`, replaced by `health`; false when it is not an item.
    pub fn set_item_health(&mut self, id: u32, health: f32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(w) = self.world_mut() else { return false };
        w.get_mut::<Health>(e).map(|mut h| h.0 = health).is_some()
    }
    /// `ItemStack.onDestroyed` of the item `id`: what it held spills as items where it lies (`spill_destroyed`).
    pub fn spill_item(&mut self, id: u32) {
        let (Some(e), Some(w)) = (self.entity_of(id), self.world_mut()) else { return };
        horizon_ecs_proto::item::spill_destroyed(w, e);
    }
    /// The velocity of the item `id` plus `by`; false when it is not an item.
    pub fn add_item_velocity(&mut self, id: u32, by: Vec3) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(v) = self.item(id).map(|it| it.velocity) else { return false };
        self.set_velocity(e, v.add_vector(by));
        true
    }
    /// The stack of the item `id`, replaced by `stack`; false when it is not an item.
    pub fn set_item_stack(&mut self, id: u32, stack: ItemStack) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(w) = self.world_mut() else { return false };
        w.get_mut::<Item>(e).map(|mut it| it.stack = stack).is_some()
    }
    /// The pickup delay of the item `id` in ticks from now (`ItemEntity.setPickUpDelay`); false when it is not an item.
    pub fn set_item_pickup_delay(&mut self, id: u32, delay: i64) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(w) = self.world_mut() else { return false };
        let now = now_of(w, e);
        w.get_mut::<Item>(e).map(|mut it| { it.ready_at = pickup_ready(delay).map(|d| now + d); it.neg_delay = if delay < 0 { delay as i16 } else { 0 }; }).is_some()
    }
}
