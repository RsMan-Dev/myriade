//! `Parrot`: a flier (`FlyingMoveControl(10, false)`, `FlyingPathNavigation`) that wanders in the air; its other goals (the owner, the
//! shoulder, the mimic, the panic) are not ported yet.
use super::Def;
use crate::behaviours::{fly_stroll, follow_mob, Cap, Entry};
use crate::locomotion::{fly, Medium};

pub static DEF: Def = Def {
    name: "minecraft:parrot",
    // `Parrot.registerGoals`: follow a mob (3), wander (2): `FollowMobGoal(this, 1.0, 3.0, 7.0)`.
    goals: &[Entry::new(&fly_stroll::MODULE, 2, &fly_stroll::Params::DEFAULT), Entry::new(&follow_mob::MODULE, 3, &follow_mob::Params { speed: 1.0, stop: 3.0, area: 7.0, ..follow_mob::Params::DEFAULT })],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Locomotion]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { max_turn: 10.0, hovers: false, glide: 0.6, ..fly::Params::DEFAULT }))],
    ..Def::EMPTY
};
