//! `Ghast.RandomFloatAroundGoal`: a ghast with nowhere to float to (no wanted spot, or the spot reached or too far) picks a point within
//! 16 blocks on each axis and wants to go there; its move control (`locomotion/fly.rs`, control `Float`) pushes it. The goal asks again by
//! EVENT: a system of the module looks at the wanted spot each tick of the ghast's flight (a flier does not sleep) and wakes the goal when
//! the spot is reached, too far, or dropped.
use super::*;
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {}
impl Params {
    pub const DEFAULT: Self = Self {};
}

pub static MODULE: Module = Module {
    name: "float_around",
    flags: MOVE,
    arm,
    can_use,
    start,
    tick: Some(watch),
    requires: &[Cap::Locomotion],
    ..Module::DEFAULT
};
fn arm(w: &mut World, e: Entity, inst: Inst) {
    crate::goals::arm(w, e, inst.slot, 1);
}
/// `!hasWanted() || distance to it outside 1..=3600 (squared)`.
fn wants_a_spot(m: &MoveTo, p: horizon_core::geometry::Vec3) -> bool {
    if !m.active {
        return true;
    }
    let d = (m.x - p.x).powi(2) + (m.y - p.y).powi(2) + (m.z - p.z).powi(2);
    !(1.0..=3600.0).contains(&d)
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let (Some(m), Some(p)) = (w.get::<MoveTo>(e).copied(), w.get::<Pos>(e).map(|p| p.0)) else { return false };
    wants_a_spot(&m, p)
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let p = w.get::<Pos>(e).unwrap().0;
    let t = with_rng(w, e, |r| {
        let mut f = || f64::from(r.next_float() * 2.0 - 1.0) * 16.0;
        [p.x + f(), p.y + f(), p.z + f()]
    });
    *w.get_mut::<MoveTo>(e).unwrap() = MoveTo { active: true, x: t[0], y: t[1], z: t[2], speed_modifier: 1.0 };
}
/// The ghast is asked again when its wanted spot is gone (the tick of the module: only the animals with the float control, read through a kept query).
fn watch(w: &mut World) {
    let es = crate::goals::entities::<With<crate::locomotion::fly::FloatState>>(w);
    for e in es {
        let (Some(m), Some(p), Some(slot)) = (w.get::<MoveTo>(e).copied(), w.get::<Pos>(e).map(|p| p.0), w.get::<GoalSet>(e).and_then(|g| g.slot_of(&MODULE))) else { continue };
        if wants_a_spot(&m, p) {
            w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
        }
    }
}
