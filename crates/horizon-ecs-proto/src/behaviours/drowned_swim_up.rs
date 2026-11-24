//! `DrownedSwimUpGoal` (`mobs/undead.rs:438-477`, priority 6, flags 0): at night, a drowned in the water below the sea level minus 2
//! swims up. While it runs it sets `flag2` (searching for land): the marker `WantsToSwim` here. Each tick while it is below
//! `sea - 1` and its path is done (or within 2 blocks of the path's target), it picks a point towards `(x, sea - 1, z)` with
//! `DefaultRandomPos.getPosTowards(4, 8, pi/2)` and walks there at speed 1.0; no point: `stuck`, and the goal ends at the next tick.
//! The request while out of the water is a chain of +2 deadlines that runs while the animal is in the water (the condition it
//! tests is then the depth and the night, which move without an event); the event `entered_fluid` starts the chain again.
use super::*;
use crate::locomotion::swim::WantsToSwim;
use crate::spots::Pick;
use crate::hostile::Daylight;
use crate::rules::drowned_swim_up::{below_surface, close_to_target, deep_enough, rise_target};
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub h: i32,
    pub v: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { h: 4, v: 8 };
}

/// The state: the picker's box, whether the goal found no point, whether a request of the chain is armed.
#[derive(Component, Clone, Copy, Debug)]
pub struct Rising {
    pub h: i32,
    pub v: i32,
    pub stuck: bool,
    pub armed: bool,
    pub slot: u8,
}
/// The goal runs: its tick (`tick_all`) visits it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct SwimmingUp;

