//! `PiglinBrute` (`mobs/piglin.rs:1463-1482`, `register_brute`). The names of the piglin and of the wither skeleton are not in the
//! registry yet: they change nothing until those species exist. Not ported: the piglin brain (`PiglinBruteAi`), `R:piglin`.
use crate::behaviours::{brute_stroll_home, hurt_by, float, melee, nearest_target, stroll, look_at_player, Entry};
use crate::species::Def;

pub static DEF: Def = Def {
    name: "minecraft:piglin_brute",
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert: true, alert_except: &["minecraft:piglin", "minecraft:piglin_brute"], ..hurt_by::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER),
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { players: false, types: &["minecraft:wither_skeleton"], ..nearest_target::Params::DEFAULT }),
    ],
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        // DOUTE : le cooldown de mêlée de 20 ticks n'a pas de paramètre dans melee
        Entry::new(&melee::MODULE, 2, &melee::Params::DEFAULT),
        Entry::new(&brute_stroll_home::MODULE, 5, &brute_stroll_home::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 5, &stroll::Params { speed: 0.6, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
    ],
    ..Def::EMPTY
};
