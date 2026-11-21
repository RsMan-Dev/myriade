//! What a dispenser does to a creature, as P keeps it: shearing (`Sheared`, a sparse marker present once the creature is shorn, saved as
//! the byte `sheared`), the marker the species' configuration sets (`Brushable`), and the loot of a brush stroke
//! (`BRUSH_TABLE`, the table `Armadillo.brushOffScute` rolls). Nothing here names a species.
use crate::cow::with_level_random;
use crate::loot::roll_table;
use crate::nbt::{owner, Field};
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::nbt::{Compound, Tag};
use horizon_core::random::RandomSource;
use std::borrow::Cow;

/// `BuiltInLootTables.ARMADILLO_BRUSH` (`brush/armadillo`), rolled by each brush stroke on a creature that can be brushed.
pub const BRUSH_TABLE: &str = "minecraft:brush/armadillo";

/// Present once the creature's wool or shell is off; posed by `behaviours::shear`'s species changes.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Sheared;

/// Set by the species' configuration: the creature can be brushed.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Brushable;

/// The stacks of `BRUSH_TABLE` a brush stroke on `e` rolls; none when `e` is not `Brushable`.
pub fn brush_drop(w: &mut World, e: Entity) -> Vec<ItemStack> {
    if w.get::<Brushable>(e).is_none() {
        return Vec::new();
    }
    let seed = with_level_random(w, |r| r.next_long());
    roll_table(w, BRUSH_TABLE, seed)
}

/// The keys whose presence poses their component (`nbt::register_creators`): `sheared`, a byte, 1 while the creature is shorn.
pub const CREATORS: &[Field] = &[Field { key: Cow::Borrowed("sheared"), write: write_sheared, read: read_sheared, accepts: is_byte, owner: Some(owner::<Sheared>) }];

fn is_byte(t: &Tag) -> bool {
    matches!(t, Tag::Byte(_))
}

fn write_sheared(w: &World, e: Entity, c: &mut Compound) {
    if w.get::<Sheared>(e).is_some() {
        c.insert("sheared".into(), Tag::Byte(1));
    }
}

fn read_sheared(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    if *v != 0 {
        w.entity_mut(e).insert(Sheared);
    }
    true
}
