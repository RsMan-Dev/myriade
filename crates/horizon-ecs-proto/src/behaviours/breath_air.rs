//! `Dolphin.BreathAirGoal` (`mobs/goals.rs:2896-2960`): MOVE | LOOK, not interruptible; `canUse` is `air < 140`; `start` stops the
//! navigation and looks for air; `tick` looks for air again and walks to it. `canContinue` is `canUse` (the air rises above 140 and the
//! goal ends). The deadline: the air falls by one a tick while the eyes are under, so the tick it crosses 140 is known at once
//! (`air - 140` ticks); no ask every two ticks. Not ported: the push `move_relative(0.02, xxa/yya/zza)` of `tick` (the inputs of
//! the mob's move control are not in the prototype: DOUTE), the sound and the bubble column's particles.
use super::*;
use crate::{cow::*, goals::*};
use horizon_client_lib::blocks::Fluid;
use horizon_client_lib::mobs::data::PathComputationType;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The air below which the animal looks for air (`m.air < 140`).
    pub threshold: i32,
    /// How high the search goes above the animal (`findAirPosition`: `y..=y + 8`).
    pub reach_up: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { threshold: 140, reach_up: 8 };
}
/// The goal's settings, from the entry (read at each deadline and each tick).
#[derive(Component, Clone, Copy, Debug)]
pub struct AirSearch {
    pub threshold: i32,
    pub reach_up: i32,
}
/// The goal runs: the animal swims to the air it found.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Surfacing;

pub static MODULE: Module = Module {
    name: "breath_air",
    tick: Some(tick),
    flags: MOVE | LOOK,
    interruptable: false,
    retry_on_end: true,
    equip,
    arm: arm_entry,
    fired,
    can_use,
    start,
    stop,
    requires: &[Cap::Living, Cap::Locomotion, Cap::Goals],
    ..Module::DEFAULT
};
/// Once at creation: the settings of the entry.
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(AirSearch { threshold: p.threshold, reach_up: p.reach_up });
}
fn settings(w: &World, e: Entity) -> Option<AirSearch> {
    w.get::<AirSearch>(e).copied()
}
/// The wait before the goal is asked again, from the air now: the crossing of the threshold when the eyes are under (the air
/// falls by one a tick), else a poll of 20 ticks while the eyes are out (the air does not fall: DOUTE, the moment the eyes go
/// under is not an event of the framework). `None`: the air is already under the threshold, the goal is due now.
fn wait_for(w: &World, e: Entity) -> Option<i32> {
    let s = settings(w, e)?;
    let air = w.get::<Air>(e)?.0;
    if air < s.threshold {
        return None;
    }
    let eyes_under = w.get::<FluidState>(e).is_some_and(|f| f.eye_in_water);
    Some(if eyes_under { (air - s.threshold).max(1) } else { 20 })
}
/// Arms the deadline of the entry `inst` (or wakes it when the air is already short).
fn arm_entry(w: &mut World, e: Entity, inst: Inst) {
    match wait_for(w, e) {
        Some(wait) => crate::goals::arm(w, e, inst.slot, wait),
        None => crate::goals::wake(w, e, &MODULE),
    }
}
/// A deadline of this entry came up: ask `canUse` unless the goal already runs (the framework replays a start that came
/// off the phase through a deadline: `true` must then be answered for it).
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    !w.get::<GoalSet>(e).is_some_and(|g| g.is_running(&MODULE))
}
/// `BreathAirGoal.canUse`: `air < 140`. When it says no, the next deadline is armed (`retry_on_end` and the deadlines of the
/// events keep the chain going).
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(s) = settings(w, e) else { return false };
    let short = w.get::<Air>(e).is_some_and(|a| a.0 < s.threshold);
    if !short {
        arm_entry(w, e, inst);
    }
    short
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Surfacing);
    stop_nav(w, e);
    find_air(w, e);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Surfacing>();
    stop_nav(w, e);
}

/// `BreathAirGoal.findAirPosition`: the centre, north, east, south and west columns, each from `y` up to `y + reach_up`; the
/// first block with no fluid (or a bubble column) that the land navigation can stand in. `None`: none found.
fn air_position(w: &World, [x, y, z]: [i32; 3], reach_up: i32) -> Option<[i32; 3]> {
    let mw = w.resource::<Level>().mw.0;
    let air_ok = |[x, y, z]: [i32; 3]| {
        let s = mw.world.block(x, y, z);
        (mw.fluid(x, y, z) == Fluid::None || mw.block_name(x, y, z) == "minecraft:bubble_column") && mw.data.pathfindable(s, PathComputationType::Land)
    };
    for (dx, dz) in [(0, 0), (0, -1), (1, 0), (0, 1), (-1, 0)] {
        for yy in y..=y + reach_up {
            let p = [x + dx, yy, z + dz];
            if air_ok(p) {
                return Some(p);
            }
        }
    }
    None
}
/// `BreathAirGoal.findAir`: walk to the row above the air found (`nav_move_to(.., 1.0)`), or else to the row `reach_up` above
/// the animal.
fn find_air(w: &mut World, e: Entity) {
    let Some(s) = settings(w, e) else { return };
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return };
    let (x, y, z) = (pos.x.floor() as i32, pos.y.floor() as i32, pos.z.floor() as i32);
    let d = air_position(w, [x, y, z], s.reach_up).unwrap_or([x, (pos.y + f64::from(s.reach_up)).floor() as i32, z]);
    go_to_at(w, e, [f64::from(d[0]), f64::from(d[1] + 1), f64::from(d[2])], 1.0, 1);
}

