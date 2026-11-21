//! A pocket: one slot holding one kind of item, the `SimpleContainer(1)` of the allay. A plain data component: the module that carries
//! one inserts it on its creatures, and the event that uses it writes it. Nothing here names a species.
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;

/// The largest count a slot takes (`SimpleContainer`'s cap).
pub const MAX_STACK: u32 = 99;

/// One slot, one kind of item (`SimpleContainer(1)` of the allay).
#[derive(Component, Clone, Debug, Default)]
pub struct Pocket(pub Option<ItemStack>);

impl Pocket {
    /// Room for `stack`: `max.min(99)` when empty; `max.min(99) - held.count` when `held.same_item_same_components(stack)`; else 0.
    pub fn room(&self, stack: &ItemStack, max: u32) -> u32 {
        let max = max.min(MAX_STACK);
        match &self.0 {
            None => max,
            Some(held) if held.same_item_same_components(stack) => max.saturating_sub(held.count),
            Some(_) => 0,
        }
    }

    /// Takes `min(stack.count, room)` of `stack` in; returns how many went in.
    pub fn add(&mut self, mut stack: ItemStack, max: u32) -> u32 {
        let count = stack.count.min(self.room(&stack, max));
        if count > 0 {
            match &mut self.0 {
                Some(held) => held.count += count,
                None => {
                    stack.count = count;
                    self.0 = Some(stack);
                }
            }
        }
        count
    }

    /// Empties the slot.
    pub fn take(&mut self) -> Option<ItemStack> {
        self.0.take()
    }

    pub fn held(&self) -> Option<&ItemStack> {
        self.0.as_ref()
    }
}

/// `Allay.allayConsidersItemEqual`: same item and same potion (`held.item == candidate.item && held.potion() == candidate.potion()`).
pub fn considers_item_equal(held: &ItemStack, candidate: &ItemStack) -> bool {
    held.item == candidate.item && held.potion() == candidate.potion()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_pocket_takes_up_to_the_cap() {
        let mut pocket = Pocket::default();
        assert_eq!(pocket.add(ItemStack::new("minecraft:cod", 120), 64), 64);
        assert_eq!(pocket.held().map(|s| s.count), Some(64));
    }

    #[test]
    fn the_same_item_fills_to_99_and_another_is_refused() {
        let mut pocket = Pocket::default();
        pocket.add(ItemStack::new("minecraft:cod", 120), 64);
        assert_eq!(pocket.add(ItemStack::new("minecraft:cod", 50), 1000), 35);
        assert_eq!(pocket.held().map(|s| s.count), Some(99));
        assert_eq!(pocket.add(ItemStack::new("minecraft:stone", 1), 1000), 0);
    }

    #[test]
    fn take_empties_the_slot() {
        let mut pocket = Pocket::default();
        pocket.add(ItemStack::new("minecraft:cod", 3), 64);
        assert_eq!(pocket.take().map(|s| s.count), Some(3));
        assert!(pocket.take().is_none());
    }

    // Only the object is compared here: the potion is not set in this test.
    #[test]
    fn potions_decide_equality() {
        let a = ItemStack::new("minecraft:cod", 1);
        let b = ItemStack::new("minecraft:cod", 5);
        assert!(considers_item_equal(&a, &b));
        assert!(!considers_item_equal(&a, &ItemStack::new("minecraft:stone", 1)));
    }
}
