//! Réapprovisionnement des offres (`Villager.shouldRestock` et `restock`, dans `Trader` du client : les constantes d'intervalle y
//! sont, pas recopiées ici). Appelé par le travail au poste (`trade_restock_work::restock_if_due`) ; le but se réarme sur une
//! échéance de roue, au prochain instant où un réapprovisionnement est permis (`Trader::next_check`, ASSUMED).
use super::schedule::Times;
use super::trade_offers::Merchant;
use super::*;
use crate::cow::game_now;
use crate::goals::*;
use crate::rules::trade_restock::{next_check_at, ticks_until};

pub static MODULE: Module = Module {
    name: "trade_restock",
    flags: 0,
    arm,
    can_use,
    start,
    stop,
    requires: &[Cap::Body],
    ..Module::DEFAULT
};

/// Ticks avant le prochain contrôle, selon l'état de réapprovisionnement du marchand.
fn delay(w: &World, e: Entity) -> i32 {
    let game = game_now(w);
    w.get::<Merchant>(e).map_or(1, |m| ticks_until(next_check_at(m.trade.last_restock, m.trade.restocks_today, game), game))
}

fn arm(w: &mut World, e: Entity, inst: Inst) {
    let wait = delay(w, e);
    crate::goals::arm(w, e, inst.slot, wait);
}

/// `Trader::should_restock` au temps de jeu `game` : dû maintenant ?
pub fn due_at(w: &mut World, e: Entity, game: i64) -> bool {
    let day = Times::absolute(w);
    w.get_mut::<Merchant>(e).is_some_and(|mut m| m.trade.should_restock(game, day))
}

/// `Trader::restock` au temps de jeu `game` : usages remis à zéro, demande montée (`update_demand`, dans le client).
pub fn restock_at(w: &mut World, e: Entity, game: i64) {
    if let Some(mut m) = w.get_mut::<Merchant>(e) {
        m.trade.restock(game);
    }
}

fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let game = game_now(w);
    due_at(w, e, game)
}

fn start(w: &mut World, e: Entity, inst: Inst) {
    let game = game_now(w);
    restock_at(w, e, game);
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}

fn stop(w: &mut World, e: Entity, inst: Inst) {
    arm(w, e, inst);
}
