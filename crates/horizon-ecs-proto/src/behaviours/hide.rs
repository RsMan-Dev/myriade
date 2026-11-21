//! `LocateHidingPlace` and `SetHiddenState` (`villager_brain.rs:2530-2591`): after a bell or a raid the villager walks to a home
//! (the nearest one if it is close, else a random one within `radius`), and stays there 300 ticks from its arrival.
//! Speed and closeness come from the species: hide 0.625 / 2, raid defeat 0.7 / 1.
use super::bell_heard::HEARD_BELL;
use super::block_reach::{centre_dist_sq, walk_to};
use super::{raid, schedule};
use crate::brain::*;
use crate::cow::{now_of, Pos};
use crate::goals::with_rng;
use bevy_ecs::prelude::*;

/// Present from the bell to the end of the hiding: the requirement of the `hide` activity (the jar poses it with `setActiveActivityIfPossible`).
pub static HIDE_ON: MemKey = MemKey::new("horizon:hide_on", Shape::Unit);
pub static HIDING_PLACE: MemKey = MemKey::new("minecraft:hiding_place", Shape::Block);
/// Set on arrival with a ttl of 300 ticks: once it expires the hiding is over.
pub static HIDDEN: MemKey = MemKey::new("horizon:hidden", Shape::Bool);
/// Set on arrival, without ttl: tells an expired `HIDDEN` from one not yet set.
pub static ARRIVED: MemKey = MemKey::new("horizon:hidden_arrived", Shape::Unit);

pub struct Params {
    /// The homes within `radius` of the entity, nearest first.
    pub homes: fn(&World, Entity, i32) -> Vec<[i32; 3]>,
    pub radius: i32,
    pub speed: f32,
    pub close: i32,
    /// The villager's `Home`, the last place when no bed is in range (`mm.home.value()`).
    pub home: &'static MemKey,
}

/// `ReactToBell`: the bell rang and no raid is on: the `hide` activity (`raid_nearby` is any raid status but none).
pub static REACT_TO_BELL: Behaviour = Behaviour { name: "minecraft:react_to_bell", mems: &[&HIDE_ON], one_shot: true, can_start: react_to_bell, ..Behaviour::DEFAULT };

fn react_to_bell(w: &mut World, e: Entity, _: &Ctx) -> bool {
    if int(w, e, &HEARD_BELL).is_some() && raid::status(w, e) == raid::RAID_NONE {
        set(w, e, &HIDE_ON, MemValue::Unit, None);
    }
    true
}

/// `LocateHidingPlace`.
pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:villager_locate_hiding_place", mems: &[&WALK_TARGET, &HIDING_PLACE, &LOOK_TARGET, &HIDE_ON], one_shot: true, can_start: locate, ..Behaviour::DEFAULT };

/// `SetHiddenState`.
pub static HIDDEN_STATE: Behaviour = Behaviour { name: "minecraft:villager_set_hidden_state", mems: &[&HIDING_PLACE, &HIDDEN, &ARRIVED, &HEARD_BELL, &HIDE_ON], one_shot: true, can_start: hidden, ..Behaviour::DEFAULT };

fn locate(w: &mut World, e: Entity, c: &Ctx) -> bool {
    // `SetHiddenState` ran earlier in this step and ended the hiding: the jar changes the activity at once, here it changes at the step's end.
    // The bell's `hide` activity holds `HIDE_ON`; the raid package's `LocateHidingPlace` runs in the `raid` activity (`RAID_ON`).
    if present(w, e, &WALK_TARGET) || !(present(w, e, &HIDE_ON) || present(w, e, &raid::RAID_ON)) {
        return false;
    }
    let p = c.get::<Params>();
    let Some(at) = w.get::<Pos>(e).map(|q| q.0) else { return false };
    let r2 = f64::from(p.close * p.close);
    let near = (p.homes)(w, e, p.close + 1).into_iter().next().filter(|q| centre_dist_sq(*q, at) < r2);
    let place = near.or_else(|| {
        let mut all = (p.homes)(w, e, p.radius);
        for i in (2..=all.len()).rev() {
            let j = with_rng(w, e, |r| r.next_int(i as i32)) as usize;
            all.swap(i - 1, j);
        }
        all.into_iter().next()
    }).or_else(|| match get(w, e, p.home) {
        Some(MemValue::Block(q)) => Some(*q),
        _ => None,
    });
    if let Some(q) = place {
        erase(w, e, &LOOK_TARGET);
        set(w, e, &HIDING_PLACE, MemValue::Block(q), None);
        if centre_dist_sq(q, at) >= r2 {
            walk_to(w, e, q, p.speed, p.close);
        }
    }
    true
}

/// Ends the hiding when the bell's 300 ticks are over, or when the ttl of `HIDDEN` has run out after the arrival.
fn hidden(w: &mut World, e: Entity, _: &Ctx) -> bool {
    let Some(&MemValue::Block(place)) = get(w, e, &HIDING_PLACE) else { return false };
    let Some(heard) = int(w, e, &HEARD_BELL) else { return false };
    let Some(at) = w.get::<Pos>(e).map(|q| q.0) else { return false };
    let now = now_of(w, e) as i64;
    let arrived = present(w, e, &ARRIVED);
    if heard + 300 <= now || (arrived && !present(w, e, &HIDDEN)) {
        for k in [&HEARD_BELL, &HIDING_PLACE, &HIDDEN, &ARRIVED, &HIDE_ON] {
            erase(w, e, k);
        }
        restart(w, e, &schedule::BEHAVIOUR);
    } else if !arrived && centre_dist_sq(place, at) < 9.0 {
        set(w, e, &ARRIVED, MemValue::Unit, None);
        set(w, e, &HIDDEN, MemValue::Bool(true), Some(300));
    }
    true
}
