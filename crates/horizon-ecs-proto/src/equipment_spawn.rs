//! Spawn equipment: `Mob.populateDefaultEquipmentSlots` of the classes that wear or hold something (`finalize.rs`, `armor` and `populate`).
//! Pure: it returns the pieces (slot, item) in the order the game rolls them; the caller puts them with `equipment::set_slot`, the armour
//! only into the slots the mob does not hold yet (the rolls happen anyway). A single choice draws nothing. Not here: `enchant` (its odds
//! scale with the difficulty too) and the drowned's trident roll (a gate of `float > 0.9`, then `int(16)`).
use crate::goals::Slot;
use horizon_core::random::RandomSource;

const MATERIALS: [&str; 6] = ["leather", "copper", "golden", "chainmail", "iron", "diamond"];

/// The weapon in hand of a class: a gate (a chance, easy then hard; None: always, with no roll) and a weighted choice.
#[derive(Clone, Copy, Debug)]
pub struct Hand {
    pub gate: Option<[f32; 2]>,
    pub weapons: &'static [(&'static str, u32)],
}

/// What a class gets at spawn: a set of armour, and a weapon in hand.
#[derive(Clone, Copy, Debug)]
pub struct Spawn {
    pub armor: bool,
    pub hand: Option<Hand>,
}

/// Zombie, husk and zombie villager.
pub const ZOMBIE: Spawn = Spawn {
    armor: true,
    hand: Some(Hand {
        gate: Some([0.01, 0.05]),
        weapons: &[("minecraft:iron_sword", 1), ("minecraft:iron_spear", 1), ("minecraft:iron_shovel", 4)],
    }),
};
/// Skeleton, stray, bogged and parched.
pub const SKELETON: Spawn = Spawn { armor: true, hand: Some(Hand { gate: None, weapons: &[("minecraft:bow", 1)] }) };
pub const ZOMBIFIED_PIGLIN: Spawn = Spawn {
    armor: false,
    hand: Some(Hand { gate: None, weapons: &[("minecraft:golden_spear", 1), ("minecraft:golden_sword", 19)] }),
};
pub const PIGLIN_BRUTE: Spawn = Spawn { armor: false, hand: Some(Hand { gate: None, weapons: &[("minecraft:golden_axe", 1)] }) };
pub const WITHER_SKELETON: Spawn = Spawn { armor: false, hand: Some(Hand { gate: None, weapons: &[("minecraft:stone_sword", 1)] }) };

/// The pieces `spawn` gives a fresh mob. `special` is the difficulty's special multiplier, `hard` whether it is Hard; `worn` tells the
/// armour slots the mob already holds.
pub fn populate(spawn: &Spawn, special: f32, hard: bool, worn: impl Fn(Slot) -> bool, random: &mut impl RandomSource) -> Vec<(Slot, String)> {
    let mut out = Vec::new();
    if spawn.armor {
        armor(special, hard, &worn, random, &mut out);
    }
    if let Some(h) = spawn.hand {
        let gate = h.gate.map(|g| g[usize::from(hard)]);
        if gate.is_none_or(|c| random.next_float() < c) {
            out.push((Slot::Main, pick(h.weapons, random).to_string()));
        }
    }
    out
}

/// `Mob.populateDefaultEquipmentSlots` of the armour: one set of one material, from the head down; past the helmet each piece is
/// cut by `partial`.
fn armor(special: f32, hard: bool, worn: &impl Fn(Slot) -> bool, random: &mut impl RandomSource, out: &mut Vec<(Slot, String)>) {
    if random.next_float() >= 0.15 * special {
        return;
    }
    let mut kind = random.next_int_bound(3).unwrap_or(0) as usize;
    for _ in 0..3 {
        if random.next_float() < 0.1087 {
            kind += 1;
        }
    }
    let partial = if hard { 0.1 } else { 0.25 };
    let pieces = [(Slot::Head, "helmet"), (Slot::Chest, "chestplate"), (Slot::Legs, "leggings"), (Slot::Feet, "boots")];
    for (i, (slot, piece)) in pieces.into_iter().enumerate() {
        if i > 0 && random.next_float() < partial {
            break;
        }
        if !worn(slot) {
            out.push((slot, format!("minecraft:{}_{piece}", MATERIALS[kind])));
        }
    }
}

/// The weighted choice of a hand (`int(total)` against the cumulative weights); one entry draws nothing.
pub(crate) fn pick(weapons: &'static [(&'static str, u32)], random: &mut impl RandomSource) -> &'static str {
    if let [(only, _)] = weapons {
        return only;
    }
    let total: u32 = weapons.iter().map(|w| w.1).sum();
    let mut r = random.next_int_bound(total as i32).unwrap_or(0) as u32;
    for &(name, weight) in weapons {
        if r < weight {
            return name;
        }
        r -= weight;
    }
    weapons[weapons.len() - 1].0
}
