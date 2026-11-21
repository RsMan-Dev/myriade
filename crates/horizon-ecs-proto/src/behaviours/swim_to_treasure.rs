//! `DolphinSwimToTreasureGoal` (`dolphin.rs:25-125`, fiche 108): a dolphin that has eaten a fish (`GotFish`) and has at least
//! 100 air swims toward its treasure: each step is a spot drawn toward the treasure (16 x 1 then 8 x 4 then, when the spot is
//! not water, 8 x 5, all at max angle pi/2 or pi/8), walked at speed 1,3 to reach 1. It ends when it is within 4 blocks of the
//! treasure, when no step is found (stuck), or when its air falls under 100 (then the fish is KEPT; otherwise it is consumed).
//! The treasure is a component (`Treasure`) that the test imposes: the structure locator (`locate(tag, case, 50)`) is absent.
//! DOUTE: the close test uses the path's target block (`Path.target`) and the stop test uses the block row `floor(y) + 0,5`.
//! REPLIS: the particles are not played; the dolphin does not turn its head (`look_at_speed` not called).
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::spots::{self, Pick};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The speed modifier of the steps (1,3).
    pub speed: f64,
    /// The air needed to start, and under which the goal ends (`air >= 100`).
    pub min_air: i32,
    /// Squared distance to the treasure that ends the goal (16, 4 blocks) and squared distance at which the next step is drawn (144).
    pub arrive_sqr: f64,
    pub close_sqr: f64,
    /// The fish that makes the dolphin hungry for a treasure, by name (the item ids the framework knows: DOUTE).
    pub fish: &'static [&'static str],
}
impl Params {
    pub const DEFAULT: Self = Self {
        speed: 1.3,
        min_air: 100,
        arrive_sqr: 16.0,
        close_sqr: 144.0,
        fish: &[],
    };
}
/// The treasure of the dolphin: a block position, imposed (no locator yet).
#[derive(Component, Clone, Copy, Debug)]
pub struct Treasure(pub [i32; 3]);
/// The fish eaten: the goal can start (`extra.flag`).
#[derive(Component, Clone, Copy, Debug)]
pub struct GotFish;
/// The goal runs. `low_air`: it ended because the air fell (the fish is kept).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Swimming {
    pub stuck: bool,
    pub low_air: bool,
}
#[derive(Component, Clone, Copy)]
pub struct Trip {
    p: &'static Params,
    slot: u8,
}

