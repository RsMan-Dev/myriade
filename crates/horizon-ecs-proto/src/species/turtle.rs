//! `Turtle` (`mobs/turtle.rs:28-56`): tempted by seagrass (the tag `turtle_food`), going to the water, travelling in the water, looking at
//! players, strolling in the water. Left out, waiting for their parameter or module: the panic that looks for water (`looking_for_water`
//! is not a parameter of `panic`), the breeding that lays an egg (`breed` has no egg variant), (`turtle_lay_egg` and `turtle_go_home` are in).
//! The water malus, the door maluses and the amphibious navigation have no field in `Def`: not in this file.
use super::Def;
use crate::behaviours::{panic, look_at_player, tempt, turtle_go_home, turtle_go_to_water, turtle_lay_egg, turtle_stroll, turtle_travel, Entry};

pub static DEF: Def = Def {
    name: "minecraft:turtle",
    reference: None,
    goals: &[
        Entry::new(&panic::MODULE, 0, &panic::Params { speed: 1.2, water: Some(7), ..panic::Params::DEFAULT }),
        // attend : breed (priorité 1, variante à oeuf)
        Entry::new(&turtle_lay_egg::MODULE, 1, &turtle_lay_egg::Params::DEFAULT),
        Entry::new(&tempt::MODULE, 2, &tempt::Params { speed: 1.1, items: &["minecraft:seagrass"], ..tempt::Params::DEFAULT }),
        Entry::new(&turtle_go_to_water::MODULE, 3, &turtle_go_to_water::Params::DEFAULT),
        Entry::new(&turtle_go_home::MODULE, 4, &turtle_go_home::Params::DEFAULT),
        Entry::new(&turtle_travel::MODULE, 7, &turtle_travel::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 8, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&turtle_stroll::MODULE, 9, &turtle_stroll::Params::DEFAULT),
    ],
    ..Def::EMPTY
};
