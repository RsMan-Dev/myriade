//! `GiveGiftToHero` (`villager_brain.rs:1579`, `:2790`, `:3142`, `:3868`): with the nearest visible player a hero of the village (`Heroes`),
//! the villager waits 600 ticks once (`until_next`), then takes the player as its interaction target, looks at it and walks to it (speed 0.5,
//! close 5). Within 5 blocks, 20 ticks after it started, it throws one roll of the gift table `table`. The entry lasts 100 ticks.
//! The 600-tick wait counts from the first check (a deadline, not a count of the checks made while the hero is visible).
use crate::behaviours::food_gift::Heroes;
use crate::behaviours::play_tag::INTERACTION_TARGET;
use crate::brain::*;
use crate::cow::Pos;
use crate::goals::with_rng;
use crate::item::drop_item;
use crate::loot::roll_table;
use horizon_core::geometry::Vec3;
use crate::targeting::{info, Who};

pub struct Params {
    /// The gift table of this villager, from the world (its profession, or the baby's).
    pub table: fn(&World, Entity) -> String,
}

/// Set at the start for 20 ticks: the gift is thrown when it has run out and the hero is close.
pub static GIFT_READY: MemKey = MemKey::new("horizon:villager_gift_ready", Shape::Unit);
/// Set for the 600 ticks of the first wait, and `GIFT_DELAYED` (permanent) records that the wait was taken.
pub static GIFT_WAIT: MemKey = MemKey::new("horizon:villager_gift_wait", Shape::Unit);
pub static GIFT_DELAYED: MemKey = MemKey::new("horizon:villager_gift_delayed", Shape::Unit);

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_give_gift_to_hero",
    mems: &[&NEAREST_VISIBLE_PLAYER, &INTERACTION_TARGET, &LOOK_TARGET, &WALK_TARGET, &GIFT_READY, &GIFT_WAIT, &GIFT_DELAYED],
    can_start,
    start,
    tick,
    ..Behaviour::DEFAULT
};

/// The nearest visible player when that player is a hero of the village.
fn hero(w: &World, e: Entity) -> Option<u32> {
    let Some(MemValue::Who(Who::Player(id))) = get(w, e, &NEAREST_VISIBLE_PLAYER) else { return None };
    w.get_resource::<Heroes>().filter(|h| h.0.contains(id)).map(|_| *id)
}

/// The blocks of both are within 5 blocks (25 squared, `BlockPos.closerThan`).
fn close(me: Vec3, at: Vec3) -> bool {
    let block = |v: Vec3| [v.x.floor() as i32, v.y.floor() as i32, v.z.floor() as i32];
    let (a, b) = (block(me), block(at));
    (0..3).map(|i| (a[i] - b[i]).pow(2)).sum::<i32>() < 25
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    if hero(w, e).is_none() || present(w, e, &GIFT_WAIT) {
        return false;
    }
    if present(w, e, &GIFT_DELAYED) {
        return true;
    }
    set(w, e, &GIFT_DELAYED, MemValue::Unit, None);
    set(w, e, &GIFT_WAIT, MemValue::Unit, Some(600));
    false
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    let Some(p) = hero(w, e) else { return };
    set(w, e, &INTERACTION_TARGET, MemValue::Who(Who::Player(p)), None);
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(Who::Player(p), true)), None);
    set(w, e, &GIFT_READY, MemValue::Unit, Some(20));
}

fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let Some(p) = hero(w, e) else { return false };
    let who = Who::Player(p);
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(who, true)), None);
    let (Some(me), Some(at)) = (w.get::<Pos>(e).map(|q| q.0), info(w, who).map(|i| i.pos)) else { return false };
    if !close(me, at) {
        set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Entity(who, true), speed: 0.5, close: 5 }), None);
        return true;
    }
    if present(w, e, &GIFT_READY) {
        return true;
    }
    let table = (c.get::<Params>().table)(w, e);
    let seed = with_rng(w, e, |r| r.next_int(i32::MAX));
    let from = Vec3::new(me.x, me.y + 1.0, me.z);
    for stack in roll_table(w, &table, i64::from(seed)) {
        drop_item(w, stack, from, Vec3::new(0.0, 0.3, 0.0), 40);
    }
    false
}
