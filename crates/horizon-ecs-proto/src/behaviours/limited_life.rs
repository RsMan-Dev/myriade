//! `Vex.limitedLifeTicks`: a creature with a life span (set by whoever summons it) takes 1 magic damage a tick once the span is over. A
//! standalone component, `Lifespan(until)`, in the creature's own time; the module only ticks the component, it has no goal (`can_use` never).
use super::*;
use crate::cow::*;

/// The tick (the world's) from which the creature is hurt each tick.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Lifespan(pub u64);

pub static MODULE: Module = Module { name: "limited_life", flags: 0, can_use: never, tick: Some(tick), ..Module::DEFAULT };
fn never(_: &mut World, _: Entity, _: Inst) -> bool {
    false
}
fn tick(w: &mut World) {
    let now = w.resource::<Clock>().0;
    let late: Vec<(Entity, Option<(f64, f64)>)> = w.query::<(Entity, &Lifespan, &Health, &Pos)>().iter(w).filter(|(_, l, h, _)| l.0 <= now && h.0 > 0.0).map(|(e, _, _, p)| (e, Some((p.0.x, p.0.z)))).collect();
    let mut q = w.resource_mut::<DamageQueue>();
    for (e, from) in late {
        q.0.push(Hit { target: e, amount: 1.0, cause: Cause::Magic, damage: "starve", from, by: None, direct: false, power: None, projectile: None });
    }
}
