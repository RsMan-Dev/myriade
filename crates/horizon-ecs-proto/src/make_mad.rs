//! `AbstractHorse.makeMad`: a mount that refuses its rider rears (when its species rears: it has the `Stand` cycle) and makes its angry
//! sound. The rule `angry` gives the sound of the species (`getAngrySound`; a species with none is silent and takes no rule). Nothing ticks.
use crate::behaviours::random_stand::{Stand, Standing, STAND_TICKS};
use crate::cow::{now_of, Bounds, GrowAt};
use crate::rules::{no_attrs, no_interact, params, Rule};
use bevy_ecs::prelude::*;

/// The sound of an adult and of a baby (the same for species with one voice).
#[derive(Clone, Copy, Debug)]
pub struct Angry {
    pub adult: &'static str,
    pub baby: &'static str,
}
impl Angry {
    pub const HORSE: Self = Self { adult: "minecraft:entity.horse.angry", baby: "minecraft:entity.baby_horse.angry" };
    pub const DONKEY: Self = Self { adult: "minecraft:entity.donkey.angry", baby: "minecraft:entity.donkey.angry" };
    pub const MULE: Self = Self { adult: "minecraft:entity.mule.angry", baby: "minecraft:entity.mule.angry" };
    pub const ZOMBIE_HORSE: Self = Self { adult: "minecraft:entity.zombie_horse.angry", baby: "minecraft:entity.zombie_horse.angry" };
    pub const LLAMA: Self = Self { adult: "minecraft:entity.llama.angry", baby: "minecraft:entity.llama.angry" };
}

#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct CanBeAngry(pub &'static Angry);

/// The rule `angry`, parameter: an `Angry`.
pub static ANGRY: Rule = Rule {
    name: "angry",
    equip: |w, e, p| {
        w.entity_mut(e).insert(CanBeAngry(params::<Angry>(p)));
    },
    attrs: no_attrs,
    interact: no_interact,
};

/// `makeMad`: no effect while the mount already rears; else it rears for 20 ticks (`standIfPossible`) and says its sound.
pub fn make_mad(w: &mut World, e: Entity) {
    let now = now_of(w, e);
    if w.get::<Standing>(e).is_some_and(|s| now < s.until) {
        return;
    }
    if w.get::<Stand>(e).is_some() {
        w.entity_mut(e).insert(Standing { until: now + STAND_TICKS });
    }
    let Some(a) = w.get::<CanBeAngry>(e).map(|a| a.0) else { return };
    let sound = if w.get::<GrowAt>(e).is_some() { a.baby } else { a.adult };
    let Some(b) = w.get::<Bounds>(e).map(|b| b.0) else { return };
    let c = b.bottom_center();
    let at = [c.x, c.y + (b.max_position().y - b.min_position().y) / 2.0, c.z];
    w.resource_mut::<crate::goals::Events>().host.sound(sound, at, 1.0, 1.0);
}
