//! `ZombifiedPiglin` (`mobs/undead.rs:36-90`): its own targets (no villager, golem or turtle), the melee at 2, the turtle egg at 4,
//! the stroll and the look. Left out: the nearest player, `when` angry (`neutral:angry` is not a registered predicate; the
//! `neutral_anger::is_angry` function must be named by a mod), and the spear (`spear_use`: weapons, not ported).
use crate::behaviours::{attack_turtle_egg, hurt_by, look_at_player, melee, stroll, Entry};
use crate::species::Def;

pub static DEF: Def = Def {
    name: "minecraft:zombified_piglin",
    targets: &[Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert: true, ..hurt_by::Params::DEFAULT })],
    goals: &[
        Entry::new(&melee::MODULE, 2, &melee::Params::DEFAULT),
        Entry::new(&attack_turtle_egg::MODULE, 4, &attack_turtle_egg::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 7, &stroll::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 8, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
    ],
    ..Def::EMPTY
};
