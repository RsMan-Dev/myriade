//! `PandaSitGoal` (`mobs/panda.rs:820-876`): a panda with an item in its main hand sits down, and gets up after a rest; the
//! item is then dropped and the next sit waits a cooldown. Flag MOVE, priority 7 in the species. Only the held item starts
//! it: the food on the ground (`food_near`) is absent from the world (DOUTE), so nothing walks to food.
use super::panda_state::{self as ps, Gene, FLAG_SIT};
use super::*;
use crate::cow::{now_of, FluidState};
use crate::goals::{equipped, stop_nav, Equipment, Req, Requests, Slot, MOVE};

/// `reduced(600)` = 300 (a non-lazy panda stops with one chance in 300 per ask), `reduced(2000)` = 1000 (one in 1000),
/// `wait` (`nextInt(150) + 10`, `nextInt(50) + 10` for a lazy one) times 20 ticks of cooldown.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub stop_asks: i32,
    pub rest_asks: i32,
    pub wait: i32,
    pub wait_lazy: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { stop_asks: 300, rest_asks: 1000, wait: 150, wait_lazy: 50 };
}

/// `cooldown`: the entity time before which it does not sit again.
#[derive(Component, Clone, Copy, Debug)]
pub struct Sit {
    pub def: Params,
    pub slot: u8,
    pub cooldown: u64,
}
/// The goal runs (sitting): its marker. Its asks come every 2 ticks, the selector's phase.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Sitting;

pub static MODULE: Module = Module {
    name: "panda_sit",
    flags: MOVE,
    equip,
    fired,
    can_use,
    start,
    stop,
    requires: &[Cap::Goals],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    ps::ensure(w, e);
    let def = *params::<Params>(p);
    w.entity_mut(e).insert(Sit { def, slot: inst.slot, cooldown: 0 });
}
fn held(w: &World, e: Entity) -> bool {
    equipped(w, e, Slot::Main) != 0
}
fn in_water(w: &World, e: Entity) -> bool {
    w.get::<FluidState>(e).is_some_and(|f| f.in_water)
}
fn arm_to(w: &mut World, e: Entity, slot: u8, at: u64) {
    let now = now_of(w, e);
    crate::goals::arm(w, e, slot, at.saturating_sub(now).min(i32::MAX as u64) as i32);
}

/// A sitting panda asks its continuation every 2 ticks (`rest_goes_on`); otherwise the goal is asked (true).
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    if w.get::<Sitting>(e).is_none() {
        return true;
    }
    let s = *w.get::<Sit>(e).unwrap();
    if rest_goes_on(w, e, &s.def) {
        if held(w, e) && !in_water(w, e) {
            ps::set_flag(w, e, FLAG_SIT, true);
        }
        crate::goals::arm(w, e, inst.slot, 2);
    } else {
        w.resource_mut::<Requests>().0.push(Req::End(e, s.slot));
    }
    false
}
/// `PandaSitGoal.rest_goes_on`: not in water, and (lazy, or a draw other than 1 of `reduced(600)`), and a draw other than 1
/// of `reduced(2000)`. The draws are made in that order and only when reached, as the Mob makes them.
fn rest_goes_on(w: &mut World, e: Entity, def: &Params) -> bool {
    if in_water(w, e) {
        return false;
    }
    let lazy = ps::gene(w, e) == Gene::Lazy;
    let first = lazy || crate::goals::with_rng(w, e, |r| r.next_int(def.stop_asks)) != 1;
    first && crate::goals::with_rng(w, e, |r| r.next_int(def.rest_asks)) != 1
}
/// `PandaSitGoal.canUse`: after the cooldown, not a baby, not in water, acting, not unhappy (no unhappy state: DOUTE),
/// and an item in the main hand (food on the ground is absent). A refusal with an item in hand is asked again: the item
/// is a condition that changes without an event.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let s = *w.get::<Sit>(e).unwrap();
    let now = now_of(w, e);
    let blocked = s.cooldown > now || ps::is_baby(w, e) || in_water(w, e) || !ps::acts(w, e);
    if !blocked && held(w, e) {
        return true;
    }
    if held(w, e) {
        let at = if s.cooldown > now { s.cooldown } else { now + 2 };
        arm_to(w, e, s.slot, at);
    }
    false
}
/// `PandaSitGoal.start`: with an item, sit down (the cooldown is reset to 0, as the Mob does).
fn start(w: &mut World, e: Entity, inst: Inst) {
    if held(w, e) && !in_water(w, e) {
        stop_nav(w, e);
        ps::set_flag(w, e, FLAG_SIT, true);
    }
    w.get_mut::<Sit>(e).unwrap().cooldown = 0;
    w.entity_mut(e).insert(Sitting);
    crate::goals::arm(w, e, inst.slot, 2);
}
/// `PandaSitGoal.stop`: an item in hand is dropped (its ground entity is absent: DOUTE) and the hand is emptied, then the
/// cooldown runs; the sitting ends.
fn stop(w: &mut World, e: Entity, _: Inst) {
    if held(w, e) {
        if let Some(mut q) = w.get_mut::<Equipment>(e) {
            q.main = 0;
        }
        let s = *w.get::<Sit>(e).unwrap();
        let lazy = ps::gene(w, e) == Gene::Lazy;
        let n = if lazy { s.def.wait_lazy } else { s.def.wait };
        let wait = crate::goals::with_rng(w, e, |r| r.next_int(n)) + 10;
        w.get_mut::<Sit>(e).unwrap().cooldown = now_of(w, e) + wait as u64 * 20;
    }
    ps::set_flag(w, e, FLAG_SIT, false);
    w.entity_mut(e).remove::<Sitting>();
}

