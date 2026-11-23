//! The slots a creature carries on itself (`SimpleContainer`): a piglin's eight, given when it is made. Its stacks are its death loot, each
//! whole (`Piglin.dropCustomDeathLoot`: `removeAllItems`), and the slots they leave are empty. Nothing here names a species.
use crate::rules::{no_interact, params, Rule};
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;

/// The slots of a carried inventory (`None` empty).
#[derive(Component, Clone, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct ItemStore {
    pub items: Vec<Option<ItemStack>>,
}

/// The number of slots a rule gives (`Piglin`: `SimpleContainer(8)`).
#[derive(Clone, Copy, Debug)]
pub struct Slots(pub usize);
impl Slots {
    pub const PIGLIN: Self = Self(8);
}

/// The rule `item_store` (`Slots`): the empty slots, once at creation.
pub static STORE: Rule = Rule {
    name: "item_store",
    equip: |w, e, p| {
        let n = params::<Slots>(p).0;
        w.entity_mut(e).insert(ItemStore { items: vec![None; n] });
    },
    attrs: crate::rules::no_attrs,
    interact: no_interact,
};

/// Takes every stack of `e`, whole, and leaves its slots empty (none without a store).
pub fn death_stacks(world: &mut World, e: Entity) -> Vec<ItemStack> {
    let Some(mut s) = world.get_mut::<ItemStore>(e) else { return Vec::new() };
    let out: Vec<ItemStack> = s.items.iter_mut().filter_map(|s| s.take()).collect();
    out
}
