//! `Vex`: flies through the blocks (`noPhysics`) straight at a wanted point (`VexMoveControl`), no path. Float (0), charge (4), random move
//! (8), the look at the nearest player within 3 blocks at 9 (`vex.rs:31`, `probability(1.0)`: period 1, every ask); targets: hurt by (1; the
//! raiders' blows are not retaliated against: not ported), the owner's target (2), the nearest player (3). `attend : look_at_player des
//! mobs` (`vex.rs:32`, range 8 at 10: the module looks at players only).
use super::Def;
use crate::behaviours::{flier_goals as g, float, hurt_by, limited_life, look_at_player, nearest_target, Cap, Entry};
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
        Entry::new(&look_at_player::MODULE, 9, &look_at_player::Params { range: 3.0, period: 1, ..look_at_player::Params::DEFAULT }),
        Entry::new(&limited_life::MODULE, 10, &()),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { control: fly::Control::Direct, noclip: true, ..fly::Params::DEFAULT }))],
    ..Def::EMPTY
};
