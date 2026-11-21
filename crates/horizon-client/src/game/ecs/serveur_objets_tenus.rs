//! Items held and used by ECS mobs: the random draws, the hand stacks, the use counter and the knock-back the
//! `_ecs` variants of `items/` read, each through the bridge (the methods they call are not on `EcsLevel` yet).
use crate::game::Game;
use horizon_client::items::ItemStack;

impl Game<'_> {
    /// `Mob.random.next_float()` of the ECS mob `id`.
    pub(in crate::game) fn ecs_next_float(&mut self, id: u32) -> Option<f32> {
        self.ecs.next_float(id)
    }
    /// `Mob.random.next_int_bound(bound)` of the ECS mob `id`.
    pub(in crate::game) fn ecs_next_int_bound(&mut self, id: u32, bound: i32) -> Option<i32> {
        self.ecs.next_int_bound(id, bound)
    }
    /// The position of the ECS mob `id`, as `[x, y, z]`.
    pub(in crate::game) fn ecs_vec3(&self, id: u32) -> Option<[f64; 3]> {
        self.ecs.position(id)
    }
    /// The stack in the hand of the ECS mob `id`: the main hand from `HandStack`, the off hand from the bridge.
    pub(in crate::game) fn ecs_hand_stack(&self, id: u32, off_hand: bool) -> Option<ItemStack> {
        if off_hand {
            self.ecs.off_hand_stack(id)
        } else {
            self.ecs_main_hand(id)
        }
    }
    /// Whether the ECS mob `id` can start a use: it uses nothing and counts no timer of its own.
    pub(in crate::game) fn ecs_mob_free(&self, id: u32) -> bool {
        !self.ecs.is_using_item(id)
    }
    /// `startUsingItem` of the ECS mob `id` in the off hand or the main hand.
    pub(in crate::game) fn ecs_start_using(&mut self, id: u32, off_hand: bool) -> bool {
        self.ecs.start_using(id, off_hand)
    }
    /// The use of the ECS mob `id`: the hand it uses and its use counter (0 when it uses nothing).
    pub(in crate::game) fn ecs_using(&self, id: u32) -> Option<(bool, i32)> {
        self.ecs.using(id)
    }
    /// Sets the use counter of the ECS mob `id`.
    pub(in crate::game) fn ecs_set_using(&mut self, id: u32, counter: i32) {
        self.ecs.set_using(id, counter);
    }
    /// `stopUsingItem` of the ECS mob `id`.
    pub(in crate::game) fn ecs_stop_using(&mut self, id: u32) {
        self.ecs.stop_using(id);
    }
    /// Whether a totem in a hand saves the ECS mob `id` from its death (`try_totem`, read before the death).
    pub(in crate::game) fn ecs_totem_saves(&mut self, id: u32) -> bool {
        self.ecs.try_totem(id)
    }
}

#[cfg(test)]
mod tests {
    use crate::game::ecs::test_support::ecs_game_with;
    use horizon_client::{items::ItemStack};
    use mod_api::client::first_person::Hand;
    use horizon_ecs_proto::goals::Slot;

    #[test]
    fn an_ecs_mob_starts_the_use_of_the_item_in_its_hand() {
        let (mut g, id) = ecs_game_with("minecraft:cow", [0.0, 64.0, 0.0]);
        g.ecs_set_slot(id, Slot::Main, &ItemStack::new("minecraft:apple", 1));
        assert!(g.start_mob_using_item_ecs(id, Hand::Main));
        assert!(g.mob_item_use_remaining_ecs(id).is_some_and(|t| t > 0));
    }

    #[test]
    fn a_totem_in_hand_saves_an_ecs_mob_once() {
        let (mut g, id) = ecs_game_with("minecraft:zombie", [0.0, 64.0, 0.0]);
        g.ecs_set_slot(id, Slot::Off, &ItemStack::new("minecraft:totem_of_undying", 1));
        assert!(g.ecs_totem_saves(id));
        assert!(!g.ecs_totem_saves(id));
    }
}
