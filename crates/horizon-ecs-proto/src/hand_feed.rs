//! The rule `HAND_FEED`: a player holds food up to an animal that sits down and eats it (`Panda.mobInteract`). The breed module answers first for an
//! adult that can fall in love; this rule takes what it refused. A baby ages up by a tenth of what it has left; an adult that is not already sitting
//! or in water sits, starts eating (`eat(true)`), drops what its hand held and takes one of the food (nothing in a creative player's case:
//! the eating then stops at the first step, the hand being empty). The eating (`Panda.handleEating`) is a chain of deadlines of the wheel: a
//! sound every 5 counted ticks, then from the counter 81 on one chance in 20 per tick, drawn once, that it ends (an item of the ground tag
//! is eaten and the animal gets up). It stops early when the animal no longer sits or its hand is empty (read at each step, up to 4 ticks late).
//! Not done: the item particles of the client, `gotBamboo` (the target set), the scared and on-back answers before the food (the breed module
//! answers earlier for an adult), the idle start (one chance in 80 per tick, sitting with an item: a draw every tick).
use crate::behaviours::panda_state::{self as ps, FLAG_ON_BACK, FLAG_SIT};
use crate::cow::*;
use crate::goals::*;
use crate::rules::{no_attrs, params, Rule};
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;

#[derive(Clone, Copy, Debug)]
pub struct FeedParams {
    /// The tag of the foods, and of the foods the animal eats up from the ground (it gets up and the item goes).
    pub food: &'static str,
    pub eats_up: &'static str,
    pub sound: &'static str,
    /// The counter after which the end is drawn, the odds' denominator, and the counter after which an item is eaten up.
    pub after: i32,
    pub one_in: i32,
    pub eats_after: i32,
}
impl FeedParams {
    pub const PANDA: Self = Self { food: "#minecraft:panda_food", eats_up: "#minecraft:panda_eats_from_ground", sound: "minecraft:entity.panda.eat", after: 80, one_in: 20, eats_after: 100 };
}
/// An eating in progress: the counter at the last step, the counter it ends at, and the own tick of the next step (older callbacks find it later).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Eating {
    p: &'static FeedParams,
    counter: i32,
    end: i32,
    due: u64,
}
pub static HAND_FEED: Rule = Rule { name: "hand_feed", equip: |_, _, _| {}, attrs: no_attrs, interact };

