//! `WitherSkeleton` (`mobs/undead.rs:148-188`, `666-776`): the skeleton's goals and targets, with a sword instead of a bow: the bow goal
//! goes, the melee at 4 (`SkeletonWeapon`'s melee, speed 1.2) comes. Its piglin target (priority 3) is in the `Mob` placed first among the
//! priority 3 goals; here it comes last among them (order of equal priorities: DOUTE)..
use super::{skeleton, Change, Def, Section};
use crate::behaviours::{melee, nearest_target, ranged_bow, Entry, ambient_sound};

pub static DEF: Def = Def {
    name: "minecraft:wither_skeleton",
    caps: Some(super::FIRE_WITHER_IMMUNE_CAPS),
    reference: Some(&skeleton::DEF),
    changes: &[
        Change::Remove { module: &crate::behaviours::convert_in_water::MODULE, nth: 0 },
        Change::Set { module: &ambient_sound::MODULE, nth: 0, params: &ambient_sound::Params { sounds: &["entity.wither_skeleton.ambient"], interval: Some(80), ..ambient_sound::Params::DEFAULT } },
        Change::Remove { module: &ranged_bow::MODULE, nth: 0 },
        Change::Add(Section::Goals, Entry::new(&melee::MODULE, 4, &melee::Params { speed: 1.2, ..melee::Params::DEFAULT })),
        Change::Add(Section::Targets, Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:piglin", "minecraft:piglin_brute"], ..nearest_target::Params::DEFAULT })),
    ],
    // The wither it puts on what it hits (`on_hit_effects`). It keeps the skeleton's `sun_burn` (`Mob.aiStep`: it is in `burn_in_daylight`);
    // fire immune, it is set alight and put out without fire (`hostile::sun_burn`).
    rules: &[
        crate::rules::RuleEntry::new(&crate::effects::ON_HIT, &crate::effects::WITHER_SKELETON),
        crate::rules::RuleEntry::new(&crate::spawn_gear::RULE, &crate::spawn_gear::WITHER_SKELETON),
    ],
    not_in_peaceful: true,
    sound_source: super::SoundCategory::Hostile,
    ..Def::EMPTY
};
