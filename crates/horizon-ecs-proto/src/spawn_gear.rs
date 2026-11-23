//! What a class puts on a fresh mob in its `finalizeSpawn`: `populateDefaultEquipmentSlots`, `populateDefaultEquipmentEnchantments`, the
//! carved pumpkin of Halloween, the pick-up roll that some classes draw, and the small draws of a class (a baby piglin, the drowned's shell).
//! A species' configuration is a `Kit` (the `RULE` puts it on the creature as `Gear`); `spawn_finalize` calls the steps in the order of each class
//! (`mob_first`). Pure draws in `equipment_spawn` (armour, weapon of hand); the enchantments are `equipment_enchant`'s. Nothing here names a species.
use crate::equipment_drop::{index, DropChances, DEFAULT};
use crate::equipment_spawn::{self as spawn, Spawn};
use crate::goals::{equipped, Slot};
use crate::rules::{no_attrs, no_interact, params, Rule};
use crate::spawn_reason::SpawnReason;
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;
use horizon_core::random::RandomSource;

/// The date of the host's day: `SpecialDates.isHalloween` (31 October). Absent: false.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Halloween(pub bool);

/// `SpecialDates.isHalloween`: the host's day is 31 October.
pub fn halloween_date(month: u32, day: u32) -> bool {
    month == 10 && day == 31
}

