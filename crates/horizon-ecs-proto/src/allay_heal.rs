//! `Allay.aiStep`: a living allay on the server heals one point on each tenth tick of its age, with or without AI (the step runs before the
//! AI check). A deadline of the wheel (`Due::AllayHeal`) at each multiple of 10: nothing runs between two.
use crate::cow::*;
use bevy_ecs::prelude::*;

pub static HEAL: crate::rules::Rule = crate::rules::Rule { name: "allay_heal", equip, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };

fn equip(w: &mut World, e: Entity, _: &'static (dyn std::any::Any + Sync)) {
    arm(w, e);
}

/// Puts the next tenth tick of the allay's age in the wheel.
fn arm(w: &mut World, e: Entity) {
    let at = (w.resource::<Clock>().of(e) / 10 + 1) * 10;
    let when = w.resource::<Clock>().at(e, at);
    w.resource_mut::<Wheel>().push(when, e, Due::AllayHeal);
}

/// The deadline: a living allay gains a point up to its max health, and the next tenth tick is armed.
pub fn due(w: &mut World, e: Entity) {
    let max = w.get::<SpeciesId>(e).map_or(f32::MAX, |s| w.resource::<SpeciesTable>().0[s.0 as usize].max_health);
    let Some(mut h) = w.get_mut::<Health>(e) else { return };
    if h.0 <= 0.0 {
        return;
    }
    h.0 = (h.0 + 1.0).min(max);
    drop(h);
    crate::bridge::view::mark(w, e);
    arm(w, e);
}
