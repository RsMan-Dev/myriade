//! `Turtle` (`mobs/turtle.rs:28-56`), amphibious (`AmphibiousPathNavigation`, `TurtleMoveControl` in both media, `travelInWater` at 0.1: the
//! swim mode with its amphibious parameters, the only mode it declares; its walk preference is water and sand, `WalkValue::Turtle`): tempted by seagrass (the tag `turtle_food`), going to the water, travelling in the water, looking at
//! players, strolling in the water. Left out, waiting for their parameter or module: the panic that looks for water (`looking_for_water`
//! is not a parameter of `panic`). `breed` (with `carries`), `lay_egg` and `turtle_go_home` are in.
//! The water malus, the door maluses and the amphibious navigation have no field in `Def`: not in this file.
use super::Def;
use crate::locomotion::{swim, Medium};
use crate::behaviours::{Cap, Cond, panic, look_at_player, tempt, turtle_go_home, turtle_go_to_water, breed, lay_egg, turtle_stroll, turtle_travel, Entry};

/// `TurtleBreedGoal.canUse`: no mating while the turtle carries its egg.
static NOT_CARRYING: Cond = Cond::Not(&Cond::Is("turtle:has_egg"));

pub static DEF: Def = Def {
    name: "minecraft:turtle",
    reference: None,
    goals: &[
        Entry::new(&panic::MODULE, 0, &panic::Params { speed: 1.2, water: Some(7), ..panic::Params::DEFAULT }),
        Entry::new(&breed::MODULE, 1, &breed::Params { speed: 1.0, foods: &["minecraft:seagrass"], carries: true, xp: true, ..breed::Params::DEFAULT }).when(&NOT_CARRYING),
        Entry::new(&lay_egg::MODULE, 1, &lay_egg::Params { radius: 9.0, dig_ticks: 200, floor: "minecraft:sand", name: "minecraft:turtle_egg", states: &[&[("eggs","1")], &[("eggs","2")], &[("eggs","3")], &[("eggs","4")]], sound: Some(("entity.turtle.lay_egg", 0.3)), ..lay_egg::Params::DEFAULT }),
        Entry::new(&tempt::MODULE, 2, &tempt::Params { speed: 1.1, items: &["minecraft:seagrass"], ..tempt::Params::DEFAULT }),
        Entry::new(&turtle_go_to_water::MODULE, 3, &turtle_go_to_water::Params::DEFAULT),
        Entry::new(&turtle_go_home::MODULE, 4, &turtle_go_home::Params::DEFAULT),
        Entry::new(&turtle_travel::MODULE, 7, &turtle_travel::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 8, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&turtle_stroll::MODULE, 9, &turtle_stroll::Params::DEFAULT),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion]),
    locomotion: &[(Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params { needs_water: false, amphibious: Some(false), breach: false, control: swim::Control::Turtle, push: swim::Push::Fixed(0.1), sinks: true, squid: false, reset_pitch: false }))],
    ..Def::EMPTY
};