/// A draw's gate: `float < chance` (easy/normal, hard), or the drowned's `float > bound`.
#[derive(Clone, Copy, Debug)]
pub enum Gate {
    Below([f32; 2]),
    Above(f64),
}
/// What a draw gives: a weighted choice (one entry draws nothing); one float against the bounds of the arms, the last arm taking the rest
/// (an arm of two items draws a boolean, true takes the first); one float under `at` for the first item, else a weighted choice.
#[derive(Clone, Copy, Debug)]
pub enum Choice {
    Weighted(&'static [(&'static str, u32)]),
    Roll(&'static [(f32, &'static [&'static str])]),
    Split(f32, &'static str, &'static [(&'static str, u32)]),
}
/// When a draw happens in `finalizeSpawn`: in the baby branch (the adult's spawn weapon), with the default slots, or after the class's own work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Early,
    Populate,
    Late,
}
/// One piece a class may put in a slot: slot, gate, choice, and its conditions (the slot empty, an adult, no raid) and the drop chance it sets.
#[derive(Clone, Copy, Debug)]
pub struct Draw {
    pub slot: Slot,
    pub gate: Option<Gate>,
    pub choice: Choice,
    pub phase: Phase,
    pub empty_only: bool,
    pub adult_only: bool,
    pub not_in_raid: bool,
    pub drop: Option<f32>,
}
impl Draw {
    /// A piece that always comes, with no draw, in the main hand.
    pub const fn main(item: &'static [(&'static str, u32)]) -> Self {
        Self { slot: Slot::Main, gate: None, choice: Choice::Weighted(item), phase: Phase::Populate, empty_only: false, adult_only: false, not_in_raid: false, drop: None }
    }
}

/// The configuration of a class.
#[derive(Clone, Copy, Debug)]
pub struct Kit {
    /// `Mob`'s armour and the class's weapon of hand (`equipment_spawn::populate`).
    pub base: Spawn,
    pub draws: &'static [Draw],
    /// `populateDefaultEquipmentEnchantments` runs (false for a class that never calls it).
    pub enchant: bool,
    /// `Monster.finalizeSpawn` (`Mob`'s draws) comes before the gear (zombie, skeleton, vindicator) rather than after it.
    pub mob_first: bool,
    /// The carved pumpkin of Halloween on an empty head.
    pub pumpkin: bool,
    /// The `0.55 * special` pick-up roll comes after the gear (skeleton; the zombie draws its own earlier).
    pub picks_after: bool,
    /// A baby chance drawn unless the reason is `Structure` (`Piglin`), the baby staying one.
    pub baby: Option<f32>,
}
impl Kit {
    pub const NONE: Kit = Kit { base: Spawn { armor: false, hand: None }, draws: &[], enchant: false, mob_first: false, pumpkin: false, picks_after: false, baby: None };
}

/// The kit of a creature, from its species' configuration.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Gear(pub &'static Kit);

/// A creature that does not pick up loot (`canPickUpLoot` false) though its species could: the roll of its spawn failed.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct NoLoot;

/// The rule `spawn_gear`, parameters `Kit`.
pub static RULE: Rule = Rule {
    name: "spawn_gear",
    equip: |w, e, p| {
        w.entity_mut(e).insert(Gear(params::<Kit>(p)));
    },
    attrs: no_attrs,
    interact: no_interact,
};

const IRON_SWORD: &[(&str, u32)] = &[("minecraft:iron_sword", 1)];
const GOLD: [&[(&str, u32)]; 4] = [&[("minecraft:golden_helmet", 1)], &[("minecraft:golden_chestplate", 1)], &[("minecraft:golden_leggings", 1)], &[("minecraft:golden_boots", 1)]];
const GOLD_PIECE: Gate = Gate::Below([0.1, 0.1]);
const fn gold(slot: Slot, i: usize) -> Draw {
    Draw { slot, gate: Some(GOLD_PIECE), choice: Choice::Weighted(GOLD[i]), phase: Phase::Populate, empty_only: false, adult_only: true, not_in_raid: false, drop: None }
}

/// Zombie, husk, zombie villager: `Mob`'s armour, the weapon of `Zombie`.
pub const ZOMBIE: Kit = Kit { base: spawn::ZOMBIE, draws: &[], enchant: true, mob_first: true, pumpkin: true, picks_after: false, baby: None };
/// `ZombifiedPiglin.populateDefaultEquipmentSlots`.
pub const ZOMBIFIED_PIGLIN: Kit = Kit { base: spawn::ZOMBIFIED_PIGLIN, ..ZOMBIE };
/// `Drowned`: no armour, a trident or a rod in 10% of cases, then a nautilus shell in the empty off hand (3%, always dropped).
pub const DROWNED: Kit = Kit {
    base: Spawn { armor: false, hand: None },
    draws: &[
        Draw {
            gate: Some(Gate::Above(0.9)),
            choice: Choice::Weighted(&[(crate::species::lists::TRIDENT, 10), ("minecraft:fishing_rod", 6)]),
            ..Draw::main(&[])
        },
        Draw { slot: Slot::Off, gate: Some(Gate::Below([0.03, 0.03])), choice: Choice::Weighted(&[("minecraft:nautilus_shell", 1)]), phase: Phase::Late, empty_only: true, drop: Some(2.0), ..Draw::main(&[]) },
    ],
    ..ZOMBIE
};
/// Skeleton, stray, bogged and parched: armour and a bow, the pick-up roll after.
pub const SKELETON: Kit = Kit { base: spawn::SKELETON, draws: &[], enchant: true, mob_first: true, pumpkin: true, picks_after: true, baby: None };
/// `WitherSkeleton`: a stone sword, no enchantments (the method is empty).
pub const WITHER_SKELETON: Kit = Kit { base: spawn::WITHER_SKELETON, enchant: false, ..SKELETON };
/// `Piglin`: 20% babies, else (not from a structure) a crossbow or a golden spear or sword; each golden piece of an adult in 10%.
pub const PIGLIN: Kit = Kit {
    base: Spawn { armor: false, hand: None },
    draws: &[
        Draw { choice: Choice::Split(0.5, "minecraft:crossbow", &[("minecraft:golden_spear", 1), ("minecraft:golden_sword", 9)]), phase: Phase::Early, adult_only: true, ..Draw::main(&[]) },
        gold(Slot::Head, 0),
        gold(Slot::Chest, 1),
        gold(Slot::Legs, 2),
        gold(Slot::Feet, 3),
    ],
    enchant: true,
    mob_first: false,
    pumpkin: false,
    picks_after: false,
    baby: Some(0.2),
};
/// `PiglinBrute`: a golden axe, no enchantments.
pub const PIGLIN_BRUTE: Kit = Kit { base: spawn::PIGLIN_BRUTE, ..Kit::NONE };
/// `Pillager`: a crossbow and the enchantments, before `Mob`'s draws.
pub const PILLAGER: Kit = Kit { draws: &[Draw::main(&[("minecraft:crossbow", 1)])], enchant: true, ..Kit::NONE };
/// `Vindicator`: after `Mob`'s draws, an iron axe when it is in no raid.
pub const VINDICATOR: Kit = Kit { draws: &[Draw { not_in_raid: true, ..Draw::main(&[("minecraft:iron_axe", 1)]) }], enchant: true, mob_first: true, ..Kit::NONE };
/// `Illusioner`: a bow in hand, set before the spellcaster's draws (no enchantments).
pub const ILLUSIONER: Kit = Kit { draws: &[Draw::main(&[("minecraft:bow", 1)])], ..Kit::NONE };
/// `Vex`: an iron sword that never drops, then the enchantments, before `Mob`'s draws.
pub const VEX: Kit = Kit { draws: &[Draw { drop: Some(0.0), ..Draw::main(IRON_SWORD) }], enchant: true, ..Kit::NONE };
/// `Fox.populateDefaultEquipmentSlots`: in 20% of cases something in the mouth, no enchantments.
pub const FOX: Kit = Kit {
    draws: &[Draw {
        gate: Some(Gate::Below([0.2, 0.2])),
        choice: Choice::Roll(&[
            (0.05, &["minecraft:emerald"]),
            (0.2, &[crate::species::lists::EGG]),
            (0.4, &["minecraft:rabbit_foot", "minecraft:rabbit_hide"]),
            (0.6, &["minecraft:wheat"]),
            (0.8, &["minecraft:leather"]),
            (f32::MAX, &["minecraft:feather"]),
        ]),
        ..Draw::main(&[])
    }],
    ..Kit::NONE
};

/// The item a draw gives, with its draws in the order of the source; None when the gate stays shut.
fn pick(d: &Draw, hard: bool, r: &mut impl RandomSource) -> Option<&'static str> {
    match d.gate {
        Some(Gate::Below(c)) if r.next_float() >= c[usize::from(hard)] => return None,
        Some(Gate::Above(x)) if f64::from(r.next_float()) <= x => return None,
        _ => {}
    }
    Some(match d.choice {
        Choice::Weighted(w) => spawn::pick(w, r),
        Choice::Roll(arms) => {
            let f = r.next_float();
            let items = arms.iter().find(|a| f < a.0).map_or(arms[arms.len() - 1].1, |a| a.1);
            match items {
                [a, b] => if r.next_boolean() { *a } else { *b },
                [a, ..] => *a,
                [] => return None,
            }
        }
        Choice::Split(at, first, rest) => {
            if r.next_float() < at {
                first
            } else {
                spawn::pick(rest, r)
            }
        }
    })
}

