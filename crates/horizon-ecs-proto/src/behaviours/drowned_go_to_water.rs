//! `Drowned.DrownedGoToWaterGoal` (`mobs/undead.rs:1008-1050`, priority 1, MOVE): by day, out of the water, a drowned draws up to
//! 10 points around it (`x + nextInt(20) - 10`, `y + 2 - nextInt(8)`, `z + nextInt(20) - 10`) and walks to the first that is the
//! water BLOCK (kelp, seagrass and waterlogged blocks are not). It runs until its path is done (`ends_with_path`).
//! Written by event: the ask is a deadline chain (2 ticks while it is day and out of the water, as the `Mob` asks), the event
//! `left_fluid` restarts it, and at night out of the water a slow retry (100 ticks) stands for the missing "day began" event
//! (a recorded gap: `suivi/ecs-ecarts.md`). In the water nothing is armed: `left_fluid` wakes it.
use super::*;
use crate::hostile::Daylight;
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    /// Ticks between two asks while it is night and the drowned is out of the water.
    pub night_retry: i32,
}
impl Params {
    /// `Drowned.DrownedGoToWaterGoal` moves at 1.0.
    pub const DEFAULT: Self = Self { speed: 1.0, night_retry: 100 };
}

/// The state: the parameters, the water point found by `can_use`, whether a deadline is armed.
#[derive(Component, Clone, Copy, Debug)]
pub struct ToWater {
    pub p: Params,
    pub wanted: [f64; 3],
    pub armed: bool,
    pub slot: u8,
}

pub static MODULE: Module = Module {
    name: "drowned_go_to_water",
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
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(ToWater { p, wanted: [0.0; 3], armed: false, slot: inst.slot });
}
fn chain(w: &mut World, e: Entity, wait: i32) {
    let slot = {
        let Some(mut t) = w.get_mut::<ToWater>(e) else { return };
        if t.armed {
            return;
        }
        t.armed = true;
        t.slot
    };
    crate::goals::arm(w, e, slot, wait);
}
fn arm_first(w: &mut World, e: Entity, _: Inst) {
    chain(w, e, 1);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if let Some(mut t) = w.get_mut::<ToWater>(e) {
        t.armed = false;
    }
    true
}
fn in_water(w: &World, e: Entity) -> bool {
    w.get::<FluidState>(e).is_some_and(|f| f.in_water)
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    if in_water(w, e) {
        return false;
    }
    if !w.resource::<Daylight>().0 {
        let retry = w.get::<ToWater>(e).map_or(100, |t| t.p.night_retry);
        chain(w, e, retry);
        return false;
    }
    let p = w.get::<Pos>(e).unwrap().0;
    let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    for _ in 0..10 {
        let q = with_rng(w, e, |r| [bx + r.next_int(20) - 10, by + 2 - r.next_int(8), bz + r.next_int(20) - 10]);
        let mw = w.resource::<Level>().mw.0;
        // `getBlockState(pos).is(Blocks.WATER)`: kelp, seagrass and waterlogged blocks hold water but are not the water block.
        if &*mw.facts.get(mw.world.block(q[0], q[1], q[2])).name == "minecraft:water" {
            w.get_mut::<ToWater>(e).unwrap().wanted = [f64::from(q[0]) + 0.5, f64::from(q[1]), f64::from(q[2]) + 0.5];
            return true;
        }
    }
    chain(w, e, 2);
    false
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let t = *w.get::<ToWater>(e).unwrap();
    go_to_at(w, e, t.wanted, t.p.speed, 1);
}
/// The path ended: the ask starts again at once (the `Mob` asks every selector tick, out of the water by day).
fn stop(w: &mut World, e: Entity, _: Inst) {
    if !in_water(w, e) {
        chain(w, e, 2);
    }
}
/// `left_fluid` restarts the chain of a drowned that came out of the water.
fn tick(w: &mut World) {
    let woken: Vec<Entity> = w.resource::<Events>().left_fluid.iter().collect();
    for e in woken {
        if w.get::<ToWater>(e).is_some() {
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

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 1, &Params::DEFAULT)]));
        Config { entries }
    }
    fn skip(name: &str) -> bool {
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            eprintln!("skipped {name}: HORIZON_TEST_RESOURCES unset");
            return true;
        }
        false
    }
    /// A drowned on a stone floor at `sea`, a pool of water (x 3..=8, rows `sea-2..=sea+1`) beside it.
    fn world(day: bool) -> (Herd, Entity) {
        let mut h = Herd::with_slab(200, false, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:drowned").unwrap();
        sp.goals = config();
        let sea = h.world.resource::<Level>().mw.0.sea_level;
        let c = spawn_species(&mut h.world, &sp, 0.5, 0.5, 77);
        h.world.get_mut::<ActiveUntil>(c).unwrap().0 = u64::MAX;
        h.world.resource_mut::<crate::goals::Entered>().0.push(c);
        for x in -6..=10 {
            for z in -6..=6 {
                h.set_block(x, sea - 1, z, "minecraft:stone");
                for y in sea..=sea + 3 {
                    h.set_block(x, y, z, "minecraft:air");
                }
            }
        }
        for x in 3..=8 {
            for z in -3..=3 {
                for y in sea - 1..=sea + 1 {
                    h.set_block(x, y, z, "minecraft:water");
                }
            }
        }
        h.set_day(day);
        for _ in 0..2 {
            h.tick();
        }
        h.teleport(c, Vec3::new(-3.5, f64::from(sea), 0.5));
        (h, c)
    }
    fn started_within(h: &mut Herd, c: Entity, n: usize) -> bool {
        for _ in 0..n {
            h.tick();
            if h.world.get::<GoalSet>(c).unwrap().is_running(&MODULE) {
                return true;
            }
        }
        false
    }

    /// By day, out of the water, with a pool in reach: the goal starts, and its point is a water block.
    #[test]
    fn by_day_out_of_the_water_it_starts_for_a_water_block() {
        if skip("by_day_out_of_the_water_it_starts_for_a_water_block") {
            return;
        }
        let (mut h, c) = world(true);
        assert!(started_within(&mut h, c, 400), "temoin : le depart vers l'eau, de jour");
        let t = h.world.get::<ToWater>(c).unwrap().wanted;
        let mw = h.world.resource::<Level>().mw.0;
        let s = mw.world.block(t[0].floor() as i32, t[1].floor() as i32, t[2].floor() as i32);
        assert_eq!(&*mw.facts.get(s).name, "minecraft:water");
    }

    /// At night nothing starts; the same drowned starts once it is day (the retry stands for the missing event).
    #[test]
    fn at_night_it_stays_and_starts_at_day() {
        if skip("at_night_it_stays_and_starts_at_day") {
            return;
        }
        let (mut h, c) = world(false);
        assert!(!started_within(&mut h, c, 400), "pas de depart la nuit");
        h.set_day(true);
        assert!(started_within(&mut h, c, 400), "temoin : le depart au jour");
    }
}
