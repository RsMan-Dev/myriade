//! `Vex`: flies through the blocks (`noPhysics`) straight at a wanted point (`VexMoveControl`), no path. Float (0), charge (4), random move
//! (8); targets: hurt by (1; the raiders' blows are not retaliated against: not ported), the owner's target (2), the nearest player (3). The look goals wait.
use super::Def;
use crate::behaviours::{flier_goals as g, float, hurt_by, nearest_target, Cap, Entry};
use crate::locomotion::{fly, Medium};

pub static DEF: Def = Def {
    name: "minecraft:vex",
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert: true, ..hurt_by::Params::DEFAULT }),
        Entry::new(&g::VEX_COPY_OWNER_TARGET, 2, &g::Params::DEFAULT),
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params::PLAYER),
    ],
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&g::VEX_CHARGE, 4, &g::Params::DEFAULT),
        Entry::new(&g::VEX_RANDOM_MOVE, 8, &g::Params::DEFAULT),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Targets, Cap::Locomotion]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { control: fly::Control::Direct, noclip: true, ..fly::Params::DEFAULT }))],
    ..Def::EMPTY
};