/// The ticks of the running goal, run by `arbitrate` (`Module::tick`) while a creature carries it (one call, no system: the module costs the
/// schedule nothing). A goal that runs looks for air each tick; it ends when the air is back above the threshold. The
/// animals that entered a fluid get their deadline (or wake at once when the air is short).
fn tick(w: &mut World) {
    static ID: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    let id = ID.get_or_init(|| crate::species::registry().module_id(&MODULE));
    let carried = id.is_some_and(|i| w.resource::<Carriers>().0.get(i).is_some_and(|n| *n > 0));
    if !carried {
        return;
    }
    let entered: Vec<Entity> = w.resource::<Events>().entered_fluid.iter().collect();
    for e in entered {
        let Some(slot) = slot_of(w, e, &MODULE) else { continue };
        match wait_for(w, e) {
            Some(wait) => crate::goals::arm(w, e, slot, wait),
            None => crate::goals::wake(w, e, &MODULE),
        }
    }
    let running: Vec<Entity> = w.query_filtered::<Entity, With<Surfacing>>().iter(w).collect();
    for e in running {
        let Some(s) = settings(w, e) else { continue };
        if w.get::<Air>(e).is_some_and(|a| a.0 >= s.threshold) {
            crate::goals::end(w, e, &MODULE);
        } else {
            find_air(w, e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Config;

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 0, &Params::DEFAULT)];

    /// One tick of the world, then the goal's tick (the call `arbitrate` makes in the game: the test calls it itself).
    fn step(h: &mut Herd) {
        h.tick();
    }
    fn herd() -> Option<Herd> {
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            eprintln!("breath_air: skipped (HORIZON_TEST_RESOURCES unset)");
            return None;
        }
        let mut h = Herd::with_slab_awake(30, false, false);
        set_lazy_look(&mut h.world, false);
        Some(h)
    }
    /// A cow body renamed dolphin, whose only goal is the breathing, in a pool (water on y 1..=8 around its spot), its
    /// air set and its entry announced to the framework (`Entered`: the deadlines are armed).
    fn dolphin(h: &mut Herd, air: i32) -> Entity {
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.name = "minecraft:dolphin";
        sp.goals = Config { entries: &ENTRIES };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
        for x in -4..=4 {
            for z in 3..=11 {
                for y in 1..=8 {
                    h.set_block(x, y, z, "minecraft:water");
                }
            }
        }
        h.world.flush();
        h.world.get_mut::<Pos>(e).unwrap().0 = horizon_core::geometry::Vec3::new(0.5, 3.0, 7.5);
        h.world.get_mut::<Air>(e).unwrap().0 = air;
        h.world.resource_mut::<Entered>().0.push(e);
        e
    }

    /// An animal whose air is short (100), under water, starts the goal and walks up to the first air above it: the goal
    /// runs, and its target row is the one above the first air column (the centre column is water up to y 8, air at 9: row 10).
    #[test]
    fn a_dolphin_short_of_air_starts_and_walks_to_the_air_above_it() {
        let Some(mut h) = herd() else { return };
        let e = dolphin(&mut h, 100);
        for _ in 0..4 {
            step(&mut h);
        }
        assert!(h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE), "short of air under water: the goal runs");
        assert_eq!(h.world.get::<Stroll>(e).unwrap().ty, 10, "the walk goes to the row above the first air (y 9)");
    }

    /// Control: with air above the threshold (300) the same animal does not start the goal.
    #[test]
    fn a_dolphin_with_air_does_not_start_the_goal() {
        let Some(mut h) = herd() else { return };
        let e = dolphin(&mut h, 300);
        for _ in 0..3 {
            step(&mut h);
        }
        assert!(!h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE), "air 300: no start");
        assert!(h.world.get::<Air>(e).unwrap().0 >= 140, "the control has its air");
    }

    /// The goal ends once the air is back above the threshold (the animal breathes at the surface).
    #[test]
    fn the_goal_ends_when_the_air_is_back_above_the_threshold() {
        let Some(mut h) = herd() else { return };
        let e = dolphin(&mut h, 100);
        for _ in 0..4 {
            step(&mut h);
        }
        assert!(h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE), "precondition: the goal runs");
        h.world.get_mut::<Air>(e).unwrap().0 = 150;
        for _ in 0..3 {
            step(&mut h);
        }
        assert!(!h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE), "air 150: the goal has ended");
    }
}
