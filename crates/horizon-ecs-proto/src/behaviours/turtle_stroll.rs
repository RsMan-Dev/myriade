//! `Turtle.TurtleRandomStrollGoal` (fiche 49): the stroll of `stroll.rs` with the turtle's gate (no stroll in water, going home or
//! with an egg) and its own spot (`LandRandomPos(10, 7)`). The wait is `stroll`'s (one chance in `reduced(interval)` per ask, an
//! ask every 2 ticks); the walk is `fly_stroll`'s. The gate is read when the attempt comes up: a turtle that leaves the water
//! asks again at once, a turtle that enters it does not walk (the attempt is not re-armed while the gate is shut).
use super::turtle_state as ts;
use super::*;
use crate::rules::turtle_stroll::may_stroll;
use crate::spots::{Pick, pick_for};
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    /// The turtle's `Stroll.interval` (100): one chance in `reducedTickDelay(interval)` per ask.
    pub interval: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, interval: 100 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct TurtleStroll {
    pub speed: f64,
    pub slot: u8,
}

pub static MODULE: Module = Module {
    name: "turtle_stroll",
    flags: MOVE,
    ends_with_path: true,
    blocked: Some(blocked),
    equip,
    arm: arm_first,
    fired,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(TurtleStroll { speed: p.speed, slot: inst.slot });
    if let Some(mut s) = w.get_mut::<Stroll>(e) {
        s.interval = (p.interval + 1) / 2;
    }
    ts::ensure(w, e);
}
fn arm_first(w: &mut World, e: Entity, _: Inst) {
    arm_now(w, e);
}
fn gate(w: &World, e: Entity) -> bool {
    let t = ts::get(w, e);
    may_stroll(w.get::<FluidState>(e).is_some_and(|f| f.in_water), t.going_home, ts::has_egg(w, e))
}
/// Arms the next attempt, unless one is armed.
fn arm_now(w: &mut World, e: Entity) {
    let Some(mut s) = w.get::<Stroll>(e).copied() else { return };
    if s.scheduled {
        return;
    }
    let wait = with_rng(w, e, |r| idle_wait(r, s.interval));
    arm_module(w, e, &MODULE, wait);
    s.scheduled = true;
    *w.get_mut::<Stroll>(e).unwrap() = s;
}
fn blocked(w: &mut World, e: Entity, _: Inst) {
    arm_now(w, e);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = crate::cow::now_of(w, e);
    let active = w.get::<ActiveUntil>(e).map_or(0, |a| a.0);
    let Some(mut s) = w.get_mut::<Stroll>(e) else { return false };
    s.scheduled = false;
    !(s.walking || now >= active)
}
/// The attempt came up: the gate, then a spot; without one the attempt is armed again.
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    if !gate(w, e) {
        return false;
    }
    let Some(mut s) = w.get::<Stroll>(e).copied() else { return false };
    let forced = take_xz(w, e, inst.slot);
    let picked = match forced {
        Some((x, z)) => Some([x, f64::from(s.ty), z]),
        None => pick_for(w, e, Pick::Land { h: 10, v: 7 }),
    };
    match picked {
        Some(p) => {
            s.walking = true;
            (s.tx, s.tz, s.ty) = (p[0], p[2], p[1].floor() as i32);
            *w.get_mut::<Stroll>(e).unwrap() = s;
            true
        }
        None => {
            *w.get_mut::<Stroll>(e).unwrap() = s;
            arm_now(w, e);
            false
        }
    }
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let speed = w.get::<TurtleStroll>(e).map_or(1.0, |g| g.speed);
    w.get_mut::<Nav>(e).unwrap().speed = speed;
    w.entity_mut(e).insert(NeedsPath);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.get_mut::<Stroll>(e).unwrap().walking = false;
    stop_nav(w, e);
    arm_now(w, e);
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(leave_water.in_set(slot));
}
/// A turtle that leaves the water and has no attempt armed gets one now.
fn leave_water(ev: Res<Events>, idle: Query<Entity, (With<TurtleStroll>, With<Stroll>)>, mut reqs: ResMut<Requests>) {
    for e in ev.left_fluid.iter() {
        if idle.contains(e) {
            reqs.0.push(Req::Call(e, rearm));
        }
    }
}
fn rearm(w: &mut World, e: Entity) {
    if gate(w, e) && w.get::<Stroll>(e).is_some_and(|s| !s.walking) {
        arm_now(w, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::species::Config;
    use horizon_core::geometry::Vec3;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 9, &Params::DEFAULT)]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    fn cow(h: &mut Herd, x: f64, seed: u64) -> Entity {
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.goals = config();
        let c = spawn_species(&mut h.world, &sp, x, 0.5, seed);
        h.world.get_mut::<ActiveUntil>(c).unwrap().0 = u64::MAX;
        h.world.resource_mut::<crate::goals::Entered>().0.push(c);
        c
    }
    fn world() -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(200, true, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        for x in 40..=44 {
            for z in 0..=4 {
                h.set_block(x, 0, z, "minecraft:water");
            }
        }
        Some(h)
    }

    /// On land a turtle strolls (a walk starts within 2000 ticks); a turtle in the water, in the same run, does not.
    #[test]
    fn a_turtle_on_land_strolls_and_one_in_water_does_not() {
        let Some(mut h) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        let land = cow(&mut h, 0.5, 61);
        let wet = cow(&mut h, 42.5, 62);
        h.teleport(wet, Vec3::new(42.5, 1.0, 2.5));
        let (mut land_walks, mut wet_walks) = (false, false);
        for _ in 0..2000 {
            h.tick();
            land_walks |= running(&h, land);
            wet_walks |= running(&h, wet);
        }
        assert!(land_walks, "témoin : la tortue sur terre se promène");
        assert!(!wet_walks, "pas de promenade dans l'eau");
    }

    /// Over 3000 ticks an adult turtle on land walks about 80 % of the time, stopping between walks (vanilla 77 to 82 %).
    #[test]
    fn a_turtle_stops_between_walks() {
        walks(None, 0.5);
    }
    /// The real bench: the home is 400 blocks away and the floor ends 10 blocks from the turtles, air beyond. `TurtleGoHomeGoal`
    /// starts by chance; at the edge no spot has ground under it, `stuck` ends it, and the turtle strolls again (it walked 97 %).
    #[test]
    fn a_turtle_with_a_far_home_stops_between_walks() {
        walks(Some([-610, 0, 0]), -189.5);
    }
    fn walks(home: Option<[i32; 3]>, x0: f64) {
        let Some(mut h) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        if home.is_some() {
            for x in -250..=-196 {
                for z in -60..=60 {
                    for y in -40..=0 {
                        h.set_block(x, y, z, "minecraft:air");
                    }
                }
            }
        }
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let sp = *table.iter().find(|s| s.name == "minecraft:turtle").unwrap();
        let ts: Vec<Entity> = (0..5u32)
            .map(|k| {
                let t = spawn_species(&mut h.world, &sp, x0 + 2.0 * f64::from(k), 0.5, 61 + u64::from(k));
                h.world.get_mut::<ActiveUntil>(t).unwrap().0 = u64::MAX;
                h.world.resource_mut::<crate::goals::Entered>().0.push(t);
                if let Some(home) = home {
                    crate::behaviours::turtle_state::update(&mut h.world, t, |s| s.home = home);
                }
                t
            })
            .collect();
        let mut last: Vec<_> = ts.iter().map(|&t| h.world.get::<Pos>(t).unwrap().0).collect();
        let mut stat = vec![(0, 0, 0); ts.len()];
        for _ in 0..3000 {
            h.tick();
            for (i, &t) in ts.iter().enumerate() {
                let p = h.world.get::<Pos>(t).unwrap().0;
                let (moving, idle, stops) = &mut stat[i];
                if (p.x - last[i].x).hypot(p.z - last[i].z) > 1.0e-3 {
                    *moving += 1;
                    *stops += i32::from(*idle >= 6);
                    *idle = 0;
                } else {
                    *idle += 1;
                }
                last[i] = p;
            }
        }
        let moving: i32 = stat.iter().map(|s| s.0).sum();
        let stops: i32 = stat.iter().map(|s| s.2).sum();
        assert!(moving <= 2700 * 5, "part du temps en marche: {moving}/15000");
        assert!(stops >= 3 * 5, "arrêts de 6 ticks ou plus: {stops}");
    }
}
