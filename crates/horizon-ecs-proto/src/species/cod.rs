//! `Cod`: a fish of the base, in a school (`AbstractSchoolingFish`): it follows its leader at priority 5 (`FollowFlockLeader`).
use super::{templates::fish, Def};
use crate::locomotion::{swim, walk, Medium};
use crate::behaviours::{follow_flock_leader, Entry};

pub static DEF: Def = Def {
    name: "minecraft:cod",
    reference: Some(&fish::DEF),
    locomotion: &[
        (Medium::Ground, Entry::new(&walk::MODULE, 0, &walk::Params::DEFAULT)),
        (Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params { flop: Some("entity.cod.flop"), ..swim::Params::DEFAULT })),
    ],
    goals: &[Entry::new(&follow_flock_leader::MODULE, 5, &follow_flock_leader::Params::DEFAULT)],
    rules: &[crate::rules::RuleEntry::new(&crate::behaviours::held_swap::HELD_SWAP, &crate::behaviours::held_swap::SwapParams { from: "minecraft:water_bucket", to: "minecraft:cod_bucket", adult_only: false, takes_creature: true, sound: "minecraft:item.bucket.fill_fish", alt: None })],
    ..Def::EMPTY
};
