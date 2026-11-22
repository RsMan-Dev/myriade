//! `TryFindLand` (`ai/behavior/TryFindLand`, the frog's SWIM and IDLE activities, `FrogAi`): an animal standing in water draws the
//! Manhattan box of `range` around its feet; the first cell out of the water, empty, with solid ground below and not in its own
//! column, is the walk target at reach 1. A search runs at most once per 60 ticks (a deadline, not a counter). The wake after a
//! failed search is armed by the chain; a walk that ends is followed by the next search, which the deadline still gates.
use super::*;
use crate::rules::lay_spawn::manhattan_cells;
use crate::{cow::*, goals::*};
use horizon_client_lib::blocks::Fluid;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The Manhattan radius of the box searched (`TryFindLand(range, speed)`).
    pub range: i32,
    /// The speed modifier of the walk.
    pub speed: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { range: 6, speed: 1.0 };
}
/// The ticks between two searches (`TryFindLand`'s cooldown).
const COOLDOWN: i32 = 60;

/// The state of the module: its parameters, the next search (a deadline), the point the walk aims at, the chain's slot.
#[derive(Component, Clone, Copy, Debug)]
pub struct Land {
    pub p: Params,
    pub next: u64,
    pub wanted: [f64; 3],
    pub slot: u8,
}
/// The walk runs (its path ends the goal).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Landing;

pub static MODULE: Module = Module {
    name: "try_find_land",
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
    w.entity_mut(e).insert(Land { p, next: 0, wanted: [0.0; 3], slot: inst.slot });
}
fn arm_first(w: &mut World, e: Entity, inst: Inst) {
    crate::goals::arm(w, e, inst.slot, 1);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Landing>(e).is_none()
}
fn block_of(w: &World, e: Entity) -> [i32; 3] {
    let p = w.get::<Pos>(e).map_or_else(Default::default, |p| p.0);
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
/// The first cell of the box that `dry` accepts, outside the animal's own column (`TryFindLand`'s filter, nearest first).
pub fn first_landing(at: [i32; 3], range: i32, dry: impl Fn([i32; 3]) -> bool) -> Option<[i32; 3]> {
    manhattan_cells(at, range).into_iter().find(|p| (p[0] != at[0] || p[2] != at[2]) && dry(*p))
}
/// `canUse`: in water at the feet, past the deadline, then the first dry cell of the box becomes the walk target.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let l = *w.get::<Land>(e).unwrap();
    let now = now_of(w, e);
    if now < l.next {
        return false;
    }
    let at = block_of(w, e);
    let mw = w.resource::<Level>().mw.0;
    if mw.fluid(at[0], at[1], at[2]) != Fluid::Water {
        return false;
    }
    let dry = |p: [i32; 3]| mw.collision_bounds(p).is_none() && mw.fluid(p[0], p[1], p[2]) != Fluid::Water && mw.collision_bounds([p[0], p[1] - 1, p[2]]).is_some();
    let found = first_landing(at, l.p.range, dry);
    let mut l = l;
    l.next = now + COOLDOWN as u64;
    if let Some(p) = found {
        l.wanted = [f64::from(p[0]) + 0.5, f64::from(p[1]), f64::from(p[2]) + 0.5];
    }
    *w.get_mut::<Land>(e).unwrap() = l;
    if found.is_none() {
        crate::goals::arm(w, e, l.slot, COOLDOWN);
    }
    found.is_some()
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let l = *w.get::<Land>(e).unwrap();
    go_to_at(w, e, l.wanted, l.p.speed, 1);
    w.entity_mut(e).insert(Landing);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Landing>();
    let slot = w.get::<Land>(e).unwrap().slot;
    crate::goals::arm(w, e, slot, 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_landing_skips_own_column() {
        assert_eq!(first_landing([0, 64, 0], 2, |c| c[0] == 1), Some([1, 64, 0]));
        assert_eq!(first_landing([0, 64, 0], 2, |c| c[0] == 0), Some([0, 64, 1]));
    }
}
