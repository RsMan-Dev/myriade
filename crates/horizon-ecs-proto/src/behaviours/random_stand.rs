//! `RandomStandGoal` (the equids that rear): a counter climbs one per ask; once it is above 0, each ask draws
//! `nextInt(1000) < counter`, and a hit makes a second draw, one in ten, to rear for 20 ticks. A cycle that fails
//! (the rearing is not allowed now, or the second draw misses) resets the counter and starts again.
//! The whole wait is drawn once, at arming, with the same law (the Mob draws one value per ask, the asks being every
//! other tick).
use super::*;
use crate::rules::random_stand::{asks_until_draw, stand_at};
use crate::{cow::*, goals::*};

/// `AbstractHorse`: `resetStandInterval` puts the counter at `-getAmbientStandInterval` (400 for every horse).
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub interval: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { interval: 400 };
}
/// The second draw of a cycle: `nextInt(10) == 0`.
const ONE_IN: i32 = 10;
/// `stand_if_possible`: `counter = 20`; `AbstractHorse.tick` lowers it in the tick it is set (after the goals), so a
/// sample taken after each tick sees 19 ticks up (measured against a `Mob` horse: `pilot_vs_mob.rs`, stand_4).
pub const STAND_TICKS: u64 = 19;

/// The counter of the cycle (`next`, `-interval` between two cycles); `armed`: its deadline is in the wheel.
#[derive(Component, Clone, Copy, Debug)]
pub struct Stand {
    pub next: i32,
    pub interval: i32,
    pub armed: bool,
}
/// The animal rears until `until` (the stand counter); it cannot take a goal meanwhile.
#[derive(Component, Clone, Copy, Debug)]
#[component(on_insert = crate::bridge::view::touched, on_remove = crate::bridge::view::touched)]
pub struct Standing {
    pub until: u64,
}
/// `AbstractHorse.animate`'s stand value: `now` of this tick, `last` of the one before (`standAnimO`). Sparse: only an animal
/// that rears, or whose value is falling back, carries it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct StandAnim {
    pub now: f32,
    pub last: f32,
}

