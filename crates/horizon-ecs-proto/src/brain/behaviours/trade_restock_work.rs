//! The restock of `WorkAtPoi.start` (`restock_if_due`): called by the work at the site (`composter`), after it, not by a behaviour of
//! its own. The due test and the restock are `behaviours::trade_restock`'s, on the game time of the world.
use crate::behaviours::trade_restock::{due_at, restock_at};
use crate::cow::game_now;
use bevy_ecs::prelude::{Entity, World};

/// `Villager.shouldRestock` then `restock`: a villager with a trader restocks its offers when the day is due.
pub fn restock_if_due(w: &mut World, e: Entity) {
    let game = game_now(w);
    if due_at(w, e, game) {
        restock_at(w, e, game);
    }
}
