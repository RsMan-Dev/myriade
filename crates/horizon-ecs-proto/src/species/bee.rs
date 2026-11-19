//! `Bee`: flies by a path (`FlyingMoveControl(20, hovers)`, `FlyingPathNavigation`), its goals the cycle of the nectar: pollinate (4), go to the hive
//! (5), locate it (5), enter it (1), the known flower (6), the validations (3), crops (7: only counted), wander (8: the frame's flying stroll).
//! Not configured: the attack and the anger (the sting), breeding, tempting, following the parent, the float goal.
use super::Def;
use crate::behaviours::{bee, fly_stroll, Cap, Entry};
use crate::locomotion::{fly, Medium};

pub static DEF: Def = Def {
    name: "minecraft:bee",
    goals: &[
        Entry::new(&bee::ENTER_HIVE, 1, &bee::Params::DEFAULT),
        Entry::new(&bee::VALIDATE_HIVE, 3, &bee::Params::DEFAULT),
        Entry::new(&bee::VALIDATE_FLOWER, 3, &bee::Params::DEFAULT),
        Entry::new(&bee::POLLINATE, 4, &bee::Params::DEFAULT),
        Entry::new(&bee::LOCATE_HIVE, 5, &bee::Params::DEFAULT),
        Entry::new(&bee::GO_TO_HIVE, 5, &bee::Params::DEFAULT),
        Entry::new(&bee::GO_TO_KNOWN_FLOWER, 6, &bee::Params::DEFAULT),
        Entry::new(&bee::GROW_CROP, 7, &bee::Params::DEFAULT),
        Entry::new(&fly_stroll::MODULE, 8, &fly_stroll::Params::DEFAULT),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Locomotion]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { control: fly::Control::Path, max_turn: 20.0, hovers: true, ..fly::Params::DEFAULT }))],
    ..Def::EMPTY
};
