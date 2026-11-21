//! `Fox.FoxStrollThroughVillageGoal` (`fox.rs:928-1020`, `StrollThroughVillageGoal(fox, 200)`): at night, a fox within 6 sections of a
//! village point picks a spot toward the village heart (the best of ten candidates by score: fewer sections to a village point) and
//! walks there. The chance is a geometric wait of `reduced(200)` asks, two ticks apart; a refused ask arms the next wait.
//! NOT PORTED, DOUTE (the notes say why): (1) the `tick` of the goal, 10-block steps toward a point 0.4 of the way back from the wanted
//! spot, on the motion-blocking surface, and `moveRandomly`: it needs the framework to call `tick` while the goal runs (`arbitrate`), and
//! a path that ends the goal (`ends_with_path`, kept so the goal can never be stuck) stops it after the first walk; (2) the candidates:
//! `Pick::Land` is already the best of ten by walk value (`spots.rs`), so the score here is applied to ten picks, not to ten candidates
//! (not the `Mob`'s law: BESOIN DU CADRE, a `Pick` scored by a function); (3) the dimension (`has_fixed_time`) is absent: the
//! overworld is assumed; (4) `DEFENDING` is not a fox flag here; (5) `clear_states` (`ints[0]`) has no equivalent.
use super::*;
use super::fox_state;
use crate::{cow::*, goals::*, poi::PoiRegistry, spots::Pick, targeting::Target};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The chance is one in `interval` per ask (`nextInt(reduced(interval)) == 0`).
    pub interval: i32,
    /// The walk starts only within this many sections of a village point (`isCloseToVillage(pos, 6)`).
    pub near_sections: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { interval: 200, near_sections: 6 };
}
/// The goal's state: its parameters and the spot it wants (`wantedPos`).
#[derive(Component, Clone, Copy, Debug)]
pub struct Village {
    pub interval: i32,
    pub near: i32,
    pub wanted: Option<[i32; 3]>,
}

