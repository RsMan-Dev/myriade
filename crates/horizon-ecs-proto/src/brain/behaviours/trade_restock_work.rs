//! The restock of `WorkAtPoi.start` (`villager_brain.rs:3345`, `restock_if_due`): a villager with a trader, when the day is due, restocks
//! its offers. One shot; the due test and the restock are `behaviours::trade_restock`'s, on the game time of `trade_offers::clock`.
use crate::behaviours::trade_offers::clock;
use crate::behaviours::trade_restock::{due_at, restock_at};
use crate::brain::*;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_restock",
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    let (game, _) = clock(w, e);
    due_at(w, e, game)
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    let (game, _) = clock(w, e);
    restock_at(w, e, game);
}
