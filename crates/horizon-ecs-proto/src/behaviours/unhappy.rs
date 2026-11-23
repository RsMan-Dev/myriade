//! `AbstractVillager.DATA_UNHAPPY_COUNTER`: the head shake of a refused trade. The counter is a deadline of the wheel, not a
//! count per tick; the client counts the shake down from the synched start value.
use crate::cow::{after, now_of};
use bevy_ecs::prelude::*;

/// `AbstractVillager.setUnhappy`: the counter starts at 40 ticks.
pub const TICKS: u64 = 40;

/// The shake is running; `until` is the own-time tick it ends at.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Unhappy {
    pub until: u64,
}

/// Starts (or restarts) the shake and arms its end.
pub fn start(w: &mut World, e: Entity) {
    let until = now_of(w, e) + TICKS;
    if let Ok(mut ent) = w.get_entity_mut(e) {
        ent.insert(Unhappy { until });
        after(w, e, TICKS as i32, end);
    }
}

/// `Villager.setUnhappy`: the no sound (at the voice pitch of the villager, `ram::voice_pitch`) and the head shake.
pub fn say_no(w: &mut World, e: Entity) {
    let Some(wandering) = w.get::<crate::behaviours::trade_offers::Merchant>(e).map(|m| m.wandering) else { return };
    start(w, e);
    let name = if wandering { "entity.wandering_trader.no" } else { "entity.villager.no" };
    let pitch = crate::behaviours::ram::voice_pitch(w, e);
    let at = w.get::<crate::cow::Pos>(e).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
    w.resource_mut::<crate::goals::Events>().host.sound(name, at, 1.0, pitch);
}

/// The wheel's deadline: ends the shake unless it was restarted since.
fn end(w: &mut World, e: Entity) {
    let now = now_of(w, e);
    if w.get::<Unhappy>(e).is_some_and(|u| now >= u.until) {
        w.entity_mut(e).remove::<Unhappy>();
    }
}
