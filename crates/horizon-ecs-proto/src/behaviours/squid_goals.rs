//! The goals of the squid (`Squid.SquidRandomMovementGoal`, `SquidFleeGoal`): they only set the vector the tentacle cycle pushes it along
//! (`swim::SquidState.vector`, `Mob.extra.vector`). No flag: they never conflict. Both tick without a system of their own (called by
//! `arbitrate` while a squid lives): the random movement on the ticks where tick + id is even (the selector's cadence), the flee every tick.
//! Not ported: `noActionTime > 100` zeroing the vector (the despawn counter is not in the frame).
use super::*;
use crate::locomotion::swim::SquidState;
use crate::{cow::*, goals::*};
use crate::rules::{retreat_on_hurt, spear_use::halved_use, squid_goals as rules};

#[derive(Clone, Copy, Debug)]
pub struct Params {}
impl Params {
    pub const DEFAULT: Self = Self {};
}
/// The random movement runs (from birth, for good: `canUse` is always true).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Wandering;
/// The flee runs.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Fleeing;

/// `SquidRandomMovementGoal`: every tick of the goal, one chance in `reduced(50)`, or out of the water, or with no vector: a new horizontal
/// direction at 0.2 and a vertical push of -0.1 to 0.1.
pub static SQUID_RANDOM_MOVEMENT: Module = Module { name: "squid_random_movement", tick: Some(wander_tick), flags: 0, arm: wander_arm, start: wander_start, stop: wander_stop, requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn wander_arm(w: &mut World, e: Entity, i: Inst) {
    arm(w, e, i.slot, 1);
}
fn wander_start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Wandering);
}
fn wander_stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Wandering>();
}
fn wander_tick(w: &mut World) {
    let es: Vec<Entity> = w.query_filtered::<Entity, With<Wandering>>().iter(w).collect();
    for e in es {
        if (now_of(w, e) + phase_of(w, e)) % 2 != 0 {
            continue;
        }
        let in_water = w.get::<FluidState>(e).is_some_and(|f| f.in_water);
        let Some(mut st) = w.get::<SquidState>(e).copied() else { continue };
        let (roll, a, y) = with_rng(w, e, |r| {
            let roll = r.next_int(halved_use(50, true)) == 0;
            // The angle and the height are drawn only when the condition needs them (the `||` of the `Mob`).
            if roll || !in_water || st.vector == [0.0; 3] {
                (true, r.next_float() * std::f32::consts::PI * 2.0, -0.1 + r.next_float() * 0.2)
            } else {
                (false, 0.0, 0.0)
            }
        });
        if roll {
            st.vector = rules::wander_vector(a, y);
            *w.get_mut::<SquidState>(e).unwrap() = st;
        }
    }
}

/// `SquidFleeGoal`: hurt by an entity within 10 blocks (the memory of the blow is 100 ticks), in the water: swim away from it. By EVENT (the blow).
pub static SQUID_FLEE: Module = Module { name: "squid_flee", tick: Some(flee_tick), flags: 0, can_use: flee_can_use, start: flee_start, stop: flee_stop, requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn flee_ok(w: &World, e: Entity) -> bool {
    let (Some(h), Some(p)) = (w.get::<LastHurtBy>(e), w.get::<Pos>(e)) else { return false };
    let now = now_of_ref(w, e);
    if now.saturating_sub(h.at) > 100 || !w.get::<FluidState>(e).is_some_and(|f| f.in_water) {
        return false;
    }
    crate::targeting::info(w, h.who).is_some_and(|i| retreat_on_hurt::dist_sq(p.0, i.pos) < 100.0)
}
fn now_of_ref(w: &World, e: Entity) -> u64 {
    let c = w.resource::<Clock>();
    c.of(e)
}
fn flee_can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    flee_ok(w, e)
}
fn flee_start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Fleeing);
}
fn flee_stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Fleeing>();
}
fn flee_tick(w: &mut World) {
    // A blow starts the flee.
    let hurt: Vec<Entity> = w.resource::<Events>().hurt_by_entity.iter().collect();
    for e in hurt {
        if let Some(s) = w.get::<GoalSet>(e).and_then(|g| g.slot_of(&SQUID_FLEE)) {
            w.resource_mut::<Requests>().0.push(Req::Start(e, s));
        }
    }
    let es: Vec<Entity> = w.query_filtered::<Entity, With<Fleeing>>().iter(w).collect();
    for e in es {
        if !flee_ok(w, e) {
            end(w, e, &SQUID_FLEE);
            continue;
        }
        let (Some(h), Some(p)) = (w.get::<LastHurtBy>(e).copied(), w.get::<Pos>(e).map(|p| p.0)) else { continue };
        let Some(by) = crate::targeting::info(w, h.who) else { continue };
        let f = [p.x - by.pos.x, p.y - by.pos.y, p.z - by.pos.z];
        let at = [(p.x + f[0]).floor() as i32, (p.y + f[1]).floor() as i32, (p.z + f[2]).floor() as i32];
        let mw = w.resource::<Level>().mw.0;
        let air = mw.facts.is_air(mw.world.block(at[0], at[1], at[2]));
        if mw.fluid(at[0], at[1], at[2]) == horizon_client_lib::blocks::Fluid::Water || air {
            if let Some(mut st) = w.get_mut::<SquidState>(e) {
                st.vector = rules::flee_vector(f, air);
            }
        }
    }
}
