//! `PandaLieOnBackGoal` (`mobs/panda.rs:879-902`): a lazy panda now and then lies on its back. Priority 8, no flag.
//! Start: after the cooldown, one ask in `start_asks` (every 2 ticks). The rest: each ask ends it with one chance in
//! `rest_asks` (`rest_goes_on`: for a lazy panda only that draw), and at once in water. Then the cooldown: 200 ticks.
use super::panda_state::{self as ps, Gene, FLAG_ON_BACK};
use super::*;
use crate::cow::{now_of, FluidState};
use crate::goals::{Req, Requests};

/// `reduced(400)` = 200 starting asks, `reduced(2000)` = 1000 resting asks, `cooldown = age + 200` after the rest.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub start_asks: i32,
    pub rest_asks: i32,
    pub cooldown_ticks: u64,
}
impl Params {
    pub const DEFAULT: Self = Self { start_asks: 200, rest_asks: 1000, cooldown_ticks: 200 };
}

/// `until`: the cooldown's end (the goal may start after it); `at`: the deadline of the next start ask (not running);
/// `asks_left`: the rest's asks still to come (running); `armed`: a wheel entry of this goal is pending.
#[derive(Component, Clone, Copy, Debug)]
pub struct LieOnBack {
    pub def: &'static Params,
    pub slot: u8,
    pub armed: bool,
    pub at: u64,
    pub until: u64,
    pub asks_left: i32,
}
/// The goal runs: its marker. Its end is the countdown of `asks_left`, asked every 2 ticks like the `Mob`'s continuation.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Lying;

pub static MODULE: Module = Module {
    name: "panda_lie_on_back",
    flags: 0,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    ps::ensure(w, e);
    let def = params::<Params>(p);
    w.entity_mut(e).insert(LieOnBack { def, slot: inst.slot, armed: false, at: 0, until: 0, asks_left: 0 });
}
fn arm(w: &mut World, e: Entity, _: Inst) {
    arm_start(w, e);
}
fn in_water(w: &World, e: Entity) -> bool {
    w.get::<FluidState>(e).is_some_and(|f| f.in_water)
}
/// The deadline of the start, or one ask of the rest.
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(l) = w.get::<LieOnBack>(e).copied() else { return false };
    if w.get::<Lying>(e).is_some() {
        // `rest_goes_on`: water ends it; else one draw of the countdown.
        let wet = in_water(w, e);
        let mut m = w.get_mut::<LieOnBack>(e).unwrap();
        m.armed = false;
        m.asks_left -= 1;
        let done = wet || m.asks_left <= 0;
        if done {
            drop(m);
            w.resource_mut::<Requests>().0.push(Req::End(e, l.slot));
        } else {
            m.armed = true;
            drop(m);
            crate::goals::arm(w, e, l.slot, 2);
        }
        return false;
    }
    if now_of(w, e) < l.at {
        ps::arm_to(w, e, l.slot, l.at);
        return false;
    }
    w.get_mut::<LieOnBack>(e).unwrap().armed = false;
    true
}
/// `PandaLieOnBackGoal.canUse`: lazy, after the cooldown, acting; the draw is the armed deadline.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let l = *w.get::<LieOnBack>(e).unwrap();
    if ps::gene(w, e) == Gene::Lazy && now_of(w, e) > l.until && ps::acts(w, e) {
        return true;
    }
    arm_start(w, e);
    false
}
/// `PandaLieOnBackGoal.start`: on its back; the rest's asks are counted from here (first ask 2 ticks on).
fn start(w: &mut World, e: Entity, inst: Inst) {
    ps::set_flag(w, e, FLAG_ON_BACK, true);
    w.entity_mut(e).insert(Lying);
    let k = ps::asks_until(w, e, w.get::<LieOnBack>(e).unwrap().def.rest_asks);
    {
        let mut m = w.get_mut::<LieOnBack>(e).unwrap();
        m.until = 0;
        m.asks_left = k;
        m.armed = true;
    }
    crate::goals::arm(w, e, inst.slot, 2);
}
/// `PandaLieOnBackGoal.stop`: off its back, the cooldown starts; then the next start is armed.
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Lying>();
    ps::set_flag(w, e, FLAG_ON_BACK, false);
    let now = now_of(w, e);
    {
        let mut m = w.get_mut::<LieOnBack>(e).unwrap();
        m.until = now + m.def.cooldown_ticks;
        m.armed = false;
    }
    arm_start(w, e);
}

