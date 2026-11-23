//! `IronGolem.hurtServer` (`Crackiness.GOLEM`): a hit that leaves the creature in a new crack level plays its damage sound once.
//! The levels are the health fractions under which a crack shows (0.75, 0.5, 0.25, from the rule's parameters); a hit inside one
//! level plays nothing. Queued by `cow/health.rs` after each landed hit, with the health before and after it.
use super::*;
use crate::cow::*;
use crate::goals::Events;
use crate::rules::{no_attrs, no_interact, Rule};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub sound: &'static str,
    pub levels: [f32; 3],
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Cracks {
    sound: &'static str,
    levels: [f32; 3],
}
pub static RULE: Rule = Rule { name: "crack_sound", equip, attrs: no_attrs, interact: no_interact };
fn equip(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync)) {
    let p = crate::rules::params::<Params>(p);
    w.entity_mut(e).insert(Cracks { sound: p.sound, levels: p.levels });
}
/// The crack level of a health fraction: how many of the thresholds it is under.
fn level(c: &Cracks, fraction: f32) -> usize {
    c.levels.iter().filter(|&&t| fraction < t).count()
}
/// A hit took the health from `before` to `after`: the sound plays when the crack level changed.
pub fn hurt(w: &mut World, e: Entity, before: f32, after: f32) {
    let (Some(c), Some(at)) = (w.get::<Cracks>(e).copied(), w.get::<Pos>(e).map(|p| p.0)) else {
        return;
    };
    // `getMaxHealth`: the component when there is one, else the species' (`cow::max_health`).
    let max = crate::cow::max_health(w, e);
    if max <= 0.0 || level(&c, before / max) == level(&c, after / max) {
        return;
    }
    w.resource_mut::<Events>().host.sound(c.sound.to_string(), [at.x, at.y, at.z], 1.0, 1.0);
}
