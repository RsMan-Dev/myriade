//! `FleeSunGoal`: a monster that burns in the daylight, sees the sky and has no target walks to a shaded spot within 10
//! blocks. A spot is taken when the sky does not see it and its walk value is below zero (a brightness above 0.5, the
//! monster's `getWalkTargetValue`). The try is drawn when the goal is asked, 10 tries at most.
use super::*;
use crate::{cow::*, goals::*, hostile::Daylight, targeting::Target};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The speed modifier of the run.
    pub speed: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0 };
}
/// The parameters, the rank of the goal (the wake-ups name it), and the spot `can_use` chose for `start`: the x and z
/// of the target, and its block row.
#[derive(Component, Clone, Copy, Debug)]
pub struct FleeSunGoal {
    pub speed: f64,
    pub slot: u8,
    pick: (f64, i32, f64),
}

pub static MODULE: Module = Module {
    name: "flee_sun",
    flags: MOVE,
    ends_with_path: true,
    equip,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Living, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(FleeSunGoal { speed: p.speed, slot: inst.slot, pick: (0.0, 0, 0.0) });
}
/// The ticks of the module (no system in the schedule): run by `arbitrate` while an animal carries the goal.
fn tick(w: &mut World) {
    crate::goals::tick_state(w, wake_on_fire);
    crate::goals::tick_state(w, wake_on_day);
}
/// A monster that catches fire asks for the goal (`canUse` is asked at the selector's ticks). Any ignition is seen here:
/// The framework's `Events.ignited` says it (read once, the tick after).
fn wake_on_fire(ev: Res<Events>, q: Query<&FleeSunGoal>, mut reqs: ResMut<Requests>) {
    for e in ev.ignited.iter() {
        if let Ok(g) = q.get(e) {
            reqs.0.push(Req::Start(e, g.slot));
        }
    }
}
/// The day came or went: the monsters that burn now look at the sun again (the burning ones only).
fn wake_on_day(day: Res<Daylight>, q: Query<(Entity, &FleeSunGoal), With<Burning>>, mut reqs: ResMut<Requests>) {
    if day.is_changed() {
        for (e, g) in &q {
            reqs.0.push(Req::Start(e, g.slot));
        }
    }
}
fn pick(w: &mut World, e: Entity, v: (f64, i32, f64)) {
    w.get_mut::<FleeSunGoal>(e).unwrap().pick = v;
}
/// `FleeSunGoal.canUse`: no target, daylight, burning, no helmet (not modelled: DOUBT, see the notes), the own block sees
/// the sky, then a hiding spot. `Forced` spots are taken as they are, at the block row of the animal.
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let now = now_of(w, e);
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let burning = w.get::<Burning>(e).is_some_and(|b| b.until > now);
    // Not burning, or at night: the ignition and the change of day wake the goal; no ask until then.
    if !burning || !w.resource::<Daylight>().0 {
        return false;
    }
    // The `Mob` asks every other tick: while it burns by day, a target (`Target`) that goes, a roof that is removed or
    // a spot that appears are seen by asking again in 2 ticks (a deadline, only for the animals that burn).
    let mw = w.resource::<Level>().mw.0;
    let (bx, by, bz) = (pos.x.floor() as i32, pos.y.floor() as i32, pos.z.floor() as i32);
    if w.get::<Target>(e).is_some() || !mw.can_see_sky(bx, by, bz) {
        arm(w, e, inst.slot, 2);
        return false;
    }
    if let Some((x, z)) = take_xz(w, e, inst.slot) {
        pick(w, e, (x, by, z));
        return true;
    }
    // `hide_pos`: 10 tries, each drawing x, then y, then z.
    let mut found = None;
    for _ in 0..10 {
        let (dx, dy, dz) = with_rng(w, e, |r| (r.next_int(20) - 10, r.next_int(6) - 3, r.next_int(20) - 10));
        let (x, y, z) = (bx + dx, by + dy, bz + dz);
        if !mw.can_see_sky(x, y, z) && f64::from(mw.brightness(x, y, z)) > 0.5 {
            found = Some((x, y, z));
            break;
        }
    }
    match found {
        Some((x, y, z)) => {
            pick(w, e, (f64::from(x) + 0.5, y, f64::from(z) + 0.5));
            true
        }
        None => {
            // No spot this time: asked again in 2 ticks while the animal burns.
            arm(w, e, inst.slot, 2);
            false
        }
    }
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let g = *w.get::<FleeSunGoal>(e).unwrap();
    let (x, y, z) = g.pick;
    crate::goals::go_to_at(w, e, [x, f64::from(y), z], g.speed, 1);
}
/// The path ended (or another goal took the flags): while the animal still burns, it is asked again in 2 ticks, as the
/// selector asks a goal that is not running.
fn stop(w: &mut World, e: Entity, inst: Inst) {
    let now = now_of(w, e);
    if w.get::<Burning>(e).is_some_and(|b| b.until > now) {
        arm(w, e, inst.slot, 2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{cow_bundle, Herd};

    /// The flee goal alone: the test configuration of the animal (`species.rs` is not changed).
    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 3, &Params::DEFAULT)];

    fn skip(test: &str) -> bool {
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            crate::cow::skip_note(test, "HORIZON_TEST_RESOURCES is not set");
            return true;
        }
        false
    }
    /// An animal of the process's species whose only goal is the flee goal, in the open, standing on the slab (it has
    /// settled for 5 ticks: a goal woken in the air gets no path, see the notes).
    fn settled_animal(h: &mut Herd, x: f64, z: f64) -> Entity {
        let e = h.world.spawn(cow_bundle(x, z, 7)).id();
        h.world.entity_mut(e).insert(GoalSet::new(&ENTRIES));
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        (MODULE.equip)(&mut h.world, e, Inst { slot: 0, nth: 0 }, &Params::DEFAULT);
        for _ in 0..5 {
            h.tick();
        }
        e
    }
    /// Sets the animal on fire for 160 ticks.
    fn ignite(h: &mut Herd, e: Entity) {
        let now = h.world.resource::<Clock>().0;
        h.world.entity_mut(e).insert(Burning { until: now + 160 });
        h.world.resource_mut::<Events>().ignited.push(e);
    }
    fn at(h: &Herd, e: Entity) -> (f64, f64) {
        let p = h.world.get::<Pos>(e).unwrap().0;
        (p.x, p.z)
    }

    /// In the sun, burning, with a spot given 4 blocks away: the animal walks there. The spot is imposed (`Forced`): the
    /// test world has no lighting (a block set does not relight it), so no shade can be built to test the draw.
    #[test]
    fn a_burning_animal_in_the_day_walks_to_its_spot() {
    if crate::cow::skip_unless_farm_goals("a_burning_animal_in_the_day_walks_to_its_spot") {
        return;
    }
    if crate::cow::skip_unless_small_body("a_burning_animal_in_the_day_walks_to_its_spot") {
        return;
    }
    if crate::cow::skip_unless_walker("a_burning_animal_in_the_day_walks_to_its_spot") {
        return;
    }
        if skip("a_burning_animal_in_the_day_walks_to_its_spot") {
            return;
        }
        let mut h = Herd::with_slab_awake(20, false, false);
        h.set_day(true);
        let e = settled_animal(&mut h, -6.5, 0.5);
        force_spot(&mut h.world, e, &MODULE, (-2.5, -3.5));
        ignite(&mut h, e);
        for _ in 0..100 {
            h.tick();
        }
        let (x, z) = at(&h, e);
        assert!((x + 2.5).abs() < 0.6 && (z + 3.5).abs() < 0.6, "the animal did not reach its spot: ({x}, {z})");
    }

    /// At night the burning animal does not run, even with a spot given: the daylight is checked first.
    #[test]
    fn a_burning_animal_at_night_does_not_run() {
    if crate::cow::skip_unless_small_body("a_burning_animal_at_night_does_not_run") {
        return;
    }
    if crate::cow::skip_unless_walker("a_burning_animal_at_night_does_not_run") {
        return;
    }
        if skip("a_burning_animal_at_night_does_not_run") {
            return;
        }
        let mut h = Herd::with_slab_awake(20, false, false);
        h.set_day(false);
        let e = settled_animal(&mut h, -6.5, 0.5);
        force_spot(&mut h.world, e, &MODULE, (-2.5, -3.5));
        ignite(&mut h, e);
        let start = at(&h, e);
        for _ in 0..100 {
            h.tick();
        }
        let end = at(&h, e);
        assert!((end.0 - start.0).abs() < 1e-6 && (end.1 - start.1).abs() < 1e-6, "it ran at night: {start:?} -> {end:?}");
    }
}
