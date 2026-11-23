//! `ShowTrades`: a merchant holds up the results of the trades a player's held item pays for, and cycles them every 40 ticks
//! (`displayCyclingItems`). Started by the player's held item, not scanned: the `show` call comes from the event that sends the
//! player's hand (`goals::Player.main`), and the goal re-arms itself on its wheel deadline while `Showing` exists.
use super::trade_offers::{ensure_offers, Merchant};
use super::*;
use myriade_engine_lib::merchant::MerchantOffer;
use crate::cow::{GrowAt, Pos};
use crate::goals::*;
use crate::rules::trade_show::{cycle_done, in_show_range, next_index, CYCLE, LOOK_TICKS};

/// The trades shown to one player: `items` are the offers' results for the held item, `index` the one in hand.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct Showing {
    pub player: u32,
    pub held: u32,
    pub items: Vec<usize>,
    pub cycle: u32,
    pub index: usize,
    pub look: u32,
}

pub static MODULE: Module = Module {
    name: "trade_show",
    flags: LOOK,
    arm,
    can_use,
    start,
    stop,
    requires: &[Cap::Body],
    ..Module::DEFAULT
};

fn arm(w: &mut World, e: Entity, inst: Inst) {
    crate::goals::arm(w, e, inst.slot, CYCLE);
}

/// `show_trades_condition` and the look timer: the player is the target, the merchant is alive, adult, and within 17 squared.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(s) = w.get::<Showing>(e) else { return false };
    let (player, look) = (s.player, s.look);
    let Some(p) = w.resource::<Online>().get(player) else { return false };
    let pos = w.get::<Pos>(e).map_or([0.0; 3], |q| [q.0.x, q.0.y, q.0.z]);
    let d2 = (p.x - pos[0]).powi(2) + (p.y - pos[1]).powi(2) + (p.z - pos[2]).powi(2);
    look > 0 && w.get::<GrowAt>(e).is_none() && in_show_range(d2)
}

/// One cycle: the held item is compared with the offers when it changes, the shown result moves on every 40 ticks.
fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(mut s) = w.get_mut::<Showing>(e) else { return };
    s.look = s.look.saturating_sub(CYCLE as u32);
    if s.items.len() >= 2 {
        s.cycle += 1;
        if cycle_done(s.cycle) {
            s.cycle = 0;
            s.index = next_index(s.index, s.items.len());
        }
    }
    let shown = s.items.get(s.index).copied();
    drop(s);
    if let Some(offer) = shown {
        show_result(w, e, offer);
    }
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}

fn stop(w: &mut World, e: Entity, inst: Inst) {
    if w.get::<Showing>(e).is_some_and(|s| s.look > 0) {
        arm(w, e, inst);
    } else if w.get::<Showing>(e).is_some() {
        clear_display(w, e);
    }
}

/// The display is cleared: no item in the main hand, and its drop chance back to the default (`Mob.setDropChance` of the slot).
fn clear_display(w: &mut World, e: Entity) {
    crate::goals::set_main_hand(w, e, None);
    crate::spawn_gear::set_drop_chance(w, e, Slot::Main, crate::equipment_drop::DEFAULT);
}

/// `findItemsToDisplay`: the offer's first or second cost is the held item (the client compares item kinds; here their ids).
fn pays(o: &MerchantOffer, held: u32) -> bool {
    item_id(&o.base_cost_a.stack.item) == held || o.cost_b.as_ref().is_some_and(|c| item_id(&c.stack.item) == held)
}

/// `setItemInHand`: the shown result goes to the main hand, which never drops it (the drop chance is zero while it shows).
fn show_result(w: &mut World, e: Entity, offer: usize) {
    let Some(m) = w.get::<Merchant>(e) else { return };
    let Some(result) = m.trade.offers.get(offer).map(|o| o.result.clone()) else { return };
    crate::goals::set_main_hand(w, e, Some(result));
    crate::spawn_gear::set_drop_chance(w, e, Slot::Main, 0.0);
}

/// The player's held item changed (`player.main`): the offers it pays for are listed, the cycle starts at the first.
/// Makes the offers when the held item is set and they are not yet (`VillagerOffers` in the client).
pub fn show(w: &mut World, e: Entity, player: u32) {
    let held = w.resource::<Online>().get(player).map_or(0, |p| p.main);
    if held == 0 {
        if let Some(mut s) = w.get_mut::<Showing>(e) {
            s.held = 0;
            s.items.clear();
        }
        clear_display(w, e);
        return;
    }
    ensure_offers(w, e);
    let Some(m) = w.get::<Merchant>(e) else { return };
    let items: Vec<usize> = m
        .trade
        .offers
        .iter()
        .enumerate()
        .filter(|(_, o)| !o.out_of_stock() && pays(o, held))
        .map(|(i, _)| i)
        .collect();
    let first = items.first().copied();
    w.entity_mut(e).insert(Showing { player, held, items, cycle: 0, index: 0, look: LOOK_TICKS });
    if let Some(i) = first {
        show_result(w, e, i);
        wake(w, e, &MODULE);
    } else {
        clear_display(w, e);
    }
}
