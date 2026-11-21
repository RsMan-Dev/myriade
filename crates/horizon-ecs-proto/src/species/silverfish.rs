//! `Silverfish` (`mobs/hostile.rs:61-70`): its targets and goals; it has no stroll (the stone merge replaces it). Not ported:
//! `R:hostile` (damage, storm).
use crate::behaviours::{climb_on_powder_snow, float, hurt_by, melee, nearest_target, silverfish_merge, silverfish_wake_up_friends, Entry};
use crate::species::Def;

pub static DEF: Def = Def {
    name: "minecraft:silverfish",
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert: true, ..hurt_by::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER),
    ],
    goals: &[
        Entry::new(&float::MODULE, 1, &float::Params::DEFAULT),
        Entry::new(&climb_on_powder_snow::MODULE, 1, &climb_on_powder_snow::Params::DEFAULT),
        Entry::new(&silverfish_wake_up_friends::MODULE, 3, &silverfish_wake_up_friends::Params::DEFAULT),
        Entry::new(&melee::MODULE, 4, &melee::Params::DEFAULT),
        Entry::new(&silverfish_merge::MODULE, 5, &silverfish_merge::Params::DEFAULT),
    ],
    not_in_peaceful: true,
    ..Def::EMPTY
};
