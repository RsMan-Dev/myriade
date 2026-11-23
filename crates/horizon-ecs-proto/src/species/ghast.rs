//! `Ghast` (`mobs/nether.rs:201-216`): a free flier (`GhastMoveControl`, no path): it floats toward spots it picks around itself
//! (`RandomFloatAround`), looks at its target (`GhastLook`, priority 7) and shoots a large fireball at it (`charged_shot`, priority 7). Its player target (priority 1) is the one within 4 blocks in height (`max_dy`). Left out: the explosion
//! Left out: the explosion of the fireball (`Events.exploded` has no capability yet).
use super::Def;
use crate::behaviours::{charged_shot, float_around, ghast_look, nearest_target, Cap, Entry};
use crate::locomotion::{fly, Medium};

pub static DEF: Def = Def {
    spawn: Some(crate::spawning::SpawnDef { rule: crate::spawning::Rule::GroundOneIn { bound: 20 }, cluster: 1, ..crate::spawning::SpawnDef::MONSTER }),
    name: "minecraft:ghast",
    rules: &[crate::rules::RuleEntry::new(&crate::rules::climbable::NO_CLIMB, &()), crate::rules::RuleEntry::new(&crate::leash::QUAD_HOLDER, &())],
    targets: &[Entry::new(&nearest_target::MODULE, 1, &nearest_target::Params { max_dy: Some(4.0), ..nearest_target::Params::PLAYER })],
    goals: &[
        Entry::new(&float_around::MODULE, 5, &float_around::Params::DEFAULT),
        Entry::new(&ghast_look::MODULE, 7, &ghast_look::Params::DEFAULT),
        Entry::new(&charged_shot::MODULE, 7, &charged_shot::Params::GHAST),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::FireImmune, Cap::RefusedByMobs]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { control: fly::Control::Float, free: true, ..fly::Params::DEFAULT }))],
    not_in_peaceful: true,
    sound_source: super::SoundCategory::Hostile,
    ..Def::EMPTY
};