pub static MODULE: Module = Module {
    name: "random_stand",
    flags: 0,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    damaged: hurt,
    tick: Some(animate),
    requires: &[Cap::Goals],
    ..Module::DEFAULT
};
/// `AbstractHorse.hurtServer`: one blow in three (`nextInt(3) == 0`) makes the mount rear (`standIfPossible`).
fn hurt(w: &mut World, e: Entity, _: Inst, _: &Damaged) {
    if with_rng(w, e, |r| r.next_int(3)) == 0 {
        crate::make_mad::stand_if_possible(w, e);
    }
}
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let interval = params::<Params>(p).interval;
    w.entity_mut(e).insert(Stand { next: -interval, interval, armed: false });
}
/// The next deadline of the cycle: its draw comes up at the `asks`-th ask, the asks being on the selector's ticks
/// (tick + phase even), the first at the next one.
fn arm(w: &mut World, e: Entity, inst: Inst) {
    let Some(s) = w.get::<Stand>(e).copied() else { return };
    if s.armed {
        return;
    }
    let asks = with_rng(w, e, |r| asks_until_draw(s.next, |k| r.next_int(k)));
    let now = crate::cow::now_of(w, e);
    let phase = crate::goals::phase_of(w, e);
    let first = if (now + 1 + phase) % 2 == 0 { now + 1 } else { now + 2 };
    let at = stand_at(first, asks);
    crate::goals::arm(w, e, inst.slot, (at - now) as i32);
    let mut st = w.get_mut::<Stand>(e).unwrap();
    st.armed = true;
    st.next = -st.interval;
}
/// A deadline came up: the ask happens only if the animal is active (a sleeping animal lets the cycle lapse; its
/// arming resumes when it enters the active zone).
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = crate::cow::now_of(w, e);
    let active = w.get::<ActiveUntil>(e).is_some_and(|a| a.0 > now);
    if let Some(mut s) = w.get_mut::<Stand>(e) {
        s.armed = false;
    }
    active
}
/// `RandomStandGoal.canUse` after the draw: no rear while the animal stands (`immobile`), else the one-in-ten draw.
/// A refusal starts the next cycle.
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let now = crate::cow::now_of(w, e);
    let standing = w.get::<Standing>(e).is_some_and(|s| now < s.until);
    let ok = !standing && with_rng(w, e, |r| r.next_int(ONE_IN) == 0);
    if !ok {
        arm(w, e, inst);
    }
    ok
}
/// `RandomStandGoal.start` (`stand_if_possible`): the rear for 20 ticks. The goal has no second tick (`can_continue`
/// is false), so it ends at once.
fn start(w: &mut World, e: Entity, inst: Inst) {
    let now = crate::cow::now_of(w, e);
    w.entity_mut(e).insert(Standing { until: now + STAND_TICKS });
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    arm(w, e, inst);
}
/// `AbstractHorse.animate` (stand part): rises while it rears (`+ (1 - v) * 0.4 + 0.05`, at most 1), else falls (`0.8 v³ - v`).
/// The value is kept while the animal rears or is still above 0; the component goes when it reaches 0 at rest.
fn animate(w: &mut World) {
    let rows: Vec<Entity> = crate::queries::entities::<Or<(With<Standing>, With<StandAnim>)>>(w);
    for e in rows {
        let now = crate::cow::now_of(w, e);
        let standing = w.get::<Standing>(e).is_some_and(|s| now < s.until);
        let mut a = w.get::<StandAnim>(e).copied().unwrap_or(StandAnim { now: 0.0, last: 0.0 });
        a.last = a.now;
        a.now = if standing {
            (a.now + (1.0 - a.now) * 0.4 + 0.05).min(1.0)
        } else {
            let s = a.now;
            (s + (0.8 * s * s * s - s) * 0.6 - 0.05).max(0.0)
        };
        if standing || a.now > 0.0 {
            w.entity_mut(e).insert(a);
        } else {
            w.entity_mut(e).remove::<StandAnim>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::Any;

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 9, &Params::DEFAULT)];

    /// A resting animal of the process's species, whose selector holds only this goal, active without end.
    fn resting() -> (Herd, Entity) {
        let mut herd = Herd::with_slab_awake(20, false, false);
        let e = spawn_species(&mut herd.world, species(), 0.0, 0.0, 7);
        herd.world.entity_mut(e).remove::<GoalSet>().insert((GoalSet::new(&ENTRIES), ActiveUntil(u64::MAX)));
        herd.world.flush();
        arm_timers(&mut herd.world, e);
        (herd, e)
    }
    /// From the counter -400, no draw comes before the 401st ask: the counter has to climb past 0 first.
    #[test]
    fn the_draw_waits_for_the_counter_to_pass_zero() {
        let mut r = Rng(12345);
        let mut total = 0i64;
        for _ in 0..2000 {
            let asks = asks_until_draw(-400, |k| r.next_int(k));
            assert!(asks >= 401, "a draw before the counter passed 0: {asks}");
            total += i64::from(asks);
        }
        let mean = total as f64 / 2000.0;
        assert!((401.0..560.0).contains(&mean), "mean asks {mean}");
    }
    /// A resting animal rears from time to time, for 20 ticks, and never starts a second rear inside the first.
    #[test]
    fn a_resting_animal_rears_now_and_then_for_twenty_ticks() {
    if crate::cow::skip_unless_walker("a_resting_animal_rears_now_and_then_for_twenty_ticks") {
        return;
    }
        let (mut herd, e) = resting();
        let mut last = u64::MAX;
        let mut rears = 0;
        let mut ending = false;
        for _ in 0..60_000 {
            herd.tick();
            if ending {
                assert!(!herd.world.get::<GoalSet>(e).unwrap().is_running(&MODULE), "the goal ends the tick after");
                ending = false;
            }
            if let Some(s) = herd.world.get::<Standing>(e).copied()
                && s.until != last
            {
                let now = herd.world.resource::<Clock>().0;
                assert_eq!(s.until, now + STAND_TICKS, "the rear is 20 ticks long");
                last = s.until;
                rears += 1;
                ending = true;
            }
        }
        assert!(rears >= 1, "no rear in 60 000 ticks");
    }
    /// A deadline that comes up while the animal rears does not start the goal again. The control: with the same
    /// deadlines and no rear going on, the goal does start (else the test would pass for a goal that never starts).
    #[test]
    fn a_standing_animal_does_not_start_the_goal() {
    if crate::cow::skip_unless_walker("a_standing_animal_does_not_start_the_goal") {
        return;
    }
        let (mut herd, e) = resting();
        let mut started = false;
        for _ in 0..400 {
            fire(&mut herd.world, e, &MODULE);
            herd.tick();
            if herd.world.get::<Standing>(e).is_some() {
                started = true;
                break;
            }
        }
        assert!(started, "a forced deadline never starts the goal when the animal is free");
        let now = herd.world.resource::<Clock>().0;
        herd.world.entity_mut(e).insert(Standing { until: now + 1000 });
        for _ in 0..400 {
            fire(&mut herd.world, e, &MODULE);
            herd.tick();
            assert_eq!(herd.world.get::<Standing>(e).unwrap().until, now + 1000, "a second rear started");
        }
    }
    /// The parameters of the module reach its state (the counter starts at -interval).
    #[test]
    fn the_counter_starts_at_minus_the_interval() {
        let mut w = World::new();
        let e = w.spawn(()).id();
        let p: &'static (dyn Any + Sync) = &Params { interval: 400 };
        equip(&mut w, e, Inst { slot: 0, nth: 0 }, p);
        let s = *w.get::<Stand>(e).unwrap();
        assert_eq!((s.next, s.interval, s.armed), (-400, 400, false));
    }
}
