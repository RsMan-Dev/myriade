//! `FloatGoal`: an animal in a fluid deeper than its threshold, or in lava, jumps with 80 % each tick.
use super::*;
use crate::behaviours::still_under_riders::{Above, Leaving, StillUntil};
use crate::{cow::*, goals::*};
use crate::rules::float::{buoyancy, is_deep, jumps};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `HappyGhastFloatGoal.canUse`: no float while the still timeout holds (a rider on it, or its countdown).
    pub unless_still: bool,
}
impl Params {
    pub const DEFAULT: Self = Self { unless_still: false };
}
/// `Mob.getFluidJumpThreshold`: the depth of fluid (in blocks) from which the animal floats.
#[derive(Component, Clone, Copy, Debug)]
pub struct Buoyancy(pub f64);
/// The float waits for the still timeout (`Params::unless_still`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct UnlessStill;
/// The goal runs (`float_run` visits it).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Floating;

pub static MODULE: Module = Module {
    name: "float",
    flags: JUMP,
    equip,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let eye = w.get::<FluidParams>(e).map_or(0.0, |p| p.eye_height);
    w.entity_mut(e).insert(Buoyancy(buoyancy(eye)));
    if params::<Params>(p).unless_still {
        w.entity_mut(e).insert(UnlessStill);
    }
}
fn floats(w: &World, e: Entity) -> bool {
    let (f, b) = (w.get::<FluidState>(e).unwrap(), w.get::<Buoyancy>(e).unwrap());
    is_deep(f.water, b.0, f.in_lava)
}
/// `canUse`: floats, unless the still timeout holds (`still_under_riders`: a rider, or the countdown after one left).
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let still = w.get::<Above>(e).is_some() || w.get::<Leaving>(e).is_some() || w.get::<StillUntil>(e).is_some_and(|u| now_of(w, e) < u.0);
    floats(w, e) && !(w.get::<UnlessStill>(e).is_some() && still)
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Floating);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Floating>();
    w.get_mut::<Jump>(e).unwrap().0 = false;
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((float_events, float_run).chain().in_set(slot));
}
/// A cow in a fluid (the fluid module visits only these) that gets deeper than the threshold, or in lava, starts
/// floating; one that gets shallower, or leaves the fluid, stops.
#[allow(clippy::type_complexity)]
fn float_events(
    q: Query<(Entity, &FluidState, &Buoyancy, &GoalSet, Has<Floating>), With<InFluid>>,
    left: Query<(Entity, &GoalSet), (With<Floating>, Without<InFluid>)>,
    mut reqs: ResMut<Requests>,
) {
    for (e, f, b, g, floating) in &q {
        let Some(slot) = g.slot_of(&MODULE) else { continue };
        let deep = is_deep(f.water, b.0, f.in_lava);
        if deep != floating {
            reqs.0.push(if deep { Req::Start(e, slot) } else { Req::End(e, slot) });
        }
    }
    for (e, g) in &left {
        if let Some(slot) = g.slot_of(&MODULE) {
            reqs.0.push(Req::End(e, slot));
        }
    }
}
/// `FloatGoal.tick`: a jump with 80 % each tick.
fn float_run(mut q: Query<(&mut Rng, &mut Jump), With<Floating>>) {
    for (mut r, mut j) in &mut q {
        j.0 = jumps(r.next_u32());
    }
}
