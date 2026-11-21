//! `ShowTrades` (`villager_brain.rs:1603`, `:2826`, `:3150`): an adult villager whose interaction target is a player within 17 squared
//! looks at it and shows the results of the trades the player's held item pays for: the first at once, then one every 40 ticks
//! (`displayCyclingItems`). The look lasts 900 ticks while a trade is shown, 40 without one; both are deadlines (memories with a ttl).
use crate::behaviours::play_tag::INTERACTION_TARGET;
use crate::behaviours::trade_offers::{ensure_offers, Merchant};
use crate::brain::*;
use crate::cow::{GrowAt, Pos};
use crate::goals::{d2, set_main_hand, Online};
use crate::items::item_id;
use crate::targeting::{info, Who};
use bevy_ecs::prelude::Component;

pub static SHOW_LOOK: MemKey = MemKey::new("horizon:villager_show_look", Shape::Unit);
pub static SHOW_CYCLE: MemKey = MemKey::new("horizon:villager_show_cycle", Shape::Unit);

const LOOK_SHOWN: u64 = 900;
const LOOK_NONE: u64 = 40;
const CYCLE: u64 = 40;

/// The trades shown: the held item they were chosen for, the offers that pay it, and the one in hand (an index into `items`).
#[derive(Component)]
pub struct Shown {
    held: u32,
    items: Vec<usize>,
    index: usize,
}

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_show_trades",
    mems: &[&INTERACTION_TARGET, &LOOK_TARGET, &SHOW_LOOK, &SHOW_CYCLE],
    can_start,
    start,
    tick,
    stop,
    ..Behaviour::DEFAULT
};

/// The player shown to: the interaction target, when it is a player in reach of an adult villager.
fn shown_player(w: &World, e: Entity) -> Option<u32> {
    let Some(MemValue::Who(Who::Player(p))) = get(w, e, &INTERACTION_TARGET) else { return None };
    if w.get::<GrowAt>(e).is_some() {
        return None;
    }
    let pos = w.get::<Pos>(e)?.0;
    info(w, Who::Player(*p)).filter(|i| d2(pos, i.pos) <= 17.0).map(|_| *p)
}

/// The offers the held item pays for, not out of stock (`findItemsToDisplay`).
fn matching(w: &World, e: Entity, held: u32) -> Vec<usize> {
    let Some(m) = w.get::<Merchant>(e) else { return Vec::new() };
    m.trade
        .offers
        .iter()
        .enumerate()
        .filter(|(_, o)| !o.out_of_stock() && (item_id(&o.base_cost_a.stack.item) == held || o.cost_b.as_ref().is_some_and(|c| item_id(&c.stack.item) == held)))
        .map(|(i, _)| i)
        .collect()
}

fn show_offer(w: &mut World, e: Entity, offer: usize) {
    let Some(stack) = w.get::<Merchant>(e).and_then(|m| m.trade.offers.get(offer)).map(|o| o.result.clone()) else { return };
    set_main_hand(w, e, Some(stack));
}

/// The held item changed: the offers are listed again and the first one is shown (`getOffers` makes them the first time).
fn restart(w: &mut World, e: Entity, held: u32) {
    if held != 0 {
        ensure_offers(w, e);
    }
    let items = if held == 0 { Vec::new() } else { matching(w, e, held) };
    let (first, n) = (items.first().copied(), items.len());
    w.entity_mut(e).insert(Shown { held, items, index: 0 });
    match first {
        Some(i) => {
            show_offer(w, e, i);
            set(w, e, &SHOW_LOOK, MemValue::Unit, Some(LOOK_SHOWN));
            if n >= 2 {
                set(w, e, &SHOW_CYCLE, MemValue::Unit, Some(CYCLE));
            }
        }
        None => {
            set_main_hand(w, e, None);
            set(w, e, &SHOW_LOOK, MemValue::Unit, Some(LOOK_NONE));
        }
    }
}

/// The cycle's deadline passed: the next result is shown and the deadline is set again (nothing with fewer than two results).
fn cycle(w: &mut World, e: Entity) {
    let offer = {
        let Some(mut s) = w.get_mut::<Shown>(e) else { return };
        if s.items.len() < 2 {
            return;
        }
        s.index = (s.index + 1) % s.items.len();
        s.items[s.index]
    };
    show_offer(w, e, offer);
    set(w, e, &SHOW_CYCLE, MemValue::Unit, Some(CYCLE));
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    shown_player(w, e).is_some()
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    let Some(p) = shown_player(w, e) else { return };
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(Who::Player(p), true)), None);
    set(w, e, &SHOW_LOOK, MemValue::Unit, Some(LOOK_NONE));
    w.entity_mut(e).insert(Shown { held: 0, items: Vec::new(), index: 0 });
}

fn tick(w: &mut World, e: Entity, _: &Ctx) -> bool {
    let Some(p) = shown_player(w, e) else { return false };
    if !present(w, e, &SHOW_LOOK) {
        return false;
    }
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(Who::Player(p), true)), None);
    let held = w.resource::<Online>().get(p).map_or(0, |pl| pl.main);
    if w.get::<Shown>(e).is_none_or(|s| s.held != held) {
        restart(w, e, held);
    } else if !present(w, e, &SHOW_CYCLE) {
        cycle(w, e);
    }
    true
}

fn stop(w: &mut World, e: Entity, _: &Ctx) {
    w.entity_mut(e).remove::<Shown>();
    erase(w, e, &SHOW_LOOK);
    erase(w, e, &SHOW_CYCLE);
}