/// Wakes the goal when an item enters the main hand: the rule that picks items up calls it (not in the world yet: BESOIN).
pub fn item_in_hand(w: &mut World, e: Entity) {
    crate::goals::wake(w, e, &MODULE);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{species, spawn_species, Herd, Species};
    use crate::species::Config;

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 7, &Params::DEFAULT)];

    fn cow(seed: u64) -> (Herd, Entity) {
        let mut h = Herd::with_slab(8, true, false);
        let mut sp: Species = *species();
        sp.goals = Config { entries: &ENTRIES };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, seed);
        crate::goals::arm_all(&mut h.world);
        (h, e)
    }
    fn give(h: &mut Herd, e: Entity, item: u32) {
        h.world.entity_mut(e).insert(Equipment { main: item, ..Default::default() });
        item_in_hand(&mut h.world, e);
    }
    fn sitting(h: &Herd, e: Entity) -> bool {
        ps::flag(&h.world, e, FLAG_SIT)
    }

    /// Without an item nothing starts (witness: the same animal sits as soon as it holds one). With an item it sits, stays
    /// sitting for several asks, gets up, drops the item and waits its cooldown.
    #[test]
    fn a_panda_with_an_item_sits_rests_then_drops_it_and_waits() {
    if crate::cow::skip_unless_walker("a_panda_with_an_item_sits_rests_then_drops_it_and_waits") {
        return;
    }
        let (mut h, e) = cow(21);
        for _ in 0..40 {
            h.tick();
        }
        assert!(!sitting(&h, e), "sat down without an item");
        let bamboo = crate::goals::item_id("minecraft:wheat");
        give(&mut h, e, bamboo);
        let mut sat = false;
        for _ in 0..4 {
            h.tick();
            sat |= sitting(&h, e);
        }
        assert!(sat, "holding an item, it did not sit down within 4 ticks");
        let mut end = None;
        for t in 1..=40000 {
            h.tick();
            if !sitting(&h, e) {
                end = Some(t);
                break;
            }
        }
        let end = end.expect("it never got up in 40000 ticks");
        assert!(end > 4, "it got up at once: the continuation did not run ({end} ticks)");
        assert_eq!(equipped(&h.world, e, Slot::Main), 0, "the item was not dropped");
        let now = now_of(&h.world, e);
        let cd = h.world.get::<Sit>(e).unwrap().cooldown;
        assert!(cd > now && cd <= now + 160 * 20, "cooldown {cd} not in ({now}, {}]", now + 160 * 20);
        assert!(!h.world.get::<crate::goals::GoalSet>(e).unwrap().is_running(&MODULE), "still running after its end");
    }
}
