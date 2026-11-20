//! `Guardian` (`guardian.rs:44-80`): the beam (`guardian_beam`, priority 4), the walk back to its restriction (5), the stroll every 80 ticks (7),
//! looking at players (8, 8 blocks) and at other guardians (8, 12 blocks, one chance in a hundred: not ported, `look_at_player` has one
//! class), random looking (9); targets: players, squid and axolotls beyond 3 blocks. `GuardianMoveControl` is `swim::Control::Guardian`. Left out, in `suivi/ecs-ecarts.md`: the spikes and the tail animation, the thorns of the spikes.
use super::Def;
use crate::behaviours::{guardian_beam, look_at_player, move_towards_restriction, nearest_target, random_look, stroll, Cap, Entry};
use crate::locomotion::{swim, Medium};

pub const TARGETS: &[Entry] = &[Entry::new(
    &nearest_target::MODULE,
    1,
    &nearest_target::Params { players: true, types: &["minecraft:squid", "minecraft:glow_squid", "minecraft:axolotl"], beyond_sqr: Some(9.0), ..nearest_target::Params::DEFAULT },
)];
pub static DEF: Def = Def {
    name: "minecraft:guardian",
    targets: TARGETS,
    goals: &[
        Entry::new(&guardian_beam::MODULE, 4, &guardian_beam::Params::GUARDIAN),
        Entry::new(&move_towards_restriction::MODULE, 5, &move_towards_restriction::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 7, &stroll::Params { interval: Some(80), ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 8, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 9, &random_look::Params::DEFAULT),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion]),
    locomotion: &[(Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params { needs_water: true, control: swim::Control::Guardian, push: swim::Push::Fixed(0.1), ..swim::Params::DEFAULT }))],
    ..Def::EMPTY
};
