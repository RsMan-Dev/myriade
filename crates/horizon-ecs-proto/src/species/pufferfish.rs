//! `Pufferfish`: a fish of the base that inflates when a creature comes near (`PufferfishPuffGoal` at priority 1, in place of the school).
//! Not ported: its sting (effects) and its body size. The species it does not fear: the tag `not_scary_for_pufferfish`.
use super::{templates::fish, Def};
use crate::behaviours::{pufferfish_puff, Entry};

pub static DEF: Def = Def {
    name: "minecraft:pufferfish",
    reference: Some(&fish::DEF),
    goals: &[Entry::new(
        &pufferfish_puff::MODULE,
        1,
        &pufferfish_puff::Params {
            fearless: &[
                "minecraft:turtle",
                "minecraft:guardian",
                "minecraft:elder_guardian",
                "minecraft:cod",
                "minecraft:pufferfish",
                "minecraft:salmon",
                "minecraft:tropical_fish",
                "minecraft:dolphin",
                "minecraft:squid",
                "minecraft:glow_squid",
                "minecraft:tadpole",
                "minecraft:nautilus",
                "minecraft:zombie_nautilus",
                "minecraft:sulfur_cube",
            ],
            ..pufferfish_puff::Params::DEFAULT
        },
    )],
        rules: &[crate::rules::RuleEntry::new(&crate::effects::ON_HIT, &crate::effects::PUFFERFISH), crate::rules::RuleEntry::new(&crate::behaviours::held_swap::HELD_SWAP, &crate::behaviours::held_swap::SwapParams { from: "minecraft:water_bucket", to: "minecraft:pufferfish_bucket", adult_only: false, takes_creature: true, sound: "minecraft:item.bucket.fill_fish", alt: None })],
    ..Def::EMPTY
};
