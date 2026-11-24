//! `Vex.limitedLifeTicks`: a creature with a life span (set by whoever summons it) takes 1 magic damage a tick once the span is over. A
//! standalone component, `Lifespan(until)`, in the creature's own time; the module only ticks the component, it has no goal (`can_use` never).
use super::*;
use crate::cow::*;

/// The tick (the world's) from which the creature is hurt each tick.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Lifespan(pub u64);

/// `Vex.setLimitedLife`: the span counted from now, in ticks.
pub fn set_limited_life(w: &mut World, e: Entity, ticks: u64) {
    let now = w.resource::<Clock>().0;
    w.entity_mut(e).insert(Lifespan(now + ticks));
}

pub static MODULE: Module = Module { name: "limited_life", flags: 0, can_use: never, tick: Some(tick), ..Module::DEFAULT };
fn never(_: &mut World, _: Entity, _: Inst) -> bool {
    false
}
fn tick(w: &mut World) {
    let now = w.resource::<Clock>().0;
    let late: Vec<(Entity, Option<(f64, f64)>)> = crate::queries::with::<(Entity, &'static Lifespan, &'static Health, &'static Pos), (), _>(w, |q, w| q.iter(w).filter(|(_, l, h, _)| l.0 <= now && h.0 > 0.0).map(|(e, _, _, p)| (e, Some((p.0.x, p.0.z)))).collect());
    let mut q = w.resource_mut::<DamageQueue>();
    for (e, from) in late {
        q.0.push(Hit { target: e, amount: 1.0, cause: Cause::Magic, damage: "starve", from, by: None, direct: false, power: None, extra: None, projectile: None });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{species, spawn_species, Herd, Species};
    use crate::species::Config;

    /// `Vex.setLimitedLife`: the span counts from the tick it is set, whatever the creature's own tick count.
    #[test]
    fn a_span_counts_from_the_tick_it_is_set() {
        if crate::cow::skip_unless_walker("a_span_counts_from_the_tick_it_is_set") {
            return;
        }
        let mut h = Herd::with_slab_awake(8, true, false);
        let mut sp: Species = *species();
        sp.goals = Config { entries: &[] };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 31);
        for _ in 0..3 {
            h.tick();
        }
        let now = h.world.resource::<Clock>().0;
        set_limited_life(&mut h.world, e, 5);
        assert_eq!(h.world.get::<Lifespan>(e).map(|l| l.0), Some(now + 5));
    }
}
