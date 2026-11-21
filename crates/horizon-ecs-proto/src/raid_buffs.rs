//! `Raider.applyRaidBuffs` (`Pillager`, `Vindicator`): the gear a raider of a wave wears, with the one draw `nextFloat <= odds` taken from the
//! entity's own `Rng`. The host gives the kind, the raid's enchant odds and the group counts (`Raid.getNumGroups` of EASY and NORMAL):
//! the numbers stay where the raid keeps them.
use crate::equip_stack;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;

/// The slot of the main hand in `equip_stack` (`EquipmentSlot` order).
const MAINHAND: usize = 0;

/// Which `applyRaidBuffs` a raider of the wave runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaidBuff {
    /// A crossbow with Quick Charge, only after the EASY (level 1) or NORMAL (level 2) group count.
    Crossbow,
    /// The iron axe, with Sharpness after the wave's odds (level 2 past the NORMAL group count, else 1).
    Axe,
}

/// What the raid says: the enchant odds (`Raid.getEnchantOdds`) and `numGroups(EASY)`, `numGroups(NORMAL)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RaidBuffRules {
    pub odds: f32,
    pub easy_groups: i32,
    pub normal_groups: i32,
}

/// Runs the buff of `kind` on `e` for `wave`. False when `e` is gone.
pub fn apply(w: &mut World, e: Entity, kind: RaidBuff, wave: i32, rules: RaidBuffRules) -> bool {
    if w.get_entity(e).is_err() {
        return false;
    }
    let roll = w.get_mut::<crate::cow::Rng>(e).map_or(1.0, |mut r| r.next_f64() as f32);
    let enchanted = roll <= rules.odds;
    match kind {
        RaidBuff::Crossbow => {
            let level = if !enchanted {
                0
            } else if wave > rules.normal_groups {
                2
            } else if wave > rules.easy_groups {
                1
            } else {
                0
            };
            if level > 0 {
                let mut stack = ItemStack::new("minecraft:crossbow", 1);
                stack.enchant("minecraft:quick_charge", level);
                equip_stack::set(w, e, MAINHAND, Some(stack));
            }
        }
        RaidBuff::Axe => {
            let mut stack = ItemStack::new("minecraft:iron_axe", 1);
            if enchanted {
                stack.enchant("minecraft:sharpness", if wave > rules.normal_groups { 2 } else { 1 });
            }
            equip_stack::set(w, e, MAINHAND, Some(stack));
        }
    }
    true
}
