//! What the worn and held pieces leave at death (`Mob.dropCustomDeathLoot`, only when the death table was rolled). Per slot: the stored drop
//! chance (0: skipped; above 1: preserved), raised by Looting when a living attacker killed (`looting`), then the roll `next_float() < chance`.
//! A piece that is not preserved falls only when a player killed the mob, and then comes with a random damage; a piece that prevents drops
//! stays. A falling piece leaves its slot. Not here: the enchantment data (Looting, `PREVENT_EQUIPMENT_DROP`), the hands' damage (not kept).
use crate::equipment::take_slot;
use crate::equipment_wear::{armor_index, Wear};
use crate::goals::{equipped, Slot};
use bevy_ecs::prelude::*;
use horizon_core::random::RandomSource;

/// `Mob.DEFAULT_EQUIPMENT_DROP_CHANCE`.
pub const DEFAULT: f32 = 0.085;

/// The drop chance of each slot, by `index` (main hand, off hand, feet, legs, chest, head). Absent: `DEFAULT` everywhere.
#[derive(Component, Clone, Copy, Debug)]
pub struct DropChances(pub [f32; 6]);

fn index(slot: Slot) -> usize {
    match slot {
        Slot::Main => 0,
        Slot::Off => 1,
        Slot::Feet => 2,
        Slot::Legs => 3,
        Slot::Chest => 4,
        Slot::Head => 5,
    }
}

/// `DropChances.byEquipment`: the stored chance of a slot.
pub fn chance_of(w: &World, e: Entity, slot: Slot) -> f32 {
    w.get::<DropChances>(e).map_or(DEFAULT, |c| c.0[index(slot)])
}

/// The roll of one piece. `chance` is the stored value, `raised` the one after Looting. Returns the damage the piece falls with, or None
/// when it stays. `own` is the damage it has (kept when preserved); `max` its durability (0: none).
pub fn roll_piece(chance: f32, raised: f32, killed_by_player: bool, prevented: bool, max: i32, own: i32, random: &mut impl RandomSource) -> Option<i32> {
    if chance == 0.0 {
        return None;
    }
    let preserved = chance > 1.0;
    if prevented || !(killed_by_player || preserved) {
        return None;
    }
    if random.next_float() >= raised {
        return None;
    }
    if preserved || max <= 0 {
        return Some(own);
    }
    let inner = random.next_int_bound((max - 3).max(1)).unwrap_or(0);
    Some(max - random.next_int_bound(1 + inner).unwrap_or(0))
}

/// The pieces `e` leaves at death, as (slot, item, damage), each taken off its slot. `looting` raises a chance (the identity without the
/// enchantment data), `prevents` tells the items that never drop.
pub fn death_drops(
    w: &mut World,
    e: Entity,
    table_rolled: bool,
    killed_by_player: bool,
    looting: impl Fn(f32) -> f32,
    prevents: impl Fn(u32) -> bool,
    random: &mut impl RandomSource,
) -> Vec<(Slot, u32, i32)> {
    let mut out = Vec::new();
    if !table_rolled {
        return out;
    }
    let mut wear = w.get::<Wear>(e).copied().unwrap_or_default();
    let mut worn_armor = false;
    for slot in [Slot::Main, Slot::Off, Slot::Feet, Slot::Legs, Slot::Chest, Slot::Head] {
        let id = equipped(w, e, slot);
        if id == 0 {
            continue;
        }
        let chance = chance_of(w, e, slot);
        let armor = armor_index(slot);
        let own = armor.map_or(0, |i| wear.0[i]);
        let max = crate::equipment_items::durability_of(id);
        if let Some(damage) = roll_piece(chance, looting(chance), killed_by_player, prevents(id), max, own, random) {
            take_slot(w, e, slot);
            if let Some(i) = armor {
                wear.0[i] = 0;
                worn_armor = true;
            }
            out.push((slot, id, damage));
        }
    }
    if worn_armor {
        w.entity_mut(e).insert(wear);
    }
    out
}