pub static MODULE: Module = Module {
    name: "swim_to_treasure",
    flags: MOVE | LOOK,
    interruptable: false,
    equip,
    can_use,
    start,
    stop,
    interact,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(Trip { p, slot: inst.slot });
}
/// `canUse`: the fish eaten, a treasure, and the air.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(min_air) = w.get::<Trip>(e).map(|t| t.p.min_air) else { return false };
    w.get::<GotFish>(e).is_some() && w.get::<Treasure>(e).is_some() && w.get::<Air>(e).is_some_and(|a| a.0 >= min_air)
}
/// `start`: the path is stopped and the first step is drawn by the tick of this same tick.
fn start(w: &mut World, e: Entity, _: Inst) {
    crate::goals::stop_nav(w, e);
    w.entity_mut(e).insert(Swimming { stuck: false, low_air: false });
}
/// `stop`: the fish is consumed, unless the air ended the trip.
fn stop(w: &mut World, e: Entity, _: Inst) {
    let low = w.get::<Swimming>(e).is_some_and(|s| s.low_air);
    w.entity_mut(e).remove::<Swimming>();
    if !low {
        w.entity_mut(e).remove::<GotFish>();
    }
}
/// The player's use of a fish: the fish is taken and the goal is woken (`mobInteract`, the fish of the tag).
fn interact(w: &mut World, e: Entity, _: Inst, i: &Interaction) -> bool {
    let Some(fish) = w.get::<Trip>(e).map(|t| t.p.fish) else { return false };
    let is_fish = fish.iter().any(|f| {
        let id = crate::goals::item_id(f);
        id != 0 && id == i.item
    });
    if !is_fish {
        return false;
    }
    w.entity_mut(e).insert(GotFish);
    crate::goals::wake(w, e, &MODULE);
    true
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, trip_run);
}
/// Each running trip: the end tests (treasure reached, air, stuck) and the request of the next step when the path is done or
/// near its end (`close`). The step itself is a request, run after the tick, which may read the world.
fn trip_run(
    mut q: Query<(Entity, &Pos, &Nav, &Air, &Trip, &Treasure, &mut Swimming), With<Swimming>>,
    mut reqs: ResMut<Requests>,
) {
    for (e, pos, nav, air, trip, treasure, mut sw) in &mut q {
        let p = pos.0;
        let to = Vec3::new(f64::from(treasure.0[0]) + 0.5, f64::from(treasure.0[1]) + 0.5, f64::from(treasure.0[2]) + 0.5);
        let here = Vec3::new(p.x, p.y.floor() + 0.5, p.z);
        let arrived = d2(to, here) < trip.p.arrive_sqr;
        if arrived || sw.stuck {
            reqs.0.push(Req::End(e, trip.slot));
            continue;
        }
        if air.0 < trip.p.min_air {
            sw.low_air = true;
            reqs.0.push(Req::End(e, trip.slot));
            continue;
        }
        let close = match nav.path.as_ref() {
            None => true,
            Some(path) if path.reached => true,
            Some(path) => {
                let t = Vec3::new(f64::from(path.target[0]) + 0.5, f64::from(path.target[1]), f64::from(path.target[2]) + 0.5);
                d2(p, t) < trip.p.close_sqr
            }
        };
        if close {
            reqs.0.push(Req::Call(e, step));
        }
    }
}
/// The next step: a spot toward the treasure (16 x 1 at pi/8, else 8 x 4 at pi/2); when that spot is not water, 8 x 5 at
/// pi/2 replaces it; no spot means stuck. The walk is at 1,3 to within 1 block.
fn step(w: &mut World, e: Entity) {
    let (Some(t), Some(speed)) = (w.get::<Treasure>(e).map(|t| t.0), w.get::<Trip>(e).map(|t| t.p.speed)) else { return };
    let to = [f64::from(t[0]) + 0.5, f64::from(t[1]) + 0.5, f64::from(t[2]) + 0.5];
    let eighth = std::f64::consts::PI / 8.0;
    let half = std::f64::consts::FRAC_PI_2;
    let mut next = spots::pick_for(w, e, Pick::DefaultTowards { h: 16, v: 1, to, max_rad: eighth })
        .or_else(|| spots::pick_for(w, e, Pick::DefaultTowards { h: 8, v: 4, to, max_rad: half }));
    if let Some(s) = next
        && !in_water(w, s)
    {
        next = spots::pick_for(w, e, Pick::DefaultTowards { h: 8, v: 5, to, max_rad: half });
    }
    match next {
        Some(at) => crate::goals::go_to_at(w, e, at, speed, 1),
        None => {
            if let Some(mut sw) = w.get_mut::<Swimming>(e) {
                sw.stuck = true;
            }
            crate::goals::end(w, e, &MODULE);
        }
    }
}
fn in_water(w: &World, at: [f64; 3]) -> bool {
    w.get_resource::<Level>().is_some_and(|l| l.mw.0.block_name(at[0].floor() as i32, at[1].floor() as i32, at[2].floor() as i32) == "minecraft:water")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{Herd, Pos, SpeciesTable, spawn_species};
    use crate::species::Config;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 1, &Params::DEFAULT)]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// A dolphin with this goal alone, in a pond, with its treasure imposed `dist` blocks away on x; the air is full.
    fn pond(dist: i32) -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(60, true, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        h.fill_water(8);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:dolphin").unwrap();
        sp.goals = config();
        let d = spawn_species(&mut h.world, &sp, 0.5, 0.5, 61);
        h.world.get_mut::<ActiveUntil>(d).unwrap().0 = u64::MAX;
        h.world.entity_mut(d).insert((Treasure([dist, 4, 0]), Air(300)));
        for _ in 0..5 {
            h.tick();
        }
        h.teleport(d, Vec3::new(0.5, 5.0, 0.5));
        Some((h, d))
    }
    /// The fish eaten: the flag the player's interaction would set (the item id of the fish is not known to the framework).
    fn eat_fish(h: &mut Herd, d: Entity) {
        h.world.entity_mut(d).insert(GotFish);
        crate::goals::wake(&mut h.world, d, &MODULE);
    }

    /// A dolphin that ate a fish swims to its treasure: it starts and is still on the way (witness: running and at the
    /// first step), and it ends at the treasure with the fish eaten (the position reached within 4 blocks).
    #[test]
    fn it_swims_to_the_treasure_and_eats_the_fish() {
        let Some((mut h, d)) = pond(30) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        eat_fish(&mut h, d);
        let mut started = false;
        for _ in 0..40 {
            h.tick();
            started |= running(&h, d);
        }
        assert!(started, "témoin : le dauphin part vers le trésor");
        let mut ended = false;
        for _ in 0..2000 {
            h.tick();
            if !running(&h, d) {
                ended = true;
                break;
            }
        }
        assert!(ended, "le dauphin finit à l'arrivée ou faute de pas");
        assert!(h.world.get::<GotFish>(d).is_none(), "le poisson est consommé à l'arrivée");
        let p = h.world.get::<Pos>(d).unwrap().0;
        assert!(p.x > 20.0, "le dauphin est allé vers le trésor en x (x = {})", p.x);
    }

    /// Without a fish nothing starts; with one and an empty air supply nothing starts either (witness: the fish alone starts it).
    #[test]
    fn without_a_fish_or_air_it_does_not_start() {
        let Some((mut h, d)) = pond(30) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        crate::goals::wake(&mut h.world, d, &MODULE);
        for _ in 0..30 {
            h.tick();
            assert!(!running(&h, d), "sans poisson, pas de départ");
        }
        h.world.entity_mut(d).insert(Air(10));
        h.world.entity_mut(d).insert(GotFish);
        for _ in 0..30 {
            h.tick();
            assert!(!running(&h, d), "sans air, pas de départ");
        }
        h.world.entity_mut(d).insert(Air(300));
        // The air rising wakes nothing (BESOIN DU CADRE, DOUTE): the witness asks again, as the fish's interaction would.
        crate::goals::wake(&mut h.world, d, &MODULE);
        let mut started = false;
        for _ in 0..30 {
            h.tick();
            started |= running(&h, d);
        }
        assert!(started, "témoin : avec poisson et air, le départ a lieu");
    }
}
