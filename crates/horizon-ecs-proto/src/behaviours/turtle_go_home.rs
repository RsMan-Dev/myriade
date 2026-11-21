//! `TurtleGoHomeGoal` (fiche 51): a turtle that is not a baby and carries an egg goes home at once; any other turtle 64 blocks or
//! more from home does so by chance: the Mob draws `nextInt(reduced(700)) == 0` at each request (every 2 ticks), so the draw is a
//! deadline of `2 * (geometric(350) + 1)` ticks. A drawn chance that finds the turtle near home (within 64) does not start the goal.
//! Walk: a spot towards home (16 blocks, half-angle pi/10, height 3; else 8 blocks, pi/2, height 7); a spot that is not water,
//! for a turtle not near home (16), is replaced by 16 blocks, height 5, pi/2. No spot: stuck. The goal ends within 7 blocks of
//! home, when stuck, or after 300 ticks of counting near home (`reduced(600)`).
use super::turtle_state::{self as ts, Turtle};
use super::*;
use crate::spots::{Pick, pick_for};
use crate::{cow::*, goals::*};
use horizon_client_lib::mobs::MobWorld;
use std::f64::consts::{FRAC_PI_2, PI};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    /// One chance in `chance` at each request (`reduced(700)` = 350).
    pub chance: i32,
    /// The stray distance: a turtle this far or more from home may go home by chance.
    pub near: f64,
    /// The goal ends within this distance of home.
    pub stop: f64,
    /// `reduced(600)`: the ticks near home the walk may last.
    pub give_up: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, chance: 350, near: 64.0, stop: 7.0, give_up: 300 };
}
/// The state of the goal: stuck, the ticks counted near home, whether a deadline is armed.
#[derive(Component, Clone, Copy, Debug)]
pub struct GoHome {
    pub def: &'static Params,
    pub stuck: bool,
    pub close_ticks: i32,
    pub armed: bool,
    pub slot: u8,
}
/// The goal runs: its system visits it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct GoingHome;

