//! `TropicalFish`: a fish of the base, in a school of at most 8 (`maxSchoolSize`, `aquatic.rs:783`), at priority 5 (`FollowFlockLeader`).
//! Its patterns and colours are data of the species (`COMMON_TROPICAL`, `aquatic.rs`): not in this file yet.
use super::{templates::fish, Def};
use crate::behaviours::{follow_flock_leader, Entry, ambient_sound};

pub static DEF: Def = Def {
    name: "minecraft:tropical_fish",
    reference: Some(&fish::DEF),
    goals: &[Entry::new(&follow_flock_leader::MODULE, 5, &follow_flock_leader::Params::DEFAULT), Entry::new(&ambient_sound::MODULE, 14, &ambient_sound::Params { sounds: &["entity.tropical_fish.ambient"], interval: Some(120), ..ambient_sound::Params::DEFAULT })],
    rules: &[crate::rules::RuleEntry::new(&crate::behaviours::held_swap::HELD_SWAP, &crate::behaviours::held_swap::SwapParams { from: "minecraft:water_bucket", to: "minecraft:tropical_fish_bucket", adult_only: false, takes_creature: true, sound: "minecraft:item.bucket.fill_fish", alt: None })],
    draws: Some(&DRAWS),
    ..Def::EMPTY
};

use crate::rules::spawn_draw::Roll;
/// `TropicalFish.COMMON_VARIANTS` as saved packed ids (`pattern | base colour << 16 | pattern colour << 24`; a pattern is its size (small 0,
/// large 1) with its index in the size << 8).
const COMMON: Roll = Roll::Pick(&[
    0x0701_0101, 0x0707_0001, 0x0B07_0001, 0x0700_0501, 0x070B_0100, 0x0001_0000, 0x0306_0500, 0x040A_0301, 0x0E00_0501, 0x0400_0500, 0x0700_0201,
    0x0100_0501, 0x0609_0300, 0x0305_0400, 0x000E_0401, 0x0E07_0200, 0x000E_0301, 0x0400_0001, 0x000E_0000, 0x0007_0100, 0x0409_0300, 0x0404_0001,
]);
/// The other 10 %: a pattern of the twelve, a base colour, a pattern colour (`Util.getRandom` over each array, in that order).
const FREE: Roll = Roll::Packed(&[
    (Roll::Pick(&[0x000, 0x100, 0x200, 0x300, 0x400, 0x500, 0x001, 0x101, 0x201, 0x301, 0x401, 0x501]), 0),
    (Roll::Int(16), 16),
    (Roll::Int(16), 24),
]);
/// `TropicalFish.finalizeSpawn` after the school's: a fish of a group that has a variant takes it; else 90 % (`nextFloat() < 0.9`) one of the
/// common variants, which the group keeps for the next ones, else a free draw that makes no group value.
static DRAWS: crate::behaviours::spawn_draw::Draws = crate::behaviours::spawn_draw::Draws {
    group: None,
    list: &[crate::behaviours::spawn_draw::Draw {
        when: crate::behaviours::spawn_draw::When::After,
        kind: crate::behaviours::spawn_draw::Kind::Shared {
            start: &[COMMON],
            odds: Some(0.9),
            member: crate::behaviours::spawn_draw::Member::First,
            free: &FREE,
            baby_from: None,
            should_baby: false,
        },
        store: crate::behaviours::spawn_draw::Store::Wide,
    }],
};
