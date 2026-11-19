//! Réapprovisionnement des offres (`Villager.shouldRestock` et `restock`, dans `Trader` du client : les constantes d'intervalle y
//! sont, pas recopiées ici). Rien ne scrute : l'événement de travail réveille le but (`on_work`), et le but se réarme sur une
//! échéance de roue, au prochain instant où un réapprovisionnement est permis (`Trader::next_check`, ASSUMED).
use super::schedule::Times;
use super::trade_offers::{clock, Merchant};
use super::*;
use crate::goals::*;

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

/// Le prochain instant où `should_restock` peut changer de réponse : 2401 ticks après le dernier réapprovisionnement s'il y en a eu un
/// aujourd'hui, sinon le changement de jour (12001) ; un échange réveille le marchand de lui-même.
fn next_check(t: &horizon_client_lib::merchant::Trader, game: i64) -> i64 {
    let at = t.last_restock + if t.restocks_today == 1 { 2401 } else { 12001 };
    at.max(game + 1)
}

/// Ticks avant le prochain contrôle, selon l'état de réapprovisionnement du marchand.
fn delay(w: &World, e: Entity) -> i32 {
    let (game, _) = clock(w, e);
    w.get::<Merchant>(e).map_or(1, |m| (next_check(&m.trade, game) - game).clamp(1, i64::from(i32::MAX)) as i32)
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
    let (game, _) = clock(w, e);
    due_at(w, e, game)
}

fn start(w: &mut World, e: Entity, inst: Inst) {
    let (game, _) = clock(w, e);
    restock_at(w, e, game);
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}

fn stop(w: &mut World, e: Entity, inst: Inst) {
    arm(w, e, inst);
}

/// Le villageois est allé à son poste (`AtWork`) : le réapprovisionnement est demandé maintenant, pas à l'échéance suivante.
pub fn on_work(w: &mut World, e: Entity) {
    wake(w, e, &MODULE);
}
