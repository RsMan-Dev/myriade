//! `Rabbit` (`mobs/rabbit.rs:47-70`): float, climbing powder snow, panic, breeding and tempting with its food (the tag `rabbit_food`), raiding
//! the garden, looking at players. Left out, waiting for their predicate, parameter or module: the flight from the player, the wolf and the
//! monsters (`avoid_entity`, the predicate `rabbit:killer` is absent, the monsters are a list built at run time), the stroll that avoids
//! water (`stroll` has no such parameter). The custom jump (`MoveKind::Custom`, `RabbitJumpControl`) is a movement control, not in this file.
use super::Def;
use crate::behaviours::{avoid_entity::{self, Avoid}, breed, climb_on_powder_snow, float, look_at_player, panic, raid_garden, stroll, tempt, Cond, Entry};

const FOOD: &[&str] = &["minecraft:carrot", "minecraft:golden_carrot", "minecraft:dandelion"];

static NOT_KILLER: Cond = Cond::Not(&Cond::Is("rabbit:killer"));
const fn flee(what: Avoid, max_dist: f64) -> avoid_entity::Params {
    avoid_entity::Params { what, max_dist, walk: 2.2, sprint: 2.2, ..avoid_entity::Params::DEFAULT }
}
static PLAYER: avoid_entity::Params = flee(Avoid::Player, 8.0);
static WOLF: avoid_entity::Params = flee(Avoid::Named(&["minecraft:wolf"]), 10.0);
static MONSTERS: avoid_entity::Params = flee(Avoid::Monsters, 4.0);

pub static DEF: Def = Def {
    name: "minecraft:rabbit",
    reference: None,
    goals: &[
        Entry::new(&float::MODULE, 1, &float::Params::DEFAULT),
        Entry::new(&climb_on_powder_snow::MODULE, 1, &climb_on_powder_snow::Params::DEFAULT),
        Entry::new(&panic::MODULE, 1, &panic::Params { speed: 2.2, ..panic::Params::DEFAULT }),
        Entry::new(&breed::MODULE, 2, &breed::Params { speed: 0.8, foods: FOOD, ..breed::Params::DEFAULT }),
        Entry::new(&tempt::MODULE, 3, &tempt::Params { speed: 1.0, items: FOOD, ..tempt::Params::DEFAULT }),
        Entry::new(&avoid_entity::MODULE, 4, &PLAYER).when(&NOT_KILLER),
        Entry::new(&avoid_entity::MODULE, 4, &WOLF).when(&NOT_KILLER),
        Entry::new(&avoid_entity::MODULE, 4, &MONSTERS).when(&NOT_KILLER),
        Entry::new(&raid_garden::MODULE, 5, &raid_garden::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 6, &stroll::Params { speed: 0.6, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 11, &look_at_player::Params { range: 10.0, ..look_at_player::Params::DEFAULT }),
    ],
    ..Def::EMPTY
};