pub static MODULE: Module = Module {
    name: "drowned_swim_up",
    tick: Some(tick),
    flags: 0,
    equip,
    arm: arm_first,
    fired,
    can_use,
    start,
    stop,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let def = *params::<Params>(p);
    w.entity_mut(e).insert(Rising { h: def.h, v: def.v, stuck: false, armed: false, slot: inst.slot });
}
fn chain(w: &mut World, e: Entity, wait: i32) {
    let slot = {
        let Some(mut r) = w.get_mut::<Rising>(e) else { return };
        if r.armed {
            return;
        }
        r.armed = true;
        r.slot
    };
    crate::goals::arm(w, e, slot, wait);
}
fn arm_first(w: &mut World, e: Entity, _: Inst) {
    chain(w, e, 1);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if let Some(mut r) = w.get_mut::<Rising>(e) {
        r.armed = false;
    }
    w.get::<SwimmingUp>(e).is_none()
}
fn in_water(w: &World, e: Entity) -> bool {
    w.get::<FluidState>(e).is_some_and(|f| f.in_water)
}
/// `canUse` (and `canContinue` without `!stuck`): night, in the water, below `sea - 2`.
fn up_ok(w: &World, e: Entity) -> bool {
    let sea = w.resource::<Level>().mw.0.sea_level;
    let y = w.get::<Pos>(e).unwrap().0.y;
    !w.resource::<Daylight>().0 && in_water(w, e) && deep_enough(y, sea)
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    if up_ok(w, e) {
        return true;
    }
    if in_water(w, e) {
        chain(w, e, 2);
    }
    false
}
fn start(w: &mut World, e: Entity, _: Inst) {
    if let Some(mut r) = w.get_mut::<Rising>(e) {
        r.stuck = false;
    }
    w.entity_mut(e).insert((SwimmingUp, WantsToSwim));
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<SwimmingUp>();
    w.entity_mut(e).remove::<WantsToSwim>();
    if in_water(w, e) {
        chain(w, e, 2);
    }
}
/// The goal, each tick: it ends when `canContinue` fails (out of the conditions, or stuck); otherwise it walks towards the
/// surface when its path is done or near its target (`closeToNextPos`: squared distance under 4). Runs from the selector.
fn tick(w: &mut World) {
    static ID: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    let id = *ID.get_or_init(|| crate::species::registry().module_id(&MODULE));
    let Some(i) = id else { return };
    if w.resource::<Carriers>().0.get(i).is_none_or(|n| *n == 0) {
        return;
    }
    let woken: Vec<Entity> = w.resource::<Events>().entered_fluid.iter().collect();
    for e in woken {
        if w.get::<Rising>(e).is_some() && w.get::<SwimmingUp>(e).is_none() {
            chain(w, e, 1);
        }
    }
    let swimming: Vec<Entity> = crate::queries::entities::<With<SwimmingUp>>(w);
    for e in swimming {
        let stuck = w.get::<Rising>(e).is_some_and(|r| r.stuck);
        if !up_ok(w, e) || stuck {
            end(w, e, &MODULE);
            continue;
        }
        let sea = f64::from(w.resource::<Level>().mw.0.sea_level);
        let p = w.get::<Pos>(e).unwrap().0;
        let nav = w.get::<Nav>(e).unwrap();
        let done = nav.path.as_ref().is_none_or(|path| path.is_done());
        let close = nav.path.as_ref().is_some_and(|path| close_to_target(p, path.target.map(f64::from)));
        if below_surface(p.y, sea) && (done || close) {
            let (h, v) = {
                let r = w.get::<Rising>(e).unwrap();
                (r.h, r.v)
            };
            let to = rise_target(p, sea);
            match crate::spots::pick_for(w, e, Pick::DefaultTowards { h, v, to, max_rad: std::f64::consts::FRAC_PI_2 }) {
                None => w.get_mut::<Rising>(e).unwrap().stuck = true,
                Some(at) => go_to_at(w, e, at, 1.0, 1),
            }
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
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 6, &Params::DEFAULT)]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    fn step(h: &mut Herd) {
        h.tick();
    }
    fn run(h: &mut Herd, n: usize) {
        for _ in 0..n {
            step(h);
        }
    }
    /// A drowned in a deep column of water over a stone platform at `sea - 12`; water from `sea - 11` to `sea + 1` over x, z -8..=8.
    fn world() -> Option<(Herd, Entity, i32)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(200, false, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:drowned").unwrap();
        sp.goals = config();
        let sea = h.world.resource::<Level>().mw.0.sea_level;
        let c = spawn_species(&mut h.world, &sp, 0.5, 0.5, 77);
        h.world.get_mut::<ActiveUntil>(c).unwrap().0 = u64::MAX;
        h.world.resource_mut::<crate::goals::Entered>().0.push(c);
        for x in -8..=8 {
            for z in -8..=8 {
                h.set_block(x, sea - 12, z, "minecraft:stone");
                for y in sea - 11..=sea + 1 {
                    h.set_block(x, y, z, "minecraft:water");
                }
            }
        }
        run(&mut h, 2);
        h.teleport(c, Vec3::new(0.5, f64::from(sea - 8), 0.5));
        run(&mut h, 4);
        Some((h, c, sea))
    }
    fn skip(name: &str) -> bool {
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            eprintln!("skipped {name}: HORIZON_TEST_RESOURCES unset");
            return true;
        }
        false
    }

    /// At night, deep in the water, the drowned starts, marks itself as searching for land and rises: its depth goes up by
    /// more than 5 blocks at some tick of the walk (it sinks back between two strokes, which the swim control decides).
    #[test]
    fn a_deep_drowned_at_night_starts_and_rises() {
        if skip("a_deep_drowned_at_night_starts_and_rises") {
            return;
        }
        let (mut h, c, _) = world().unwrap();
        h.set_day(false);
        let y0 = h.world.get::<Pos>(c).unwrap().0.y;
        let mut started = false;
        for _ in 0..800 {
            step(&mut h);
            started |= running(&h, c);
            if started {
                break;
            }
        }
        assert!(started, "témoin : le départ de nuit, sous la surface");
        assert!(h.world.get::<WantsToSwim>(c).is_some(), "le noyé qui remonte veut nager");
        let mut top = y0;
        for _ in 0..900 {
            step(&mut h);
            top = top.max(h.world.get::<Pos>(c).unwrap().0.y);
        }
        assert!(top > y0 + 5.0, "le noyé est monté : y {y0} -> {top}");
    }

    /// The goal ends at the surface: once the drowned is at `sea - 1` (not below `sea - 2`), `canContinue` fails.
    #[test]
    fn the_rise_ends_at_the_surface() {
        if skip("the_rise_ends_at_the_surface") {
            return;
        }
        let (mut h, c, sea) = world().unwrap();
        h.set_day(false);
        let mut started = false;
        for _ in 0..800 {
            step(&mut h);
            started |= running(&h, c);
            if started {
                break;
            }
        }
        assert!(started, "témoin : le départ");
        h.teleport(c, Vec3::new(0.5, f64::from(sea - 1), 0.5));
        step(&mut h);
        step(&mut h);
        assert!(!running(&h, c), "l'arrêt à la surface");
        assert!(h.world.get::<WantsToSwim>(c).is_none(), "le marqueur tombe avec le but");
    }

    /// By day the drowned, deep in the water, does not start; it starts at the next night: the night is the witness.
    #[test]
    fn by_day_the_deep_drowned_does_not_rise() {
        if skip("by_day_the_deep_drowned_does_not_rise") {
            return;
        }
        let (mut h, c, _) = world().unwrap();
        h.set_day(true);
        run(&mut h, 800);
        assert!(!running(&h, c), "pas de départ le jour");
        h.set_day(false);
        let mut started = false;
        for _ in 0..800 {
            step(&mut h);
            started |= running(&h, c);
            if started {
                break;
            }
        }
        assert!(started, "témoin : le départ à la nuit suivante");
    }
}
