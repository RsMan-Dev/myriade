//! `WitherSkeleton` (`mobs/undead.rs:148-188`, `666-776`): the skeleton's goals and targets, with a sword instead of a bow: the bow goal
//! goes, the melee at 4 (`SkeletonWeapon`'s melee, speed 1.2) comes. Its piglin target (priority 3) is in the `Mob` placed first among the
//! priority 3 goals; here it comes last among them (order of equal priorities: DOUTE)..
use super::{skeleton, Change, Def, Section};
use crate::behaviours::{melee, nearest_target, ranged_bow, Entry};

pub static DEF: Def = Def {
    name: "minecraft:wither_skeleton",
    caps: Some(super::FIRE_WITHER_IMMUNE_CAPS),
    reference: Some(&skeleton::DEF),
    changes: &[
        Change::Remove { module: &ranged_bow::MODULE, nth: 0 },
        Change::Add(Section::Goals, Entry::new(&melee::MODULE, 4, &melee::Params { speed: 1.2, ..melee::Params::DEFAULT })),
        Change::Add(Section::Targets, Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:piglin", "minecraft:piglin_brute"], ..nearest_target::Params::DEFAULT })),
    ],
    // The wither it puts on what it hits (`on_hit_effects`); fire immune (it does not burn in the day): the skeleton's `sun_burn` is not kept.
    rules: &[crate::rules::RuleEntry::new(&crate::effects::ON_HIT, &crate::effects::WITHER_SKELETON)],
    ..Def::EMPTY
};