/// Arms the next start ask of a lazy panda, after its cooldown: the first ask comes once `until` is past, then each
/// ask has one chance in `start_asks`.
fn arm_start(w: &mut World, e: Entity) {
    let Some(l) = w.get::<LieOnBack>(e).copied() else { return };
    if l.armed || w.get::<Lying>(e).is_some() || ps::gene(w, e) != Gene::Lazy {
        return;
    }
    let now = now_of(w, e);
    let k = ps::asks_until(w, e, l.def.start_asks);
    let at = (l.until + 1).max(now) + 2 * k as u64;
    ps::arm_to(w, e, l.slot, at);
    let mut m = w.get_mut::<LieOnBack>(e).unwrap();
    m.armed = true;
    m.at = at;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{species, spawn_species, Herd, Species};
    use crate::species::Config;

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 8, &Params::DEFAULT)];

    fn pandas(seed: u64, genes: &[Gene]) -> (Herd, Vec<Entity>) {
        let mut h = Herd::with_slab(8, true, false);
        let mut sp: Species = *species();
        sp.goals = Config { entries: &ENTRIES };
        let mut out = Vec::new();
        for (i, &g) in genes.iter().enumerate() {
            let e = spawn_species(&mut h.world, &sp, 0.5 + 2.0 * i as f64, 0.5, seed + i as u64);
            h.world.entity_mut(e).insert(g);
            out.push(e);
        }
        crate::goals::arm_all(&mut h.world);
        (h, out)
    }
    fn on_back(h: &Herd, e: Entity) -> bool {
        h.world.get::<ps::PandaState>(e).is_some_and(|s| s.flags & FLAG_ON_BACK != 0)
    }

    /// A lazy panda lies down (departure), stays down for several ticks (continuation), gets up and waits out its
    /// cooldown before lying down again (end and cooldown); a panda of another gene, beside it, never lies down.
    #[test]
    fn a_lazy_panda_lies_on_its_back_rests_then_waits_its_cooldown() {
    if crate::cow::skip_unless_walker("a_lazy_panda_lies_on_its_back_rests_then_waits_its_cooldown") {
        return;
    }
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            return;
        }
        let (mut h, es) = pandas(3, &[Gene::Lazy, Gene::Normal]);
        let (lazy, normal) = (es[0], es[1]);
        let mut start = None;
        for t in 1..=20000 {
            h.tick();
            if on_back(&h, lazy) {
                start = Some(t);
                break;
            }
        }
        let start = start.expect("the lazy panda never lay down in 20000 ticks");
        assert!(h.world.get::<crate::goals::GoalSet>(lazy).unwrap().is_running(&MODULE), "the goal is not running while on its back");
        for _ in 0..10 {
            h.tick();
        }
        assert!(on_back(&h, lazy), "it got up within 10 ticks of lying down: the goal did not continue");
        let mut end = None;
        for t in 1..=40000 {
            h.tick();
            if !on_back(&h, lazy) {
                end = Some(t);
                break;
            }
        }
        assert!(end.is_some(), "the lazy panda never got up in 40000 ticks");
        let now = crate::cow::now_of(&h.world, lazy);
        let until = h.world.get::<LieOnBack>(lazy).unwrap().until;
        assert!(until > now && until <= now + 201, "the cooldown is not the 200 ticks after the rest (until {until}, now {now})");
        assert!(!h.world.get::<crate::goals::GoalSet>(lazy).unwrap().is_running(&MODULE), "the goal still runs after its end");
        assert!(start > 0);
        for _ in 0..300 {
            h.tick();
            assert!(!on_back(&h, normal), "a panda that is not lazy lay on its back");
        }
    }
}
