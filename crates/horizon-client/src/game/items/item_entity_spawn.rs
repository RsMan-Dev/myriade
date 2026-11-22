//! `new ItemEntity(..)` and `ServerLevel.addFreshEntity` of a dropped item, on the ECS: the stack joins the level with the motion its
//! random draws, and its uuid stays in the host's table.
use super::{Game, spill};
use horizon_client::items::ItemStack;

impl Game<'_> {
    /// `new ItemEntity(..)` joined to the ECS level; returns its motion.
    pub(in crate::game) fn add_item_entity_prepared(
        &mut self,
        stack: ItemStack,
        position: [f64; 3],
        velocity: Option<[f64; 3]>,
        pickup_delay: u32,
        random: Option<spill::ItemRandom>,
    ) -> [f64; 3] {
        self.spawn_prepared_item(stack, position, velocity, pickup_delay, random).1
    }
    /// Item `id` leaves the level (`Entity.discard`); mods hear it. False when it is not an item.
    pub(in crate::game) fn ecs_remove_item(&mut self, id: u32) -> bool {
        if self.ecs.item(id).is_none() {
            return false;
        }
        if let Some(e) = self.ecs.entity_of(id) {
            self.ecs.despawn(e);
        }
        self.native_entity_left(id);
        true
    }
    /// Every dropped item leaves the level, as `Entity.discard` does to each.
    pub(in crate::game) fn ecs_clear_items(&mut self) {
        for id in self.ecs.item_ids() {
            self.ecs_remove_item(id);
        }
    }
    /// `spawn_item_own` that returns the id of the item it made (None when the level is inert).
    pub(in crate::game) fn spawn_item_own_id(&mut self, stack: ItemStack, position: [f64; 3], pickup_delay: u32) -> Option<u32> {
        self.spawn_prepared_item(stack, position, None, pickup_delay, None).0
    }
    /// The id of the item joined to the ECS level (None when the level is inert), with its motion.
    pub(in crate::game) fn spawn_prepared_item(
        &mut self,
        stack: ItemStack,
        position: [f64; 3],
        velocity: Option<[f64; 3]>,
        pickup_delay: u32,
        random: Option<spill::ItemRandom>,
    ) -> (Option<u32>, [f64; 3]) {
        let random = random.unwrap_or_else(|| spill::prepare_item_random(velocity));
        let velocity = velocity.unwrap_or(random.velocity);
        let delay = i16::try_from(pickup_delay).unwrap_or(i16::MAX);
        let id = self.ecs_drop_after(stack, position, velocity, delay);
        if let Some(id) = id {
            self.persist.uuids.insert(id, random.uuid);
            if let Some(e) = self.ecs.entity_of(id) {
                self.ecs.set_rotation(e, random.yaw, 0.0);
            }
        }
        (id, velocity)
    }
}
