//! `TryFindWater` of the dolphin (`mobs/goals.rs:2965-3000`, priority 0) and `DrownedGoToWaterGoal` of the drowned
//! (`mobs/undead.rs:1008-1050`, priority 1): one module, two forms by `Params`. Ten draws of a point in the box 20 x 8 x 20
//! around the animal (x, then y, then z; the feet row + 2 - nextInt(8)); the first point that holds water is wanted, and the
//! walk to it ends with its path (`ends_with_path`). The `Mob` asks it each selector tick and draws nothing unless the place
//! holds: a background condition (out of the water, and on the ground for the dolphin) keeps a chain of +2 requests going;
//! the events `landed` and `left_fluid` start the chain again. Daylight (the drowned) is read at each ask: no event announces it.
use super::*;
use crate::hostile::Daylight;
use crate::{cow::*, goals::*};
use horizon_client_lib::blocks::Fluid;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The drowned: only in daylight (`!w.daylight` gives no walk).
    pub needs_day: bool,
    /// The dolphin: only on the ground (`!m.on_ground` gives no walk).
    pub needs_ground: bool,
    /// The drowned takes the block `minecraft:water` only; the dolphin any block whose fluid is water.
    pub exact_water_block: bool,
    /// The drowned aims at the middle of the block (x + 0,5, z + 0,5); the dolphin at its corner.
    pub centre: bool,
}
impl Params {
    pub const DEFAULT: Self = Self { needs_day: false, needs_ground: false, exact_water_block: false, centre: false };
}
/// The dolphin's parameters (`TryFindWater`): on the ground, any water, the corner.
pub const DOLPHIN: Params = Params { needs_day: false, needs_ground: true, exact_water_block: false, centre: false };
/// The drowned's parameters (`DrownedGoToWaterGoal`): in daylight, the water block, the middle.
pub const DROWNED: Params = Params { needs_day: true, needs_ground: false, exact_water_block: true, centre: true };

/// The state of the module: its parameters, the point the walk aims at, whether a request of the chain is armed.
#[derive(Component, Clone, Copy, Debug)]
pub struct Seek {
    pub def: Params,
    pub wanted: [f64; 3],
    pub armed: bool,
    pub slot: u8,
}
/// The walk runs (its path ends the goal).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Seeking;

