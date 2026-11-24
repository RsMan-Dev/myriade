//! `Parrot` (`mobs/tame.rs:109-119`, `register_parrot`): a flier (`FlyingMoveControl(10, false)`, `FlyingPathNavigation`) that wanders in
//! the air, follows its owner once tame, sits when ordered, panics when hurt, looks at players, and follows a mob (`FollowMobGoal`).
//! The landing on the owner's shoulder: `land_on_shoulder` (host output `HostOut::shoulder`). Left out: the taming by seeds (the `TAME` rule: the Mob does it
//! elsewhere, not read here). DOUBT: `TamePanic` does not panic while the tame parrot sits: the `panic` module has no such option.
use super::Def;
use crate::behaviours::{fly_stroll, float, follow_mob, follow_owner, look_at_player, panic, sit_when_ordered, land_on_shoulder, Cap, Entry};
use crate::locomotion::{fly, Medium};

pub static DEF: Def = Def {
    name: "minecraft:parrot",
    variant: Some(&super::variants::PARROT),
    // `register_parrot`: the panic and the float (0), the look (1), the sit and the owner (2), the wander (2), the mob (3).
    goals: &[
        Entry::new(&panic::MODULE, 0, &panic::Params { speed: 1.25, ..panic::Params::DEFAULT }),
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 1, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&sit_when_ordered::MODULE, 2, &sit_when_ordered::Params { stands_when_hurt: true, ..sit_when_ordered::Params::DEFAULT }),
        Entry::new(&follow_owner::MODULE, 2, &follow_owner::Params { speed: 1.0, start: 5.0, stop: 1.0 }),
        Entry::new(&fly_stroll::MODULE, 2, &fly_stroll::Params::DEFAULT),
        Entry::new(&land_on_shoulder::MODULE, 3, &land_on_shoulder::Params::DEFAULT),
        Entry::new(&follow_mob::MODULE, 3, &follow_mob::Params { speed: 1.0, stop: 3.0, area: 7.0, ..follow_mob::Params::DEFAULT }),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Tamable, Cap::Locomotion]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { max_turn: 10.0, hovers: false, glide: 0.6, ..fly::Params::DEFAULT }))],
    spawn: Some(crate::spawning::SpawnDef { rule: crate::spawning::Rule::Bright { on: "minecraft:parrots_spawnable_on" }, ..crate::spawning::SpawnDef::ANIMAL }),
    despawn: Some(crate::despawn::DespawnDef::KEPT),
    rules: &[crate::rules::RuleEntry::new(&crate::rules::NO_FALL, &())],
    ..Def::EMPTY
};
