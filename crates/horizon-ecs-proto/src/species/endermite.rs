//! `Endermite` (`mobs/hostile.rs:72-84`): the silverfish's list without its stone towers. Its own rules (lifetime, the enderman
//! neighbour's teleport) are not ported.
use crate::behaviours::{climb_on_powder_snow, float, hurt_by, look_at_player, melee, nearest_target, random_look, stroll, Entry};
use crate::species::Def;

pub static DEF: Def = Def {
    name: "minecraft:endermite",
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert: true, ..hurt_by::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER),
    ],
    goals: &[
        Entry::new(&float::MODULE, 1, &float::Params::DEFAULT),
        Entry::new(&climb_on_powder_snow::MODULE, 1, &climb_on_powder_snow::Params::DEFAULT),
        Entry::new(&melee::MODULE, 2, &melee::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 3, &stroll::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 7, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 8, &random_look::Params::DEFAULT),
    ],
    not_in_peaceful: true,
    sound_source: super::SoundCategory::Hostile,
    ..Def::EMPTY
};
