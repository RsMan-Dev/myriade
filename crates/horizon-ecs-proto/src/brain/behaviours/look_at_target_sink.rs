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
    // One read of the target's components serves both the existence test and the point looked at.
    let at = match look {
        Tracker::Entity(who, eye) => {
            let Some(i) = crate::targeting::info(w, who) else { return false };
            if !visible_contains(w, e, who) {
                return false;
            }
            Some([i.pos.x, if eye { i.eye } else { i.pos.y }, i.pos.z])
        }
        _ => look.position(w),
    };
    if let Some(at) = at {
        look_at(w, e, at);
    }
    true
}
fn stop(w: &mut World, e: Entity, _: &Ctx) {
    erase(w, e, &LOOK_TARGET);
}
