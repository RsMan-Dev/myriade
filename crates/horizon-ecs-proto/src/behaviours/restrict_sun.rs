//! `RestrictSunGoal`: while it is bright outside, the mob's paths avoid the sky's sight (`Nav.avoid_sun`: `GroundPathNavigation.trimPath`).
//! The goal takes no flag and does nothing else, so the module has no deadline and no system: it marks the entity (`AvoidSun`) and the
//! path finder (`cow::pathfind`) reads the mark and the daylight when it makes a path. Not modelled: the helmet that cancels the goal
//! (`head_id().is_none()`: no equipment slot for mobs yet), as in `flee_sun`.
use super::*;

#[derive(Clone, Copy, Debug)]
pub struct Params;
/// The mob avoids the sun (read by the path finder).
#[derive(Component, Clone, Copy, Debug)]
pub struct AvoidSun;

pub static MODULE: Module = Module { name: "restrict_sun", flags: 0, equip, can_use: never, requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, _: Inst, _: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(AvoidSun);
}
fn never(_: &mut World, _: Entity, _: Inst) -> bool {
    false
}
