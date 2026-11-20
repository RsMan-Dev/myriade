//! `Allay`: a flier that wanders in the air, keeps close to its owner, takes the item it is given and flies it back, looks at players,
//! and runs when hurt. Left out, outside this species: the throw at arrival (a brain step), the jukebox dance, the duplication, the
//! note block and vibrations (no vibration event in the frame), the search for a dropped item (no event for an object near a creature:
//! relais/cadre.md), and the `interact` entry point (not called yet: the owner is set by `tame::set_owner`).
//! DOUBT: the glide damping of the allay's flight is not checked against vanilla (the default value is used).
use super::Def;
use crate::behaviours::{animal_panic, fly_stroll, give_to_owner, look_at_player, random_look, stay_close_to_owner, Cap, Entry};
use crate::locomotion::{fly, Medium};

pub static DEF: Def = Def {
    name: "minecraft:allay",
    goals: &[
        Entry::new(&animal_panic::MODULE, 0, &animal_panic::Params { speed: 2.5, ..animal_panic::Params::DEFAULT }),
        Entry::new(&stay_close_to_owner::MODULE, 3, &stay_close_to_owner::Params::DEFAULT),
        Entry::new(&give_to_owner::MODULE, 2, &give_to_owner::Params::DEFAULT),
        Entry::new(&fly_stroll::MODULE, 4, &fly_stroll::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 5, &look_at_player::Params { range: 6.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 6, &random_look::Params::DEFAULT),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Tamable, Cap::Locomotion]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { max_turn: 20.0, hovers: true, ..fly::Params::DEFAULT }))],
    ..Def::EMPTY
};
