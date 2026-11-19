//! `SetEntityLookTarget` sink (`LookAtTargetSink`): looks at the look target while it is visible, for its duration.
use crate::brain::*;
use crate::cow::*;

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:look_at_target_sink", mems: &[&LOOK_TARGET, &VISIBLE_MOBS], tick, stop, ..Behaviour::DEFAULT };
pub fn tracker_visible(w: &mut World, e: Entity, t: Tracker) -> bool {
    match t {
        Tracker::Entity(who, _) => crate::targeting::info(w, who).is_some() && visible_contains(w, e, who),
        _ => true,
    }
}
fn tick(w: &mut World, e: Entity, _: &Ctx) -> bool {
    let Some(look) = look_target(w, e) else { return false };
    if !tracker_visible(w, e, look) {
        return false;
    }
    if let Some(at) = look.position(w) {
        look_at(w, e, at);
    }
    true
}
fn stop(w: &mut World, e: Entity, _: &Ctx) {
    erase(w, e, &LOOK_TARGET);
}
