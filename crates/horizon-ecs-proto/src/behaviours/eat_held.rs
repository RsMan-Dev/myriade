//! Eating the item held in the mouth after a delay (`Fox.aiStep`'s eating block, fiche 40): a RULE without a flag and without a goal.
//! The `Mob` counts `ticksSinceEaten` up every tick; here the count is the age of `Eaten` and two deadlines replace it:
//! the crumbs (a 1-in-`crumb_chance` draw per tick between `crumbs_from` and `delay`, sampled once as a geometric wait) and the
//! meal (`delay` ticks after the last one). A meal is asked of the host (`HostEvent::HeldConsume`: the item's data, its effects and
//! its remainder are the host's); the host answers with `EcsWorld::finish_held_use`. Nothing runs while the mouth is empty.
use super::elder_curse::Silent;
use super::fox_state::{self as fs, SLEEPING};
use super::*;
use crate::cow::*;
use crate::dragon::flight::NoAi;
use crate::goals::*;
use crate::targeting::Target;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The meal is asked once the last one is more than this many ticks old.
    pub delay: u64,
    /// The crumbs start once the last meal is more than this many ticks old.
    pub crumbs_from: u64,
    /// A crumb comes with the chance of 1 in this, each tick.
    pub crumb_chance: i32,
    /// A refused meal (target, air, nap) is asked again after this many ticks.
    pub retry: i32,
    /// The items eaten (`Fox.isConsumableFood`), by name; the species gives them (the game's food data is not here).
    pub foods: &'static [&'static str],
}
impl Params {
    pub const DEFAULT: Self = Self { delay: 600, crumbs_from: 560, crumb_chance: 10, retry: 20, foods: &[] };
}
/// The rule's data.
#[derive(Component, Clone, Copy, Debug)]
pub struct Eats(pub Params);
/// `Fox.ticksSinceEaten` as the own time it was set (a pickup, a meal): its value is `now - at`, so it is `> 0` from the next tick.
#[derive(Component, Clone, Copy, Debug)]
pub struct Eaten(pub u64);
/// The own time of the one deadline that counts; a callback that finds another time is a stale one.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct EatDue(pub u64);

pub static MODULE: Module = Module {
    name: "eat_held",
    flags: 0,
    equip,
    fired: never,
    can_use: never,
    tick: None,
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    let now = now_of(w, e);
    w.entity_mut(e).insert((Eats(p), Eaten(now)));
    restart(w, e);
}
/// A rule, not a goal: nothing starts it.
fn never(_: &mut World, _: Entity, _: Inst) -> bool {
    false
}

/// The clock restarts now (`ticksSinceEaten = 0`) and the deadlines follow it; nothing is armed while the mouth is empty.
pub fn restart(w: &mut World, e: Entity) {
    let Some(p) = w.get::<Eats>(e).map(|d| d.0) else { return };
    let now = now_of(w, e);
    if let Some(mut t) = w.get_mut::<Eaten>(e) {
        t.0 = now;
    }
    if main_hand(w, e).is_none() {
        w.entity_mut(e).remove::<EatDue>();
        return;
    }
    plan(w, e, p, now + p.crumbs_from + 1);
}

/// Arms the next deadline: the first crumb at or after `from` (own time), or the meal if the crumbs run out first.
fn plan(w: &mut World, e: Entity, p: Params, from: u64) {
    let last_crumb = w.get::<Eaten>(e).map_or(0, |t| t.0) + p.delay;
    let wait = with_rng(w, e, |r| geometric(r, p.crumb_chance)).max(0) as u64;
    let at = from + wait;
    arm(w, e, if at <= last_crumb { at } else { last_crumb + 1 });
}

fn arm(w: &mut World, e: Entity, at: u64) {
    let now = now_of(w, e);
    let at = at.max(now + 1);
    w.entity_mut(e).insert(EatDue(at));
    after(w, e, (at - now) as i32, step);
}

/// `Fox.aiStep` conditions of the block: no target, on the ground, not asleep, and the mind runs.
fn may_eat(w: &World, e: Entity) -> bool {
    w.get::<Target>(e).is_none() && crate::level::is_on_ground(w, e) && !fs::has(w, e, SLEEPING) && w.get::<NoAi>(e).is_none()
}

fn step(w: &mut World, e: Entity) {
    let now = now_of(w, e);
    if w.get::<EatDue>(e).map(|d| d.0) != Some(now) {
        return;
    }
    w.entity_mut(e).remove::<EatDue>();
    let Some(p) = w.get::<Eats>(e).map(|d| d.0) else { return };
    if crate::tear_down::is_dead(w, e) {
        return;
    }
    let Some(stack) = main_hand(w, e).cloned() else { return };
    if !p.foods.contains(&&*stack.item) {
        return;
    }
    let eaten = now.saturating_sub(w.get::<Eaten>(e).map_or(0, |t| t.0));
    if eaten > p.delay {
        if may_eat(w, e) {
            ask(w, e, stack);
        }
        // Answered, the host restarts the clock; unanswered or refused, the meal is asked again.
        arm(w, e, now + p.retry.max(1) as u64);
        return;
    }
    if may_eat(w, e) {
        crumb(w, e);
    }
    plan(w, e, p, now + 1);
}

/// `Fox.aiStep`: the eating sound and the entity event 45 that makes the clients throw the crumbs of the food.
fn crumb(w: &mut World, e: Entity) {
    let at = w.get::<Pos>(e).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
    let mut ev = w.resource_mut::<Events>();
    ev.host.sound("entity.fox.eat", at, 1.0, 1.0);
    ev.host.entity_event(e, 45);
}

/// The meal: the host runs the consume effects with a random seeded from the creature's own (the host's `LegacyRandom` is not the
/// ECS generator), then answers with `finish_held_use`.
fn ask(w: &mut World, e: Entity, stack: myriade_engine_lib::items::ItemStack) {
    let (hi, lo) = with_rng(w, e, |r| (r.next_u32(), r.next_u32()));
    let seed = (i64::from(hi) << 32) | i64::from(lo);
    let silent = w.get::<Silent>(e).is_some();
    w.resource_mut::<Events>().host.held_consume(e, stack, seed, silent);
}

/// The host's answer: the meal is done. One of the stack is eaten, or the use remainder replaces it; the clock restarts.
/// False (nothing changed) when the mouth no longer holds `stack` or the creature is gone.
pub fn finish(w: &mut World, e: Entity, stack: &myriade_engine_lib::items::ItemStack, remainder: Option<myriade_engine_lib::items::ItemStack>) -> bool {
    if !still_holds(w, e, stack) {
        return false;
    }
    if stack.count > 1 {
        let mut rest = stack.clone();
        rest.count -= 1;
        set_main_hand(w, e, Some(rest));
    } else {
        set_main_hand(w, e, remainder);
    }
    restart(w, e);
    true
}

/// The creature lives and its mouth still holds the stack of the meal.
pub fn still_holds(w: &World, e: Entity, stack: &myriade_engine_lib::items::ItemStack) -> bool {
    w.get::<Pos>(e).is_some() && !crate::tear_down::is_dead(w, e) && main_hand(w, e) == Some(stack)
}
