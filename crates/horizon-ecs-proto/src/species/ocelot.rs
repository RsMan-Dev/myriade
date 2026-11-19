//! `Ocelot.registerGoals` (`mobs/tame.rs` `register_ocelot`): float, tempt (it fears the player unless it trusts him), leap and bite, breed, stroll,
//! look, and the avoidance of players added last. It hunts chickens and baby turtles on land. A trusting ocelot (the marker `Trusting`, set by
//! the game when a player feeds it) neither is scared while tempted nor flees players.
use super::Def;
use crate::behaviours::avoid_entity::{self, Avoid};
use crate::behaviours::{breed, float, leap_at_target, look_at_player, nearest_target, ocelot_attack, stroll, tempt, Cond, Entry};

const FOOD: &[&str] = &["minecraft:cod", "minecraft:salmon"];
static UNTRUSTING: Cond = Cond::Not(&Cond::Is("ocelot:trusting"));
static AVOID: avoid_entity::Params = avoid_entity::Params { what: Avoid::Player, max_dist: 16.0, walk: 0.8, sprint: 1.33, ..avoid_entity::Params::DEFAULT };

pub static DEF: Def = Def {
    name: "minecraft:ocelot",
    targets: &[
        Entry::new(&nearest_target::MODULE, 1, &nearest_target::Params { types: &["minecraft:chicken"], must_see: false, ..nearest_target::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 1, &nearest_target::Params { types: &["minecraft:turtle"], predicate: Some("baby_on_land"), must_see: false, ..nearest_target::Params::DEFAULT }),
    ],
    goals: &[
        Entry::new(&float::MODULE, 1, &float::Params::DEFAULT),
        Entry::new(&tempt::MODULE, 3, &tempt::Params { speed: 0.6, items: FOOD, ..tempt::Params::DEFAULT }),
        Entry::new(&leap_at_target::MODULE, 7, &leap_at_target::Params { yd: 0.3 }),
        Entry::new(&ocelot_attack::MODULE, 8, &ocelot_attack::Params::DEFAULT),
        Entry::new(&breed::MODULE, 9, &breed::Params { speed: 0.8, foods: FOOD, ..breed::Params::DEFAULT }),
        Entry::new(&stroll::MODULE, 10, &stroll::Params { speed: 0.8, water_avoiding: 1.000_000_1e-5, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 11, &look_at_player::Params { range: 10.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&avoid_entity::MODULE, 4, &AVOID).when(&UNTRUSTING),
    ],
    ..Def::EMPTY
};
