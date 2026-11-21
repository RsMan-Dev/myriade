//! `Fox.SleepGoal` (fiche 25): a fox naps in the shade by day when nothing is around. The `Mob` counts its countdown in asks
//! (one ask every 2 ticks); here the countdown is a deadline of the wheel. A refused ask by day asks again in 2 ticks (the
//! shelter and the watchers change without an event); a change of day wakes the foxes waiting; a sleeping fox checks its
//! conditions on its selector ticks.
use super::fox_state::{self as fs, CROUCHING, INTERESTED, SITTING, SLEEPING};
use super::*;
use crate::{
    cow::*,
    goals::*,
    hostile::{Daylight, PlayerState},
    spots::{walk_target_value, WalkValue},
};
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::{Axis, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The bound of the countdown, in asks: `reduced(140)`.
    pub countdown_asks: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { countdown_asks: 70 };
}
/// The goal's data: the bound of the countdown, the rank of the goal, and the first countdown drawn at creation (in asks).
#[derive(Component, Clone, Copy, Debug)]
pub struct SleepDef {
    pub asks: i32,
    pub slot: u8,
    pub first: i32,
}
/// The local time from which `canUse` decides: the countdown is over.
#[derive(Component, Clone, Copy, Debug)]
pub struct SleepDue(pub u64);
/// The goal runs: the fox sleeps.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Sleeping;

