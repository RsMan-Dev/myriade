//! `WitherDoNothingGoal` (fiche 24, `mobs/wither.rs:84-95`): while the invulnerability counter of the charge is above zero,
//! the goal holds MOVE, JUMP and LOOK at its priority. The counter's rule (`R:wither`) is not ported: it calls `set`.
use super::*;
use crate::goals::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {}
impl Params {
    pub const DEFAULT: Self = Self {};
}
/// `extra.counter` of the charge: above zero, the goal runs.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Invulnerable(pub i32);

pub static MODULE: Module = Module {
    name: "do_nothing",
    flags: MOVE | LOOK | JUMP,
    equip,
    can_use,
    requires: &[Cap::Goals],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, _: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Invulnerable(0));
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Invulnerable>(e).is_some_and(|i| i.0 > 0)
}
/// Sets the counter: above zero asks the goal to start (on the selector's ticks); zero asks it to end.
pub fn set(w: &mut World, e: Entity, ticks: i32) {
    w.entity_mut(e).insert(Invulnerable(ticks));
    if ticks > 0 {
        wake(w, e, &MODULE);
    } else {
        end(w, e, &MODULE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::species::Config;
    use crate::{cow::{ActiveUntil, Herd, SpeciesTable, spawn_species}, goals::set_lazy_look};

    /// A goal of the probe kind: it wants the look flag and no more; without the charge's goal it would start at once.
    static PROBE: Module = Module { name: "probe_look", flags: LOOK, ..Module::DEFAULT };

    fn guard(test: &str) -> bool {
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            crate::cow::skip_note(test, "HORIZON_TEST_RESOURCES is not set");
            return true;
        }
        false
    }
    fn herd() -> (Herd, Entity) {
        let mut h = Herd::with_slab(20, true, false);
        set_lazy_look(&mut h.world, false);
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
        let entries = vec![Entry::new(&MODULE, 0, &Params::DEFAULT), Entry::new(&PROBE, 1, &())];
        sp.goals = Config { entries: Box::leak(entries.into_boxed_slice()) };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        for _ in 0..10 {
            h.tick();
        }
        (h, e)
    }
    fn running(h: &Herd, e: Entity, m: &'static Module) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(m)
    }

    #[test]
    fn the_charge_holds_the_flags_and_the_lower_goals_wait() {
        if guard("the_charge_holds_the_flags_and_the_lower_goals_wait") {
            return;
        }
        let (mut h, e) = herd();
        wake(&mut h.world, e, &PROBE);
        h.tick();
        h.tick();
        assert!(running(&h, e, &PROBE), "witness: without the charge the lower goal runs");
        set(&mut h.world, e, 40);
        for _ in 0..6 {
            h.tick();
        }
        assert!(running(&h, e, &MODULE), "a counter above zero: the goal runs");
        assert!(!running(&h, e, &PROBE), "and it takes the look flag from the lower goal");
        for _ in 0..30 {
            h.tick();
            assert!(running(&h, e, &MODULE) && !running(&h, e, &PROBE), "the charge keeps the flags");
        }
    }

    #[test]
    fn at_zero_the_charge_ends_and_the_next_goal_starts() {
        if guard("at_zero_the_charge_ends_and_the_next_goal_starts") {
            return;
        }
        let (mut h, e) = herd();
        set(&mut h.world, e, 40);
        for _ in 0..6 {
            h.tick();
        }
        assert!(running(&h, e, &MODULE) && !running(&h, e, &PROBE), "held");
        set(&mut h.world, e, 0);
        wake(&mut h.world, e, &PROBE);
        for _ in 0..6 {
            h.tick();
        }
        assert!(!running(&h, e, &MODULE), "counter at zero: the goal ends");
        assert!(running(&h, e, &PROBE), "witness: the lower goal starts once the flags are free");
    }
}