pub static MODULE: Module = Module {
    name: "turtle_go_home",
    flags: 0,
    equip,
    arm: arm_first,
    fired,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(GoHome { def: params::<Params>(p), stuck: false, close_ticks: 0, armed: false, slot: inst.slot });
    ts::ensure(w, e);
}
fn is_baby(w: &World, e: Entity) -> bool {
    w.get::<GrowAt>(e).is_some()
}
fn arm_first(w: &mut World, e: Entity, _: Inst) {
    if !is_baby(w, e) {
        idle(w, e);
    }
}
/// Arms the chance's deadline, once: the draw at each request, `2 * (geometric + 1)` ticks.
fn idle(w: &mut World, e: Entity) {
    let (slot, chance) = {
        let Some(mut h) = w.get_mut::<GoHome>(e) else { return };
        if h.armed {
            return;
        }
        h.armed = true;
        (h.slot, h.def.chance)
    };
    let wait = 2 * (with_rng(w, e, |r| geometric(r, chance)) + 1);
    crate::goals::arm(w, e, slot, wait);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if let Some(mut h) = w.get_mut::<GoHome>(e) {
        h.armed = false;
    }
    w.get::<GoingHome>(e).is_none()
}
/// `canUse`: a baby never; an egg always; otherwise the drawn chance counts only far from home (a near draw draws again).
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    if is_baby(w, e) {
        return false;
    }
    if ts::has_egg(w, e) {
        return true;
    }
    let near = w.get::<GoHome>(e).map_or(64.0, |h| h.def.near);
    if ts::close_to_home(w, e, near) {
        idle(w, e);
        return false;
    }
    true
}
fn start(w: &mut World, e: Entity, _: Inst) {
    ts::update(w, e, |t| t.going_home = true);
    if let Some(mut h) = w.get_mut::<GoHome>(e) {
        h.stuck = false;
        h.close_ticks = 0;
    }
    w.entity_mut(e).insert(GoingHome);
}
/// The end: going home is off; an egg asks again at once (the Mob asks at its next request), else a new draw.
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<GoingHome>();
    ts::update(w, e, |t| t.going_home = false);
    if ts::has_egg(w, e) {
        crate::goals::wake(w, e, &MODULE);
    } else {
        idle(w, e);
    }
}
/// The module's tick, called only while a creature that carries it lives (no registered system).
fn tick(w: &mut World) {
    crate::goals::tick_state(w, grown);
    crate::goals::tick_state(w, loaded);
    crate::goals::tick_state(w, home_run);
}
/// A baby that grew up starts drawing its chance.
fn grown(ev: Res<Events>, babies: Query<(), With<GoHome>>, mut reqs: ResMut<Requests>) {
    for e in ev.grew.iter() {
        if babies.contains(e) {
            reqs.0.push(Req::Call(e, idle_call));
        }
    }
}
/// A turtle that takes an egg heads home at once (`canUse` is true with an egg): the load wakes the goal, with no polling.
fn loaded(new: Query<Entity, (Added<crate::behaviours::lay_egg::Carrying>, With<GoHome>)>, mut reqs: ResMut<Requests>) {
    for e in &new {
        reqs.0.push(Req::Call(e, wake_call));
    }
}
fn wake_call(w: &mut World, e: Entity) {
    crate::goals::wake(w, e, &MODULE);
}
fn idle_call(w: &mut World, e: Entity) {
    idle(w, e);
}
/// Each tick of the goal: the end test, the count near home, and the next spot when the path is over (the spot is drawn by a
/// call, with the world).
fn home_run(
    mut q: Query<(Entity, &mut GoHome, &Turtle, &Pos, &Nav, Option<&NeedsPath>), With<GoingHome>>,
    carrying: Query<(), With<crate::behaviours::lay_egg::Carrying>>,
    mut reqs: ResMut<Requests>,
) {
    for (e, mut h, t, p, nav, need) in &mut q {
        let pos = [p.0.x, p.0.y, p.0.z];
        let far = !ts::is_near_home(t, pos, h.def.stop);
        if !(far && !h.stuck && h.close_ticks <= h.def.give_up) {
            reqs.0.push(Req::End(e, h.slot));
            // The Mob stops this goal near home and restarts it at once (an egg: `canUse` is true), and the restart ticks:
            // the same tick draws the next spot when the path is over. The turtle is led away from its target.
            if carrying.contains(e) && need.is_none() && nav.path.as_ref().is_none_or(|p| p.is_done()) {
                reqs.0.push(Req::Call(e, restart_spot));
            }
            continue;
        }
        if ts::is_near_home(t, pos, 16.0) {
            h.close_ticks += 1;
        }
        if need.is_none() && nav.path.as_ref().is_none_or(|p| p.is_done()) {
            reqs.0.push(Req::Call(e, next_spot));
        }
    }
}
/// The tick of the restart that follows a stop near home with an egg: the spot is drawn as in `next_spot`.
fn restart_spot(w: &mut World, e: Entity) {
    if ts::has_egg(w, e) {
        next_spot_inner(w, e);
    }
}
fn next_spot(w: &mut World, e: Entity) {
    if w.get::<GoingHome>(e).is_none() {
        return;
    }
    next_spot_inner(w, e);
}
/// The spots the next draws return, in order, instead of the picker's (a test inserts it to give the ECS the Mob's draws; its
/// absence is the normal state). An empty queue draws nothing.
#[derive(Resource, Default)]
pub struct ImposedSpots(pub std::collections::VecDeque<[f64; 3]>);
fn next_spot_inner(w: &mut World, e: Entity) {
    if let Some(mut q) = w.get_resource_mut::<ImposedSpots>() {
        if let Some(at) = q.0.pop_front() {
            let speed = w.get::<GoHome>(e).map_or(1.0, |h| h.def.speed);
            go_to_at(w, e, at, speed, 1);
        }
        return;
    }
    let t = ts::get(w, e);
    let speed = w.get::<GoHome>(e).map_or(1.0, |h| h.def.speed);
    let to = [f64::from(t.home[0]) + 0.5, f64::from(t.home[1]), f64::from(t.home[2]) + 0.5];
    let close = ts::close_to_home(w, e, 16.0);
    let mut next = pick_for(w, e, Pick::DefaultTowards { h: 16, v: 3, to, max_rad: PI / 10.0 })
        .or_else(|| pick_for(w, e, Pick::DefaultTowards { h: 8, v: 7, to, max_rad: FRAC_PI_2 }));
    if let Some(at) = next
        && !close
        && !is_water(w.resource::<Level>().mw.0, at)
    {
        next = pick_for(w, e, Pick::DefaultTowards { h: 16, v: 5, to, max_rad: FRAC_PI_2 });
    }
    match next {
        Some(at) => go_to_at(w, e, at, speed, 1),
        None => {
            if let Some(mut h) = w.get_mut::<GoHome>(e) {
                h.stuck = true;
            }
        }
    }
}
fn is_water(mw: &MobWorld, at: [f64; 3]) -> bool {
    mw.block_name(at[0].floor() as i32, at[1].floor() as i32, at[2].floor() as i32) == "minecraft:water"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::species::Config;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 4, &Params::DEFAULT)]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// A cow with this goal alone on the slab, its home at `home` (the cow starts at x 0.5).
    fn world(home: [i32; 3]) -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(200, false, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.goals = config();
        let c = spawn_species(&mut h.world, &sp, 0.5, 0.5, 77);
        h.world.get_mut::<ActiveUntil>(c).unwrap().0 = u64::MAX;
        h.world.resource_mut::<crate::goals::Entered>().0.push(c);
        h.tick();
        ts::update(&mut h.world, c, |t| t.home = home);
        Some((h, c))
    }
    fn ticks(h: &mut Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }

    /// A turtle with an egg goes home at once: the goal runs at the next selector tick.
    #[test]
    fn an_egg_sends_a_turtle_home_at_once() {
        let Some((mut h, c)) = world([20, 0, 0]) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        h.world.entity_mut(c).insert(crate::behaviours::lay_egg::Carrying);
        crate::goals::wake(&mut h.world, c, &MODULE);
        ticks(&mut h, 20);
        assert!(running(&h, c), "témoin : un œuf fait partir la tortue");
        assert!(ts::get(&h.world, c).going_home, "la tortue rentre");
    }

    /// Far from home, the chance sends the turtle out at a drawn tick; near home, the same draws start nothing.
    #[test]
    fn a_stray_turtle_goes_home_by_chance_and_a_home_turtle_does_not() {
        let Some((mut h, c)) = world([80, 0, 0]) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        let mut started = false;
        for _ in 0..4000 {
            h.tick();
            started |= running(&h, c);
            if started {
                break;
            }
        }
        assert!(started, "témoin : à 80 blocs, un tirage fait partir la tortue");

        let Some((mut h, c)) = world([0, 0, 0]) else { return };
        ticks(&mut h, 4000);
        assert!(!running(&h, c), "à la maison, aucun départ par tirage");
    }

    /// The walk leads home and ends within 7 blocks of it.
    #[test]
    fn the_walk_ends_within_seven_blocks_of_home() {
        let Some((mut h, c)) = world([20, 0, 0]) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        h.world.entity_mut(c).insert(crate::behaviours::lay_egg::Carrying);
        crate::goals::wake(&mut h.world, c, &MODULE);
        ticks(&mut h, 20);
        assert!(running(&h, c), "témoin : le départ");
        h.world.entity_mut(c).remove::<crate::behaviours::lay_egg::Carrying>();
        ticks(&mut h, 2000);
        let p = h.world.get::<Pos>(c).unwrap().0;
        assert!(p.x > 13.0, "la tortue s'est approchée de sa maison (x = {})", p.x);
        assert!(!running(&h, c), "le but s'arrête près de la maison");
    }
}
