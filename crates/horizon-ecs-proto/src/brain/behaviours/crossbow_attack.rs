//! `CrossbowAttack` (`PiglinAi` ranged fight, 1200 ticks): with a crossbow in the main hand, the attack target in sight and within 8
//! blocks, the entity looks at it and loads the crossbow (`charge` ticks), waits `delay + rand(jitter)` ticks charged, then shoots.
//! Its state (`CROSSBOW_STATE`: uncharged, charging, charged, ready) and the wait (`CROSSBOW_TIMER`, a deadline) are memories.
use crate::brain::*;
use crate::cow::{Pos, Rng};
use crate::goals::{d2, main_hand};
use crate::projectile::{launch, Flight};
use crate::rules::{ranged_attack, ranged_crossbow as rules};
use crate::targeting::{self, info, Who};
use horizon_core::geometry::Vec3;

const UNCHARGED: i64 = 0;
const CHARGING: i64 = 1;
const CHARGED: i64 = 2;
const READY: i64 = 3;
/// `CrossbowItem.getDefaultProjectileRange`, the reach of `isWithinAttackRange`.
const RANGE: f64 = 8.0;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:crossbow_attack",
    mems: &[&CROSSBOW_STATE, &CROSSBOW_TIMER, &ATTACK_TARGET, &LOOK_TARGET, &VISIBLE_MOBS],
    requires: &[Cap::Brain, Cap::Body, Cap::Targets],
    can_start,
    start,
    tick,
    stop,
    ..Behaviour::DEFAULT
};

/// `charge`: the ticks the crossbow is loaded for; `delay` and `jitter`: the charged wait is `delay + rand(jitter)`;
/// `difficulty`: the level's, for the spread of the shot.
pub struct Params {
    pub charge: i64,
    pub delay: i64,
    pub jitter: i64,
    pub difficulty: i32,
}

fn target(w: &World, e: Entity) -> Option<Who> {
    match get(w, e, &ATTACK_TARGET) {
        Some(MemValue::Who(who)) => Some(*who),
        _ => None,
    }
}

/// `checkExtraStartConditions`: a crossbow in hand, the target in sight and within reach.
fn ready_to_fire(w: &mut World, e: Entity, who: Who) -> bool {
    let holds = main_hand(w, e).is_some_and(|s| &*s.item == "minecraft:crossbow");
    let (Some(me), Some(i)) = (w.get::<Pos>(e).map(|x| x.0), info(w, who)) else { return false };
    holds && d2(me, i.pos) < RANGE * RANGE && visible_contains(w, e, who)
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    target(w, e).is_some_and(|who| ready_to_fire(w, e, who))
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    set(w, e, &CROSSBOW_STATE, MemValue::Int(UNCHARGED), None);
}

/// `canStillUse` (the target is set and still fires), then the look and the state step.
fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let Some(who) = target(w, e) else { return false };
    if !ready_to_fire(w, e, who) {
        return false;
    }
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(who, true)), None);
    step(w, e, who, c.get::<Params>());
    true
}

/// `crossbowAttack`: the state machine of the load and the shot.
fn step(w: &mut World, e: Entity, who: Who, p: &Params) {
    match int(w, e, &CROSSBOW_STATE).unwrap_or(UNCHARGED) {
        UNCHARGED => {
            if crate::using::start(w, e, false) {
                crate::using::set_remaining(w, e, p.charge as i32);
                set(w, e, &CROSSBOW_STATE, MemValue::Int(CHARGING), None);
            }
        }
        CHARGING => {
            // The use ends at its deadline (`charge` ticks): the crossbow is loaded then.
            if crate::using::state(w, e).is_none() {
                let Some(wait) = w.get_mut::<Rng>(e).map(|mut rng| p.delay + (rng.next_f64() * p.jitter as f64) as i64) else { return };
                set(w, e, &CROSSBOW_STATE, MemValue::Int(CHARGED), None);
                set(w, e, &CROSSBOW_TIMER, MemValue::Bool(true), Some(wait as u64));
            }
        }
        CHARGED => {
            if !present(w, e, &CROSSBOW_TIMER) {
                set(w, e, &CROSSBOW_STATE, MemValue::Int(READY), None);
            }
        }
        _ => {
            shoot(w, e, who, p.difficulty);
            set(w, e, &CROSSBOW_STATE, MemValue::Int(UNCHARGED), None);
        }
    }
}

/// `performRangedAttack(target, 1.0)`: an arrow from the eye, aimed with the spread of the difficulty.
fn shoot(w: &mut World, e: Entity, who: Who, difficulty: i32) {
    let Some(i) = targeting::info(w, who) else { return };
    let (Some(pos), Some(eye)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<targeting::Eye>(e).map(|x| x.0)) else { return };
    let start = Vec3::new(pos.x, pos.y + eye - f64::from(0.1_f32), pos.z);
    let height = i.bounds.max_position().y - i.bounds.min_position().y;
    let dir = rules::aim_dir(i.pos.x - pos.x, i.pos.z - pos.z, i.pos.y, height, start.y);
    let spread = ranged_attack::spread_of(f64::from((14 - difficulty * 4) as f32));
    let Some(mut rng) = w.get_mut::<Rng>(e) else { return };
    let v = ranged_attack::shot_velocity(dir, spread, 1.6, || rng.next_f64());
    launch(w, Flight::ARROW, start, v, Some(Who::Entity(e)));
}

/// `stopUsingItem`; the state is kept, as the behaviour's field is in vanilla.
fn stop(w: &mut World, e: Entity, _: &Ctx) {
    crate::using::stop(w, e);
}