/// `Mob.setItemSlot` of a spawn piece, and the drop chance the class sets for it.
fn put(w: &mut World, e: Entity, slot: Slot, name: &str, drop: Option<f32>, out: &mut Vec<(Slot, String)>) {
    crate::equipment::set_slot(w, e, slot, crate::items::item_id(name));
    if let Some(c) = drop {
        set_drop_chance(w, e, slot, c);
    }
    out.push((slot, name.to_string()));
}

/// `Mob.setDropChance`: the slot's stored chance (`DropChances`, the default elsewhere); 2,0 is `setGuaranteedDrop`.
pub fn set_drop_chance(w: &mut World, e: Entity, slot: Slot, chance: f32) {
    let mut c = w.get::<DropChances>(e).map_or([DEFAULT; 8], |c| c.0);
    c[index(slot)] = chance;
    w.entity_mut(e).insert(DropChances(c));
}

/// `Mob.setCanPickUpLoot` of a spawn roll: the dolphin's rule reads it, the looting block through `NoLoot`.
pub fn set_can_pick_up(w: &mut World, e: Entity, picks: bool) {
    if let Some(mut p) = w.get_mut::<crate::behaviours::item_pickup::Pickup>(e) {
        p.picks_up = picks;
    }
    if picks {
        w.entity_mut(e).remove::<NoLoot>();
    } else if w.get::<crate::behaviours::pick_up_gear::Gear>(e).is_some() {
        w.entity_mut(e).insert(NoLoot);
    }
}

fn run(w: &mut World, e: Entity, kit: &Kit, phase: Phase, baby: bool, r: &mut impl RandomSource, out: &mut Vec<(Slot, String)>) {
    let hard = crate::effects::difficulty(w) >= 3;
    for d in kit.draws.iter().filter(|d| d.phase == phase) {
        if (d.adult_only && baby) || (d.not_in_raid && crate::raids::raid_of(w, e).is_some()) || (d.empty_only && equipped(w, e, d.slot) != 0) {
            continue;
        }
        if let Some(name) = pick(d, hard, r) {
            put(w, e, d.slot, name, d.drop, out);
        }
    }
}

