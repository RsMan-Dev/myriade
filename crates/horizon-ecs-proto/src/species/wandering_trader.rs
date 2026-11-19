//! `WanderingTrader` (`mobs/trader.rs:30-89`), 18 goals in the `Mob`: 11 are here, 7 are not (the trade is excluded by the task).
//! In: float (0), the avoidance of the zombie family (8 blocks), the vex (8) and the zoglin (10) at 0,5 (1: three entries, the species
//! lists that exist), panic 0,5 (1), stroll avoiding water 0,35 (8), look at the player within 3 blocks (9). Not in, and why:
//! `TradeWithPlayer` and `LookAtTradingPlayer` (the trade: another lot), `DrinkItem` x2 (the potion of invisibility and the milk: inventory
//! use of an item and the effect by day and night), the avoidance of the evoker (12), vindicator (8), pillager (15) and illusioner (12)
//! (their species are absent: the entries exist the day they do, with the same parameters), `WanderToPosition` (2: walks to the reference
//! point of its visit, 2 blocks, 0,35: needs a visit point component), `MoveTowardsRestriction` (4: 0,35, the home: absent module), the
//! look at other creatures within 8 blocks (`LookAtPlayer::mobs`: `look_at_player` has no species option). Its lamas are held on its lead
//! (`leash::attach(llama, Holder::Entity(trader))`).
use super::Def;
use crate::behaviours::{avoid_entity, float, look_at_player, panic, stroll, Entry};

pub static DEF: Def = Def {
    name: "minecraft:wandering_trader",
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&avoid_entity::MODULE, 1, &avoid_entity::Params { what: avoid_entity::Avoid::Named(&["minecraft:zombie", "minecraft:husk", "minecraft:drowned", "minecraft:zombie_villager", "minecraft:zombified_piglin"]), max_dist: 8.0, walk: 0.5, sprint: 0.5, ..avoid_entity::Params::DEFAULT }),
        Entry::new(&avoid_entity::MODULE, 1, &avoid_entity::Params { what: avoid_entity::Avoid::Named(&["minecraft:vex"]), max_dist: 8.0, walk: 0.5, sprint: 0.5, ..avoid_entity::Params::DEFAULT }),
        Entry::new(&avoid_entity::MODULE, 1, &avoid_entity::Params { what: avoid_entity::Avoid::Named(&["minecraft:zoglin"]), max_dist: 10.0, walk: 0.5, sprint: 0.5, ..avoid_entity::Params::DEFAULT }),
        Entry::new(&panic::MODULE, 1, &panic::Params { speed: 0.5, ..panic::Params::DEFAULT }),
        Entry::new(&stroll::MODULE, 8, &stroll::Params { speed: 0.35, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 9, &look_at_player::Params { range: 3.0, ..look_at_player::Params::DEFAULT }),
    ],
    ..Def::EMPTY
};
