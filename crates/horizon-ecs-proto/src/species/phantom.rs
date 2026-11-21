//! `Phantom`: a free flier (`travelFlying`) banking toward a wanted point (`PhantomMoveControl`), no path. Goals: the strategy (1), the dive
//! (2), the circle round the anchor (3); target: the player in the box (1).
use super::Def;
use crate::behaviours::{flier_goals as g, Cap, Entry};
use crate::locomotion::{fly, Medium};

pub static DEF: Def = Def {
    name: "minecraft:phantom",
    targets: &[Entry::new(&g::PHANTOM_ATTACK_PLAYER_TARGET, 1, &g::Params::DEFAULT)],
    goals: &[
        Entry::new(&crate::behaviours::move_emission::MODULE, 0, &crate::behaviours::move_emission::Params { flap: crate::behaviours::move_emission::Flap::Every { period: 25, flying_only: false, offset: true }, ..crate::behaviours::move_emission::Params::DEFAULT }),
        Entry::new(&g::PHANTOM_ATTACK_STRATEGY, 1, &g::Params::DEFAULT),
        Entry::new(&g::PHANTOM_SWEEP_ATTACK, 2, &g::Params::DEFAULT),
        Entry::new(&g::PHANTOM_CIRCLE_AROUND_ANCHOR, 3, &g::Params::DEFAULT),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Targets, Cap::Locomotion]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { control: fly::Control::Bank, free: true, ..fly::Params::DEFAULT }))],
    not_in_peaceful: true,
    ..Def::EMPTY
};