fn interact(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    let p = params::<FeedParams>(p);
    if i.item == 0 || !crate::items::is(p.food, i.item) {
        return false;
    }
    if age_up(w, e, i) {
        return true;
    }
    if ps::flag(w, e, FLAG_SIT) || w.get::<FluidState>(e).is_some_and(|f| f.in_water) {
        return false;
    }
    // `tryToSit`, then `eat(true)`.
    stop_nav(w, e);
    ps::set_flag(w, e, FLAG_SIT, true);
    ps::set_flag(w, e, FLAG_ON_BACK, false);
    set_counter(w, e, 1);
    let creative = w.get_resource::<crate::used_stack::UsedStack>().is_some_and(|u| u.creative);
    if !creative {
        if equipped(w, e, Slot::Main) != 0 {
            let old = crate::equipment::take_slot(w, e, Slot::Main);
            let at = w.get::<Pos>(e).map_or([0.0; 3], |q| [q.0.x, q.0.y, q.0.z]);
            if let Some(name) = crate::items::item_name(old) {
                crate::behaviours::shear::drop_stacks(w, vec![ItemStack::new(name, 1)], at);
            }
        }
        crate::equipment::set_slot(w, e, Slot::Main, i.item);
    }
    crate::give_item::take_from_hand(w, i);
    if equipped(w, e, Slot::Main) != 0 {
        crate::behaviours::panda_sit::item_in_hand(w, e);
    }
    w.entity_mut(e).insert(Eating { p, counter: 1, end: 0, due: 0 });
    step_in(w, e, 4);
    true
}
/// The rule `FEED_BABY` (`Animal.mobInteract` for a species that cannot fall in love, such as the happy ghast): the food (`FeedParams.food`) ages a baby up, an adult
/// gets nothing. Only `food` of the parameters is read.
pub static FEED_BABY: Rule = Rule { name: "feed_baby", equip: |_, _, _| {}, attrs: no_attrs, interact: feed_baby };
fn feed_baby(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    i.item != 0 && crate::items::is(params::<FeedParams>(p).food, i.item) && age_up(w, e, i)
}
impl FeedParams {
    /// The happy ghast's food (`#minecraft:happy_ghast_food`), for `FEED_BABY`.
    pub const HAPPY_GHAST: Self = Self { food: "#minecraft:happy_ghast_food", ..Self::PANDA };
}
/// A baby is fed: the item is spent and its growth deadline moves up by a tenth of the seconds left (`ageUp(trunc(-age / 20 * 0.1), true)`).
/// False when `e` is not a baby (nothing is spent).
pub fn age_up(w: &mut World, e: Entity, i: &crate::behaviours::Interaction) -> bool {
    let Some(g) = w.get::<GrowAt>(e).copied() else { return false };
    // `AgeableMob.canAgeUp`: a locked baby takes no food.
    if w.get::<crate::age_lock::AgeLocked>(e).is_some() {
        return false;
    }
    let now = now_of(w, e);
    let seconds = ((g.0.saturating_sub(now) as i32 / 20) as f32 * 0.1) as u64;
    crate::give_item::take_from_hand(w, i);
    let at = g.0.saturating_sub(seconds * 20).max(now + 1);
    // `AgeableMob.ageUp(.., true)`: the age gained is added to `forcedAge`, saved as `ForcedAge`.
    let gained = i32::try_from(g.0 - at).unwrap_or(i32::MAX);
    let kept = w.get::<crate::age_lock::ForcedAge>(e).map_or(0, |f| f.0);
    w.entity_mut(e).insert(crate::age_lock::ForcedAge(kept.saturating_add(gained)));
    w.get_mut::<GrowAt>(e).unwrap().0 = at;
    arm_at(w, e, at, Due::Grow);
    true
}
fn set_counter(w: &mut World, e: Entity, n: i32) {
    ps::ensure(w, e);
    w.get_mut::<ps::PandaState>(e).unwrap().eat = n;
    crate::bridge::view::mark(w, e);
}
fn step_in(w: &mut World, e: Entity, ticks: u64) {
    let due = now_of(w, e) + ticks;
    if let Some(mut s) = w.get_mut::<Eating>(e) {
        s.due = due;
    }
    after(w, e, ticks as i32, step);
}
/// `Panda.handleEating`, counted: the counter is at its next multiple of 5 (or at the end drawn once past `after`).
fn step(w: &mut World, e: Entity) {
    let Some(mut s) = w.get::<Eating>(e).copied() else { return };
    if now_of(w, e) < s.due || s.due == 0 {
        return;
    }
    if equipped(w, e, Slot::Main) == 0 || !ps::flag(w, e, FLAG_SIT) {
        w.entity_mut(e).remove::<Eating>();
        set_counter(w, e, 0);
        return;
    }
    let from = s.counter;
    s.counter = if s.end > 0 && s.end < (from / 5 + 1) * 5 { s.end } else { (from / 5 + 1) * 5 };
    if s.counter % 5 == 0 {
        let (v, a, b) = with_rng(w, e, |r| (r.next_int(2), r.next_f64() as f32, r.next_f64() as f32));
        let at = w.get::<Pos>(e).map_or([0.0; 3], |q| [q.0.x, q.0.y, q.0.z]);
        w.resource_mut::<Events>().host.sound(s.p.sound, at, 0.5 + 0.5 * v as f32, (a - b) * 0.2 + 1.0);
    }
    if s.counter == s.p.after && s.end == 0 {
        // From the counter after + 1 on, each tick ends it with the odds 1 / one_in: the first success, drawn once.
        let u = with_rng(w, e, |r| r.next_f64());
        s.end = s.p.after + 1 + crate::coating::wait_for(u, 1.0 / f64::from(s.p.one_in)) as i32;
    }
    set_counter(w, e, s.counter);
    if s.end > 0 && s.counter >= s.end {
        finish(w, e, s);
        return;
    }
    w.entity_mut(e).insert(s);
    let next = if s.end > 0 && s.end < (s.counter / 5 + 1) * 5 { s.end - s.counter } else { 5 - s.counter % 5 };
    step_in(w, e, next.max(1) as u64);
}
/// The end drawn: past `eats_after` an item of the ground tag is eaten, the animal gets up and the event is posted; `eat(false)` in any case.
fn finish(w: &mut World, e: Entity, s: Eating) {
    let held = equipped(w, e, Slot::Main);
    if s.counter > s.p.eats_after && held != 0 && crate::items::is(s.p.eats_up, held) {
        crate::equipment::take_slot(w, e, Slot::Main);
        let at = w.get::<Pos>(e).map_or([0.0; 3], |q| [q.0.x, q.0.y, q.0.z]);
        w.resource_mut::<Events>().host.game_event_by(e, "minecraft:eat", at, false, false, false);
        ps::set_flag(w, e, FLAG_SIT, false);
    }
    w.entity_mut(e).remove::<Eating>();
    set_counter(w, e, 0);
}
