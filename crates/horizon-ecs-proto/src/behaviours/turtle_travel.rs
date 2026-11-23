//! `TurtleTravelGoal` (fiche 48): a turtle in water swims towards a spot up to 512 blocks away, a leg at a time. The goal has no
//! flag and no timer: it starts when the turtle enters water (or when it was in water at its creation), and it ends with each
//! path; the next leg is drawn when it starts again (`retry_on_end`). A leg that finds no spot arms a retry in 2 ticks, only
//! while the turtle is still in water.
use super::turtle_state as ts;
use super::*;
use crate::rules::turtle_travel::{travel_aim, travel_point};
use crate::spots::{Pick, pick_for};
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    /// The spot is drawn in `[-span/2, span/2[` on x and z, `[-vspan/2, vspan/2[` on y (`nextInt(1025) - 512`, `nextInt(9) - 4`).
    pub span: i32,
    pub vspan: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, span: 1025, vspan: 9 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Wander {
    pub def: &'static Params,
    pub armed: bool,
    pub slot: u8,
}
/// The goal runs: its end is watched (the path ended).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Traveling;

pub static MODULE: Module = Module {
    name: "turtle_travel",
    flags: 0,
    ends_with_path: false,
    retry_on_end: true,
    equip,
    arm: arm_first,
    fired,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Wander { def: params::<Params>(p), armed: false, slot: inst.slot });
    ts::ensure(w, e);
}
fn arm_first(w: &mut World, e: Entity, _: Inst) {
    retry_at(w, e, 1);
}
/// Arms the next `can_use`, once.
fn retry_at(w: &mut World, e: Entity, wait: i32) {
    let slot = {
        let Some(mut s) = w.get_mut::<Wander>(e) else { return };
        if s.armed {
            return;
        }
        s.armed = true;
        s.slot
    };
    crate::goals::arm(w, e, slot, wait);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if let Some(mut s) = w.get_mut::<Wander>(e) {
        s.armed = false;
    }
    w.get::<Traveling>(e).is_none()
}
fn in_water(w: &World, e: Entity) -> bool {
    w.get::<FluidState>(e).is_some_and(|f| f.in_water)
}
fn loaded(mw: &myriade_engine_lib::mobs::MobWorld, n: [f64; 3]) -> bool {
    let (xc, zc) = (n[0].floor() as i32, n[2].floor() as i32);
    ((xc - 34) >> 4..=(xc + 34) >> 4).all(|cx| ((zc - 34) >> 4..=(zc + 34) >> 4).all(|cz| mw.world.chunks.contains_key(&(cx, cz))))
}
/// `canUse`: in water, not going home, without an egg; then the draw of the spot and the first leg towards it (`start`
/// asks no more: the leg is the walk). A failed leg asks again in 2 ticks.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let t = ts::get(w, e);
    if t.going_home || ts::has_egg(w, e) || !in_water(w, e) {
        return false;
    }
    let Some(wd) = w.get::<Wander>(e).copied() else { return false };
    let (span, vspan) = (wd.def.span, wd.def.vspan);
    let (xt, yt, zt) = with_rng(w, e, |r| (r.next_int(span) - span / 2, r.next_int(vspan) - vspan / 2, r.next_int(span) - span / 2));
    let p = w.get::<Pos>(e).unwrap().0;
    let sea = w.resource::<Level>().mw.0.sea_level;
    let point = travel_point(p, [xt, yt, zt], sea);
    let to = travel_aim(point);
    let tenth = f64::from((std::f64::consts::PI / 10.0) as f32);
    let half = f64::from(std::f32::consts::FRAC_PI_2);
    let leg = match pick_for(w, e, Pick::DefaultTowards { h: 16, v: 3, to, max_rad: tenth }) {
        Some(n) => Some(n),
        None => pick_for(w, e, Pick::DefaultTowards { h: 8, v: 7, to, max_rad: half }),
    };
    let leg = leg.filter(|n| loaded(w.resource::<Level>().mw.0, *n));
    match leg {
        Some(n) => {
            ts::update(w, e, |t| t.travel = Some(point));
            go_to_at(w, e, n, 1.0, 1);
            true
        }
        None => {
            ts::update(w, e, |t| t.travel = None);
            retry_at(w, e, 2);
            false
        }
    }
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Traveling);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Traveling>();
    ts::update(w, e, |t| t.travel = None);
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((travel_events, travel_run).chain().in_set(slot));
}
/// Entering water wakes the goal (a `Req::Start`: the framework asks `can_use` at once).
fn travel_events(ev: Res<Events>, idle: Query<&Wander, Without<Traveling>>, mut reqs: ResMut<Requests>) {
    for e in ev.entered_fluid.iter() {
        if let Ok(s) = idle.get(e) {
            reqs.0.push(Req::Start(e, s.slot));
        }
    }
}
/// A travelling animal whose leg is over (no path, none being asked) ends the goal: it starts again on the next leg.
fn travel_run(q: Query<(Entity, &Nav, &Wander), (With<Traveling>, Without<NeedsPath>)>, mut reqs: ResMut<Requests>) {
    for (e, nav, wd) in &q {
        if nav.path.as_ref().is_none_or(|p| p.is_done()) {
            reqs.0.push(Req::End(e, wd.slot));
        }
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
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 7, &Params::DEFAULT)]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// A cow with this goal alone, on the slab, standing in a lake of water over the ground.
    fn world() -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(200, false, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        for x in -8..=8 {
            for z in -8..=8 {
                for y in 0..=2 {
                    h.set_block(x, y, z, "minecraft:water");
                }
            }
        }
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.goals = config();
        let c = spawn_species(&mut h.world, &sp, 40.5, 40.5, 91);
        h.world.get_mut::<ActiveUntil>(c).unwrap().0 = u64::MAX;
        h.world.resource_mut::<crate::goals::Entered>().0.push(c);
        for _ in 0..5 {
            h.tick();
        }
        Some((h, c))
    }
    fn ticks(h: &mut Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }

    /// In water, the goal starts and draws its spot (witness of the start); the swim itself is not tested: no path in liquid yet.
    #[test]
    fn a_cow_in_water_starts_a_trip() {
        let Some((mut h, c)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        h.teleport(c, Vec3::new(0.5, 1.0, 0.5));
        ticks(&mut h, 5);
        assert!(h.world.get::<FluidState>(c).unwrap().in_water, "témoin : la vache est dans l'eau");
        let mut started = false;
        for _ in 0..50 {
            h.tick();
            started |= running(&h, c);
            if started {
                break;
            }
        }
        assert!(started, "témoin : le départ dans l'eau");
        ticks(&mut h, 150);
        let drawn = super::turtle_state::get(&h.world, c).travel;
        assert!(drawn.is_some(), "le départ tire un point de voyage");
        // The swim itself waits for the amphibious navigation (`Nav` refuses paths in liquid, as `GroundPathNavigation` does).
    }

    /// Out of the water, the goal does not start (witness: the same cow starts once it is back in the water).
    #[test]
    fn a_cow_on_land_does_not_travel() {
        let Some((mut h, c)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        h.teleport(c, Vec3::new(40.5, 1.0, 40.5));
        ticks(&mut h, 200);
        assert!(!running(&h, c), "pas de voyage hors de l'eau");
        h.teleport(c, Vec3::new(0.5, 1.0, 0.5));
        let mut started = false;
        for _ in 0..100 {
            h.tick();
            started |= running(&h, c);
        }
        assert!(started, "témoin : le voyage repart dès l'entrée dans l'eau");
    }
}
