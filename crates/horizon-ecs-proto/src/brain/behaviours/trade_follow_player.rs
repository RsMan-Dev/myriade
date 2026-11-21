//! `FollowTradingPlayer` (`villager_brain.rs:4051`, `follow_trading_condition` `:3275`): while the villager trades with a player (`Trading`,
//! the Mob's flag2), is alive, not in water, not hurt in the last 10 ticks and the player is within 4 blocks (16 squared), it looks at the
//! player and walks after it (speed 0.5, close 2). Only `Trading` gives the player: no other behaviour follows it.
use crate::behaviours::trade_session::Trading;
use crate::brain::*;
use crate::cow::{now_of, FluidState, HurtAt, Pos};
use crate::goals::d2;
use crate::targeting::{info, Who};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_follow_trading_player",
    mems: &[&LOOK_TARGET, &WALK_TARGET],
    can_start,
    start,
    tick,
    stop,
    ..Behaviour::DEFAULT
};

/// The trading player, when the follow conditions hold.
fn trader(w: &World, e: Entity) -> Option<u32> {
    let p = w.get::<Trading>(e)?.player;
    let pos = w.get::<Pos>(e)?.0;
    let at = info(w, Who::Player(p))?.pos;
    let in_water = w.get::<FluidState>(e).is_some_and(|f| f.in_water);
    let hurt = w.get::<HurtAt>(e).is_some_and(|h| now_of(w, e) < h.at + 10);
    (!in_water && !hurt && d2(pos, at) <= 16.0).then_some(p)
}

fn aim(w: &mut World, e: Entity, p: u32) {
    let who = Who::Player(p);
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(who, true)), None);
    set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Entity(who, false), speed: 0.5, close: 2 }), None);
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    trader(w, e).is_some()
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    if let Some(p) = trader(w, e) {
        aim(w, e, p);
    }
}

fn tick(w: &mut World, e: Entity, _: &Ctx) -> bool {
    let Some(p) = trader(w, e) else { return false };
    aim(w, e, p);
    true
}

fn stop(w: &mut World, e: Entity, _: &Ctx) {
    erase(w, e, &WALK_TARGET);
}