pub static MODULE: Module = Module {
    name: "fox_stroll_through_village",
    flags: MOVE,
    ends_with_path: true,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(Village { interval: p.interval, near: p.near_sections, wanted: None });
}
/// The next chance: a geometric wait of `reduced(interval)` asks, two ticks apart (`idle_wait`).
fn arm_chance(w: &mut World, e: Entity, inst: Inst) {
    let n = w.get::<Village>(e).map_or(200, |v| v.interval);
    let wait = with_rng(w, e, |r| idle_wait(r, (n + 1) / 2));
    crate::goals::arm(w, e, inst.slot, wait.max(2));
}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    arm_chance(w, e, inst);
}
/// A deadline of the module asks `canUse`.
fn fired(_: &mut World, _: Entity, _: Inst) -> bool {
    true
}
fn block_pos(p: [f64; 3]) -> [i32; 3] {
    [p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32]
}
fn section_of(p: [i32; 3]) -> [i32; 3] {
    [p[0] >> 4, p[1] >> 4, p[2] >> 4]
}
/// `Level.isBrightOutside` for the overworld: the day is the level's `Daylight`.
fn daylight(w: &World) -> bool {
    w.get_resource::<crate::hostile::Daylight>().is_some_and(|d| d.0)
}
/// `Fox.canFoxMove` without the defending flag (not a fox flag here: DOUTE): not asleep, not sitting, no target.
fn can_fox_move(w: &World, e: Entity) -> bool {
    !fox_state::has(w, e, fox_state::SLEEPING | fox_state::SITTING) && w.get::<Target>(e).is_none()
}
/// `canUse`: the checks of the `Mob` in its order (riding, daylight, the chance, the village, the spot, `canFoxMove`); a refusal arms
/// the next chance.
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let (Some(p), Some(v)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Village>(e).copied()) else { return false };
    let refuse = |w: &mut World| {
        arm_chance(w, e, inst);
        false
    };
    if is_occupied(w, e, OCC_MOUNTED) || daylight(w) {
        return refuse(w);
    }
    let pos = block_pos([p.x, p.y, p.z]);
    let near = w.get_resource::<PoiRegistry>().is_some_and(|r| r.sections_to_village(section_of(pos)) <= v.near);
    if !near {
        return refuse(w);
    }
    let spots = scored_spots(w, e);
    let best = {
        let Some(reg) = w.get_resource::<PoiRegistry>() else { return refuse(w) };
        let mut best: Option<([i32; 3], f64)> = None;
        for p in spots {
            let b = block_pos(p);
            let score = -f64::from(reg.sections_to_village(section_of(b)));
            if best.is_none_or(|(_, s)| score > s) {
                best = Some((b, score));
            }
        }
        best.map(|(b, _)| b)
    };
    w.get_mut::<Village>(e).unwrap().wanted = best;
    if best.is_some() && can_fox_move(w, e) { true } else { refuse(w) }
}
/// The ten candidates, each a `Land` pick (the best of ten by walk value, see the module's DOUTE 2), kept in draw order.
fn scored_spots(w: &mut World, e: Entity) -> Vec<[f64; 3]> {
    (0..10).filter_map(|_| crate::spots::pick_for(w, e, Pick::Land { h: 15, v: 7 })).collect()
}
/// `nav_move_to(wanted)` at speed 1, ending within one block: the walk's first path (see the module's DOUTE 1).
fn start(w: &mut World, e: Entity, _: Inst) {
    let Some(v) = w.get::<Village>(e).copied() else { return };
    let Some(wanted) = v.wanted else { return };
    let c = [f64::from(wanted[0]) + 0.5, f64::from(wanted[1]), f64::from(wanted[2]) + 0.5];
    go_to_at(w, e, c, 1.0, 1);
}
/// The walk is over: the spot is forgotten and the next chance is armed.
fn stop(w: &mut World, e: Entity, inst: Inst) {
    if let Some(mut v) = w.get_mut::<Village>(e) {
        v.wanted = None;
    }
    arm_chance(w, e, inst);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::species::Config;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 5, &Params::DEFAULT)]));
        Config { entries }
    }
    /// A zombie with this goal alone, standing at `x` on the slab, with a bell (a village point) in its own section; `None` without the
    /// test resources.
    fn world(x: f64) -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(200, true, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:zombie").unwrap();
        sp.goals = config();
        let b = spawn_species(&mut h.world, &sp, x, 0.5, 33);
        h.world.get_mut::<ActiveUntil>(b).unwrap().0 = u64::MAX;
        let y = h.world.get::<Pos>(b).unwrap().0.y.floor() as i32;
        h.set_block(8, y + 1, 8, "minecraft:bell");
        for _ in 0..3 {
            h.tick();
        }
        crate::goals::arm_all(&mut h.world);
        h.tick();
        Some((h, b))
    }
    fn ever_running(h: &mut Herd, b: Entity, ticks: usize) -> bool {
        let mut started = false;
        for _ in 0..ticks {
            h.tick();
            started |= h.world.get::<GoalSet>(b).unwrap().is_running(&MODULE);
        }
        started
    }

    /// At night, four sections from a village point: the walk starts. The same by day, or seven sections away at night: it never does.
    /// The first case is the witness of the two others.
    #[test]
    fn a_fox_near_a_village_at_night_walks_and_by_day_or_far_away_it_does_not() {
        let Some((mut h, b)) = world(0.5) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        assert!(ever_running(&mut h, b, 1500), "a fox near a village at night never started its walk");

        let Some((mut day, d)) = world(0.5) else { return };
        day.world.resource_mut::<crate::hostile::Daylight>().0 = true;
        assert!(!ever_running(&mut day, d, 1500), "the walk started by day");

        let Some((mut far, f)) = world(120.5) else { return };
        assert!(!ever_running(&mut far, f, 1500), "the walk started seven sections from the village");
    }
}