/// The baby branch of a class that has one (`Piglin`), then nothing else: true when the creature is a baby. The adult's spawn weapon is
/// the `Early` draws. A spawn from a structure draws neither.
pub fn baby_branch(w: &mut World, e: Entity, kit: &Kit, reason: SpawnReason, r: &mut impl RandomSource, out: &mut Vec<(Slot, String)>) -> bool {
    let Some(chance) = kit.baby else { return false };
    if reason == SpawnReason::Structure {
        return false;
    }
    let baby = r.next_float() < chance;
    if baby {
        crate::spawn_finalize::set_baby(w, e, crate::spawn_finalize::NEVER);
    } else {
        run(w, e, kit, Phase::Early, false, r, out);
    }
    baby
}

/// `populateDefaultEquipmentSlots` then `populateDefaultEquipmentEnchantments`: the armour and the hand (`equipment_spawn::populate`, the armour
/// only into the empty slots), the class's own pieces, then the enchantments of what it holds.
pub fn populate(w: &mut World, e: Entity, kit: &Kit, special: f32, baby: bool, r: &mut impl RandomSource, out: &mut Vec<(Slot, String)>) {
    let hard = crate::effects::difficulty(w) >= 3;
    let worn = |s: Slot| matches!(s, Slot::Head | Slot::Chest | Slot::Legs | Slot::Feet) && equipped(w, e, s) != 0;
    for (slot, name) in spawn::populate(&kit.base, special, hard, worn, r) {
        put(w, e, slot, &name, None, out);
    }
    run(w, e, kit, Phase::Populate, baby, r, out);
    if kit.enchant {
        enchant(w, e, special, r);
    }
}

/// `Mob.populateDefaultEquipmentEnchantments`: the main hand (25%), then the armour from the feet up (50%), each scaled by `special`. Empty
/// slots draw nothing. Needs the world's item and enchantment data (`TradeData`); without it nothing is enchanted.
fn enchant(w: &mut World, e: Entity, special: f32, r: &mut impl RandomSource) {
    let Some((items, enchantments)) = w.get_resource::<crate::behaviours::trade_offers::TradeData>().map(|d| (d.items, d.enchantments)) else { return };
    for (slot, chance) in [(Slot::Main, crate::equipment_enchant::MAIN_CHANCE), (Slot::Feet, crate::equipment_enchant::ARMOR_CHANCE), (Slot::Legs, crate::equipment_enchant::ARMOR_CHANCE), (Slot::Chest, crate::equipment_enchant::ARMOR_CHANCE), (Slot::Head, crate::equipment_enchant::ARMOR_CHANCE)] {
        let i = crate::loot::stack_slot(slot);
        let Some(name) = crate::equip_stack::get(w, e, i).map(|s| s.item.to_string()).or_else(|| crate::items::item_name(equipped(w, e, slot)).map(str::to_string)) else { continue };
        let levels = crate::equipment_enchant::enchant_slot(items, enchantments, &name, special, chance, r);
        if levels.is_empty() {
            continue;
        }
        let mut stack = crate::equip_stack::get(w, e, i).unwrap_or_else(|| ItemStack::new(&name, 1));
        stack.data_mut().enchantments = levels;
        crate::equip_stack::set(w, e, i, Some(stack));
    }
}

/// After the class's gear: the pick-up roll of a class that draws it here (`0.55 * special`), then the Halloween pumpkin (an empty head, a
/// quarter of cases, a jack o'lantern one time in ten, never dropped).
pub fn after(w: &mut World, e: Entity, kit: &Kit, special: f32, r: &mut impl RandomSource, out: &mut Vec<(Slot, String)>) {
    if kit.picks_after {
        let picks = r.next_float() < 0.55 * special;
        set_can_pick_up(w, e, picks);
    }
    if kit.pumpkin && equipped(w, e, Slot::Head) == 0 && w.get_resource::<Halloween>().is_some_and(|h| h.0) && r.next_float() < 0.25 {
        let item = if r.next_float() < 0.1 { "minecraft:jack_o_lantern" } else { "minecraft:carved_pumpkin" };
        put(w, e, Slot::Head, item, Some(0.0), out);
    }
}

/// The class's draws that come after the rest of its `finalizeSpawn` (`Drowned`'s shell).
pub fn late(w: &mut World, e: Entity, kit: &Kit, baby: bool, r: &mut impl RandomSource, out: &mut Vec<(Slot, String)>) {
    run(w, e, kit, Phase::Late, baby, r, out);
}

/// The kit of the creature, when its species has one.
pub fn kit_of(w: &World, e: Entity) -> Option<&'static Kit> {
    w.get::<Gear>(e).map(|g| g.0)
}