pub static MODULE: Module = Module {
    name: "try_find_water",
    tick: Some(tick),
    flags: MOVE,
    ends_with_path: true,
    equip,
    arm: arm_first,
    fired,
    can_use,
    start,
    stop,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let def = *params::<Params>(p);
    w.entity_mut(e).insert(Seek { def, wanted: [0.0; 3], armed: false, slot: inst.slot });
}
/// Arms one request of the chain, once: a second arming would start a second chain for the animal.
fn chain(w: &mut World, e: Entity, wait: i32) {
    let slot = {
        let Some(mut s) = w.get_mut::<Seek>(e) else { return };
        if s.armed {
            return;
        }
        s.armed = true;
        s.slot
    };
    crate::goals::arm(w, e, slot, wait);
}
fn arm_first(w: &mut World, e: Entity, _: Inst) {
    chain(w, e, 1);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if let Some(mut s) = w.get_mut::<Seek>(e) {
        s.armed = false;
    }
    w.get::<Seeking>(e).is_none()
}
/// The background condition: out of the water, and on the ground when the form needs it. The chain runs while it holds.
fn out_of_water(w: &World, e: Entity, def: Params) -> bool {
    let in_water = w.get::<FluidState>(e).is_some_and(|f| f.in_water);
    let on_ground = w.get::<Contact>(e).is_some_and(|c| c.on_ground);
    !in_water && (!def.needs_ground || on_ground)
}
/// Every test of `canUse` before the draws: the daylight, then the background condition.
fn place_ok(w: &World, e: Entity, def: Params) -> bool {
    (!def.needs_day || w.resource::<Daylight>().0) && out_of_water(w, e, def)
}
fn feet(w: &World, e: Entity) -> [i32; 3] {
    let p = w.get::<Pos>(e).unwrap().0;
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
fn is_water(w: &World, p: [i32; 3], def: Params) -> bool {
    let mw = w.resource::<Level>().mw.0;
    if def.exact_water_block {
        mw.block_name(p[0], p[1], p[2]) == "minecraft:water"
    } else {
        mw.fluid(p[0], p[1], p[2]) == Fluid::Water
    }
}
/// `canUse`: the place, then at most ten draws; the first water point is the wanted one. Nothing found: the chain goes on.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let def = w.get::<Seek>(e).unwrap().def;
    if !place_ok(w, e, def) {
        if out_of_water(w, e, def) {
            chain(w, e, 2);
        }
        return false;
    }
    let b = feet(w, e);
    for _ in 0..10 {
        let p = with_rng(w, e, |r| [b[0] + r.next_int(20) - 10, b[1] + 2 - r.next_int(8), b[2] + r.next_int(20) - 10]);
        if is_water(w, p, def) {
            let d = if def.centre { 0.5 } else { 0.0 };
            w.get_mut::<Seek>(e).unwrap().wanted = [f64::from(p[0]) + d, f64::from(p[1]), f64::from(p[2]) + d];
            return true;
        }
    }
    chain(w, e, 2);
    false
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let wanted = w.get::<Seek>(e).unwrap().wanted;
    go_to_at(w, e, wanted, 1.0, 1);
    w.entity_mut(e).insert(Seeking);
}
/// The end of the goal (its path ended): the next ask comes at the next selector tick.
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Seeking>();
    chain(w, e, 2);
}
/// The wakes of the chain, read once per tick (the events of the tick before): an animal that lands or leaves the water asks
/// again. Runs from the selector, not as a system: a module adds none to the schedule. Costs nothing while no carrier lives.
fn tick(w: &mut World) {
    static ID: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    let id = *ID.get_or_init(|| crate::species::registry().module_id(&MODULE));
    let Some(i) = id else { return };
    if w.resource::<Carriers>().0.get(i).is_none_or(|n| *n == 0) {
        return;
    }
    let woken: Vec<Entity> = {
        let ev = w.resource::<Events>();
        ev.landed.iter().chain(ev.left_fluid.iter()).collect()
    };
    for e in woken {
        if w.get::<Seek>(e).is_some() && w.get::<Seeking>(e).is_none() {
            chain(w, e, 1);
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

    fn config(def: Params, species: bool) -> Config {
        let _ = species;
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 1, Box::leak(Box::new(def)))]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// One tick of the world and of the module's wakes (its `tick_all`, called by the selector in the game).
    fn step(h: &mut Herd) {
        h.tick();
    }
    fn run(h: &mut Herd, n: usize) {
        for _ in 0..n {
            step(h);
        }
    }
    /// An animal with the module alone, awake, on the slab; `name` is the species of the test.
    fn world(name: &str, def: Params) -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(200, false, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == name).unwrap();
        sp.goals = config(def, true);
        let c = spawn_species(&mut h.world, &sp, 0.5, 0.5, 77);
        h.world.get_mut::<ActiveUntil>(c).unwrap().0 = u64::MAX;
        h.world.resource_mut::<crate::goals::Entered>().0.push(c);
        h.world.resource_mut::<crate::hostile::Daylight>().0 = true;
        run(&mut h, 4);
        Some((h, c))
    }
    /// A pool of water over x 6..=9, z -3..=3, rows -1 to 1 (the search's heights reach these rows from the feet row 1).
    fn pool(h: &mut Herd) {
        set_rows(h, "minecraft:water");
    }
    fn set_rows(h: &mut Herd, name: &'static str) {
        for x in 6..=9 {
            for z in -3..=3 {
                for y in -1..=1 {
                    h.set_block(x, y, z, name);
                }
            }
        }
    }
    fn skip(name: &str) -> bool {
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            eprintln!("skipped {name}: HORIZON_TEST_RESOURCES unset");
            return true;
        }
        false
    }

    /// A drowned on the sand in daylight walks to a pool within reach: the start, the chosen point is in the pool, the walk ends.
    #[test]
    fn a_drowned_in_daylight_walks_to_a_pool_and_the_walk_ends() {
        if skip("a_drowned_in_daylight_walks_to_a_pool_and_the_walk_ends") {
            return;
        }
        let (mut h, c) = world("minecraft:drowned", DROWNED).unwrap();
        pool(&mut h);
        let mut started = false;
        for _ in 0..800 {
            step(&mut h);
            started |= running(&h, c);
            if started {
                break;
            }
        }
        assert!(started, "témoin : un départ vers la mare, de jour");
        let w = h.world.get::<Seek>(c).unwrap().wanted;
        assert!((6.0..=9.0).contains(&w[0]) && w[1] >= -1.0 && w[1] <= 1.0, "le point voulu est dans la mare : {w:?}");
        assert!((w[0].fract() - 0.5).abs() < 1e-9, "le drowned vise le centre du bloc");
        run(&mut h, 600);
        assert!(!running(&h, c), "la marche se termine à l'arrivée du chemin");
        let p: Vec3 = h.world.get::<Pos>(c).unwrap().0;
        assert!(p.x > 5.0, "le noyé est arrivé au bord de la mare (x = {})", p.x);
    }

    /// At night nothing starts; the same animal starts at the first day: the day is the witness.
    #[test]
    fn at_night_the_drowned_does_not_walk_and_at_day_it_does() {
        if skip("at_night_the_drowned_does_not_walk_and_at_day_it_does") {
            return;
        }
        let (mut h, c) = world("minecraft:drowned", DROWNED).unwrap();
        pool(&mut h);
        h.set_day(false);
        run(&mut h, 800);
        assert!(!running(&h, c), "pas de départ la nuit");
        h.set_day(true);
        let mut started = false;
        for _ in 0..800 {
            step(&mut h);
            started |= running(&h, c);
            if started {
                break;
            }
        }
        assert!(started, "témoin : le départ quand le jour vient");
    }

    /// A drowned in the water walks out to no pool: the event `left_fluid` restarts its chain once it is out.
    #[test]
    fn a_drowned_in_water_starts_only_once_it_has_left_it() {
        if skip("a_drowned_in_water_starts_only_once_it_has_left_it") {
            return;
        }
        let (mut h, c) = world("minecraft:drowned", DROWNED).unwrap();
        pool(&mut h);
        for x in -2..=2 {
            for z in -2..=2 {
                for y in 1..=2 {
                    h.set_block(x, y, z, "minecraft:water");
                }
            }
        }
        run(&mut h, 60);
        assert!(h.world.get::<FluidState>(c).unwrap().in_water, "le noyé est dans l'eau");
        let mut early = false;
        for _ in 0..200 {
            step(&mut h);
            early |= running(&h, c);
        }
        assert!(!early, "pas de départ tant qu'il est dans l'eau");
        for x in -2..=2 {
            for z in -2..=2 {
                for y in 1..=2 {
                    h.set_block(x, y, z, "minecraft:air");
                }
            }
        }
        let mut started = false;
        for _ in 0..800 {
            step(&mut h);
            started |= running(&h, c);
            if started {
                break;
            }
        }
        assert!(started, "témoin : le départ après être sorti de l'eau");
    }

    /// The dolphin's form (`needs_day` false, on the ground): a cow with these parameters starts at night.
    #[test]
    fn the_dolphin_form_walks_at_night_on_the_ground() {
        if skip("the_dolphin_form_walks_at_night_on_the_ground") {
            return;
        }
        let (mut h, c) = world("minecraft:cow", DOLPHIN).unwrap();
        pool(&mut h);
        h.set_day(false);
        let mut started = false;
        for _ in 0..800 {
            step(&mut h);
            started |= running(&h, c);
            if started {
                break;
            }
        }
        assert!(started, "témoin : la forme dauphin part la nuit sur le sol");
    }
}
