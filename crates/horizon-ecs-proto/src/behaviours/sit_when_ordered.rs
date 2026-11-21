//! `SitWhenOrderedToGoal`: a pet that is ordered to sit (the occupation `SITTING`, written by the taming rule's interaction) or is
//! tame stops and stays, unless it is in water or in the air, or its owner was hurt a moment ago and is within 12 blocks (the pet
//! then gets up to help). The order ends it (`tame::order_sit`). What wakes it: the order, landing, leaving a fluid.
use super::*;
use crate::{cow::*, goals::*, tame};
use crate::rules::sit_when_ordered::owner_needs_help;

pub static MODULE: Module = Module {
    name: "sit_when_ordered",
    flags: JUMP | MOVE,
    can_use,
    start,
    systems,
    requires: &[Cap::Tamable, Cap::Locomotion],
    ..Module::DEFAULT
};
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(wake_events.after(apply_damage).in_set(slot));
}
/// Landing or leaving a fluid: the order that could not be obeyed is asked again.
fn wake_events(ev: Res<Events>, q: Query<&Occupied>, mut reqs: ResMut<Requests>, goals: Query<&GoalSet>) {
    for e in ev.landed.iter().chain(ev.left_fluid.iter()) {
        if q.get(e).is_ok_and(|o| o.0 & OCC_SITTING != 0)
            && let Ok(g) = goals.get(e)
            && let Some(slot) = g.slot_of(&MODULE)
        {
            reqs.0.push(Req::Start(e, slot));
        }
    }
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let sitting = is_occupied(w, e, OCC_SITTING);
    let tame = w.get::<tame::Owner>(e).is_some_and(|o| o.0.is_some());
    if !sitting && !tame {
        return false;
    }
    let (in_water, on_ground) = (w.get::<FluidState>(e).is_some_and(|f| f.in_water), w.get::<Contact>(e).is_some_and(|c| c.on_ground));
    if in_water || !on_ground {
        return false;
    }
    match tame::owner_distance_sqr(w, e) {
        None => true,
        Some(d) => {
            let attacked = match w.get::<tame::Owner>(e).and_then(|o| o.0) {
                Some(crate::targeting::Who::Player(id)) => {
                    let now = w.resource::<Clock>().0;
                    w.resource::<crate::hostile::PlayerState>().hurt_by.get(id as usize).copied().flatten().is_some_and(|(_, at)| now.saturating_sub(at) <= 100)
                }
                _ => false,
            };
            !owner_needs_help(d, attacked) && sitting
        }
    }
}
fn start(w: &mut World, e: Entity, _: Inst) {
    stop_nav(w, e);
}
