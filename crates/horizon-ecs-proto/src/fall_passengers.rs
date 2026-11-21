//! `Entity.propagateFallToPassengers`: a mount that lands passes its fall (distance and modifier) to what rides it, and each rider takes
//! its own `calculateFallDamage` as a hit of cause `Fall`. Only a landing that hurt reaches here, so nothing runs at rest.
use crate::cow::{Cause, DamageQueue};
use crate::mount::Mount;
use crate::targeting::Who;
use crate::type_tags;
use bevy_ecs::prelude::*;

/// `AbstractHorse.causeFallDamage`: a horse (a mount with a temper) that lands after more than a block of fall plays the land sound at 0.4,
/// the baby's when it has not grown up (`GrowAt`).
pub fn land_sound(w: &mut World, e: Entity, distance: f64) {
    if distance <= 1.0 || !w.get::<Mount>(e).is_some_and(|m| m.max_temper > 0) {
        return;
    }
    let Some(p) = w.get::<crate::cow::Pos>(e).map(|p| p.0) else { return };
    let name = if w.get::<crate::cow::GrowAt>(e).is_some() { "minecraft:entity.baby_horse.land" } else { "minecraft:entity.horse.land" };
    w.resource_mut::<crate::goals::Events>().host.sound(name, [p.x, p.y, p.z], 0.4, 1.0);
}

/// `Entity.causeFallDamage` of a rider tagged `fall_damage_immune` returns at once; the others take the hit.
pub fn propagate(w: &mut World, mount: Entity, distance: f64, modifier: f32) {
    let Some(m) = w.get::<Mount>(mount).copied() else { return };
    for who in [m.rider, m.passenger].into_iter().flatten() {
        let Who::Entity(r) = who else { continue };
        if type_tags::named(w, "fall_damage_immune", r) {
            continue;
        }
        let dmg = crate::inside_effects::fall_damage(distance, f64::from(modifier));
        if dmg > 0.0 {
            w.resource_mut::<DamageQueue>().add(r, dmg, Cause::Fall);
        }
    }
}