pub static MODULE: Module = Module {
    name: "fox_sleep",
    flags: MOVE | LOOK | JUMP,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    fs::ensure(w, e);
    let asks = params::<Params>(p).countdown_asks;
    let first = with_rng(w, e, |r| r.next_int(asks));
    w.entity_mut(e).insert(SleepDef { asks, slot: inst.slot, first });
}
fn arm(w: &mut World, e: Entity, _: Inst) {
    let d = *w.get::<SleepDef>(e).unwrap();
    countdown(w, e, d.slot, d.first);
}
/// The countdown of `asks` asks: the first ask that looks at the conditions comes at the `asks + 1`-th, two ticks each.
fn countdown(w: &mut World, e: Entity, slot: u8, asks: i32) {
    let wait = 2 * (asks + 1);
    let due = now_of(w, e) + wait as u64;
    w.entity_mut(e).insert(SleepDue(due));
    crate::goals::arm(w, e, slot, wait);
}
/// A deadline of the countdown or of a retry: while the goal runs there is nothing to ask.
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Sleeping>(e).is_none()
}
fn still(w: &World, e: Entity) -> bool {
    w.get::<Drive>(e).is_some_and(|d| d.zza == 0.0 && d.xxa == 0.0)
}
fn retry(w: &mut World, e: Entity, inst: Inst) {
    fs::retry_in(w, e, inst.slot, 2);
}
/// `SleepGoal.canUse`: standing still, the shelter by day, nobody around, not on powder snow (`can_sleep`), or asleep.
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    fs::retry_done(w, e);
    let now = now_of(w, e);
    if w.get::<SleepDue>(e).is_some_and(|d| now < d.0) {
        return false;
    }
    let day = w.resource::<Daylight>().0;
    if !still(w, e) {
        if day {
            retry(w, e, inst);
        }
        return false;
    }
    if fs::has(w, e, SLEEPING) || can_sleep(w, e) {
        return true;
    }
    if day {
        retry(w, e, inst);
    }
    false
}
/// `SleepGoal.canSleep` without the countdown (it is the wheel's): day, a shelter, no creature or player near, not on
/// powder snow.
fn can_sleep(w: &World, e: Entity) -> bool {
    let Some(p) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let mw = w.resource::<Level>().mw.0;
    let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    w.resource::<Daylight>().0 && has_shelter(w, e, mw) && !alertable(w, e, p, mw) && mw.block_name(x, y, z) != "minecraft:powder_snow"
}
/// `Fox.hasShelter`: the sky does not see the top of the box, and the walk value of the spot is not below zero.
fn has_shelter(w: &World, e: Entity, mw: &MobWorld) -> bool {
    let p = w.get::<Pos>(e).unwrap().0;
    let top = w.get::<Bounds>(e).map_or(p.y, |b| b.0.max(Axis::Y)).floor() as i32;
    let at = [p.x.floor() as i32, top, p.z.floor() as i32];
    let wv = w.get::<WalkValue>(e).copied().unwrap_or(WalkValue::Zero);
    !mw.can_see_sky(at[0], at[1], at[2]) && walk_target_value(mw, wv, at) >= 0.0
}
/// `alertable` of the entity's own feet, as the lookout asks it (`Lookout.Params.alarm`).
pub fn alarmed(w: &World, e: Entity) -> bool {
    let Some(p) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    alertable(w, e, p, w.resource::<Level>().mw.0)
}
/// `Fox.alertable`: a creature that is not a fox within 12 blocks (|dy| <= 6), or a player alive, not creative, not
/// spectator, not sneaking, within `max(12 x visibility, 2)` blocks. A creature of the fox's own species is no alarm (the
/// `Mob` says `Species::Fox`); every other one counts as untamed (no tame state here).
pub(super) fn alertable(w: &World, e: Entity, p: Vec3, _: &MobWorld) -> bool {
    let mut near = Vec::new();
    w.resource::<Index>().for_radius(p.x, p.z, 12.0, |c| near.push(c));
    let creature = near.into_iter().any(|c| {
        c != e
            && w.get::<Pos>(c).is_some_and(|q| (q.0.y - p.y).abs() <= 6.0 && d2(p, q.0) <= 144.0)
            && w.get::<SpeciesId>(c) != w.get::<SpeciesId>(e)
    });
    if creature {
        return true;
    }
    let mut players = Vec::new();
    w.resource::<Online>().for_near(p.x, p.z, 12.0, |q| players.push(q));
    let ps = w.resource::<PlayerState>();
    players.into_iter().any(|q| {
        let range = (12.0 * ps.visibility(q.id)).max(2.0);
        ps.alive(q.id)
            && !ps.creative(q.id)
            && !ps.spectator(q.id)
            && !ps.sneaking(q.id)
            && (q.y - p.y).abs() <= 6.0
            && d2(p, Vec3::new(q.x, q.y, q.z)) <= range * range
    })
}
/// `start`: the flags of the walk go, the navigation stops, the fox sleeps.
fn start(w: &mut World, e: Entity, _: Inst) {
    fs::set(w, e, SITTING | CROUCHING | INTERESTED, false);
    fs::set(w, e, SLEEPING, true);
    stop_nav(w, e);
    occupy(w, e, OCC_SLEEPING);
    w.entity_mut(e).insert(Sleeping);
}
/// `stop`: a new countdown is drawn, the states are cleared.
fn stop(w: &mut World, e: Entity, inst: Inst) {
    w.entity_mut(e).remove::<Sleeping>();
    release(w, e, OCC_SLEEPING);
    fs::retry_done(w, e);
    fs::set(w, e, 0xFF, false);
    let asks = w.get::<SleepDef>(e).unwrap().asks;
    let next = with_rng(w, e, |r| r.next_int(asks));
    countdown(w, e, inst.slot, next);
}
/// The tick of the module, run only while a fox that sleeps lives (no registered system).
fn tick(w: &mut World) {
    crate::goals::tick_state(w, wake_on_day);
    sleep_run(w);
}
/// The day came or went: the foxes waiting for their countdown look at the conditions again.
fn wake_on_day(day: Res<Daylight>, q: Query<(Entity, &SleepDef), Without<Sleeping>>, mut reqs: ResMut<Requests>) {
    if day.is_changed() {
        for (e, d) in &q {
            reqs.0.push(Req::Start(e, d.slot));
        }
    }
}
/// A sleeping fox, on its selector ticks: it ends when it can no longer sleep (day gone, shelter gone, someone near).
fn sleep_run(w: &mut World) {
    let list = crate::goals::entities::<With<Sleeping>>(w);
    for e in list {
        let Some(slot) = w.get::<SleepDef>(e).map(|d| d.slot) else { continue };
        if fs::selector_tick(w, e) && !can_sleep(w, e) {
            end_goal(w, e, slot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Config;

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 7, &Params::DEFAULT)];

    fn herd() -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(30, false, false);
        set_lazy_look(&mut h.world, false);
        Some(h)
    }
    /// A cow body named as a fox, whose only goal is the sleep.
    fn fox(h: &mut Herd) -> Entity {
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.name = "minecraft:fox";
        sp.goals = Config { entries: &ENTRIES };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 11);
        h.world.flush();
        h.world.resource_mut::<Requests>().0.push(Req::Arm(e));
        h.world.flush();
        e
    }
    /// A stone roof over the fox, five blocks wide.
    fn roof(h: &mut Herd) {
        for x in -2..=3 {
            for z in -2..=3 {
                h.set_block(x, 4, z, "minecraft:stone");
            }
        }
        h.tick();
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// The fox is asleep by day under a roof once its countdown is over: it does not start before it, and it does start.
    #[test]
    fn a_fox_under_a_roof_by_day_sleeps_once_its_countdown_is_over() {
        let Some(mut h) = herd() else { return };
        h.set_day(true);
        roof(&mut h);
        let e = fox(&mut h);
        h.tick();
        let due = h.world.get::<SleepDue>(e).expect("the countdown is armed").0;
        let mut started = None;
        for _ in 0..400 {
            h.tick();
            if running(&h, e) {
                started = Some(now_of(&h.world, e));
                break;
            }
        }
        let t = started.expect("the countdown is over: the fox sleeps");
        assert!(t >= due, "not before its countdown: started at {t}, due {due}");
        assert!(fs::has(&h.world, e, SLEEPING), "the flag is up while it sleeps");
    }
    /// At night nothing starts; the change of day wakes the waiting fox (the control: it starts after the change).
    #[test]
    fn at_night_no_fox_sleeps_and_the_dawn_wakes_it() {
        let Some(mut h) = herd() else { return };
        h.set_day(false);
        roof(&mut h);
        let e = fox(&mut h);
        for _ in 0..400 {
            h.tick();
        }
        assert!(!running(&h, e), "at night: no nap");
        h.set_day(true);
        let mut started = false;
        for _ in 0..10 {
            h.tick();
            started |= running(&h, e);
        }
        assert!(started, "the dawn wakes the fox: it sleeps");
    }
    /// A sleeping fox ends its nap on its next selector tick when a player comes within reach (the control: it sleeps
    /// before the player comes).
    #[test]
    fn a_player_coming_near_ends_the_nap() {
        let Some(mut h) = herd() else { return };
        h.set_day(true);
        roof(&mut h);
        let e = fox(&mut h);
        let mut started = false;
        for _ in 0..400 {
            h.tick();
            if running(&h, e) {
                started = true;
                break;
            }
        }
        assert!(started, "the fox sleeps before the player comes");
        h.set_online(&[(0.5, 3.0)]);
        for _ in 0..4 {
            h.tick();
        }
        assert!(!running(&h, e), "a player 2.5 blocks away: the nap is over");
        assert!(!fs::has(&h.world, e, SLEEPING), "the flag is cleared");
    }
}
