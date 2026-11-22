//! `WanderingTrader` (`mobs/trader.rs:30-89`), 18 goals in the `Mob`, all here: float (0), the potion of invisibility in the dark and the milk by
//! day (`drink_item`, 0), the trade (`trade`, 1: `TradeWithPlayer` and `LookAtTradingPlayer` are one entry with both flags), the avoidance of
//! the zombie family (8), evoker (12), vindicator (8), vex (8), pillager (15), illusioner (12) and zoglin (10) at 0,5 (1), panic 0,5 (1), the walk to
//! the place of its visit (`wander_to_position`, 2: laid by `send_to`), the return to its restriction (4), stroll avoiding water 0,35 (8), the
//! look at the player (9, `period: 1`: the interact look of probability 1) and at other creatures within 8 blocks (10). Its lamas are held
//! on its lead (`leash::attach(llama, Holder::Entity(trader))`). Not done: the item in the hand, the dimension with a fixed time (no model).
use super::Def;
use crate::behaviours::{avoid_entity, drink_item, float, look_at_player, move_towards_restriction, panic, stroll, trade_session, wander_to_position, Entry};

pub static DEF: Def = Def {
    name: "minecraft:wandering_trader",
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&drink_item::MODULE, 0, &drink_item::Params::POTION),
        Entry::new(&drink_item::MODULE, 0, &drink_item::Params::MILK),
        Entry::new(&trade_session::MODULE, 1, &trade_session::Params { wandering: true }),
        Entry::new(&avoid_entity::MODULE, 1, &avoid_entity::Params { what: avoid_entity::Avoid::Named(&["minecraft:zombie", "minecraft:husk", "minecraft:drowned", "minecraft:zombie_villager", "minecraft:zombified_piglin"]), max_dist: 8.0, walk: 0.5, sprint: 0.5, ..avoid_entity::Params::DEFAULT }),
        Entry::new(&avoid_entity::MODULE, 1, &avoid_entity::Params { what: avoid_entity::Avoid::Named(&["minecraft:evoker"]), max_dist: 12.0, walk: 0.5, sprint: 0.5, ..avoid_entity::Params::DEFAULT }),
        Entry::new(&avoid_entity::MODULE, 1, &avoid_entity::Params { what: avoid_entity::Avoid::Named(&["minecraft:vindicator"]), max_dist: 8.0, walk: 0.5, sprint: 0.5, ..avoid_entity::Params::DEFAULT }),
        Entry::new(&avoid_entity::MODULE, 1, &avoid_entity::Params { what: avoid_entity::Avoid::Named(&["minecraft:vex"]), max_dist: 8.0, walk: 0.5, sprint: 0.5, ..avoid_entity::Params::DEFAULT }),
        Entry::new(&avoid_entity::MODULE, 1, &avoid_entity::Params { what: avoid_entity::Avoid::Named(&["minecraft:pillager"]), max_dist: 15.0, walk: 0.5, sprint: 0.5, ..avoid_entity::Params::DEFAULT }),
        Entry::new(&avoid_entity::MODULE, 1, &avoid_entity::Params { what: avoid_entity::Avoid::Named(&["minecraft:illusioner"]), max_dist: 12.0, walk: 0.5, sprint: 0.5, ..avoid_entity::Params::DEFAULT }),
        Entry::new(&avoid_entity::MODULE, 1, &avoid_entity::Params { what: avoid_entity::Avoid::Named(&["minecraft:zoglin"]), max_dist: 10.0, walk: 0.5, sprint: 0.5, ..avoid_entity::Params::DEFAULT }),
        Entry::new(&panic::MODULE, 1, &panic::Params { speed: 0.5, ..panic::Params::DEFAULT }),
        Entry::new(&wander_to_position::MODULE, 2, &wander_to_position::Params::DEFAULT),
        Entry::new(&move_towards_restriction::MODULE, 4, &move_towards_restriction::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 8, &stroll::Params { speed: 0.35, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::INTERACT, 9, &look_at_player::Params { range: 3.0, period: 1, ..look_at_player::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 10, &look_at_player::Params { range: 8.0, species: look_at_player::Params::ALL, ..look_at_player::Params::DEFAULT }),
    ],
    rules: &[crate::rules::RuleEntry::new(&crate::leash::NEVER, &())],
    ..Def::EMPTY
};
