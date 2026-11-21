//! `Fox.FaceplantGoal` (fiche 21): the fox stays with its nose in the snow for 20 selector ticks, that is 40 game ticks.
//! It starts when `Pounce` sets FACEPLANTED (a wake), ends at its deadline and clears the flag when it stops.
use super::fox_state::{self as fs, FACEPLANTED};
use crate::rules::fox_faceplant::game_ticks;
use super::*;
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The countdown, in selector ticks (`reduced(40)`); a selector tick is two game ticks.
    pub ticks: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { ticks: 20 };
}

/// The countdown in selector ticks, kept from the creation (`equip`).
#[derive(Component, Clone, Copy, Debug)]
pub struct FaceplantTicks(pub i32);
/// The local time the goal ends at.
#[derive(Component, Clone, Copy, Debug)]
pub struct FaceplantEnd(pub u64);

pub static MODULE: Module = Module {
    name: "fox_faceplant",
    flags: MOVE | LOOK | JUMP,
    equip,
    can_use,
    start,
    stop,
    fired,
    requires: &[Cap::Goals],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    fs::ensure(w, e);
    w.entity_mut(e).insert(FaceplantTicks(params::<Params>(p).ticks));
}

fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    fs::has(w, e, FACEPLANTED)
}

fn start(w: &mut World, e: Entity, inst: Inst) {
    let wait = game_ticks(w.get::<FaceplantTicks>(e).unwrap().0);
    let end = now_of(w, e) + wait as u64;
    w.entity_mut(e).insert(FaceplantEnd(end));
    crate::goals::arm(w, e, inst.slot, wait);
}

/// A deadline of this goal: while it runs it is the end of the countdown (it ends now; a deadline of an earlier run is
/// older than the end of this one and is ignored); when it does not run it is the framework's retry of a start request
/// that came on a tick that was not the selector's (`Req::Start` re-arms the goal for the next tick): ask `can_use`.
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    match w.get::<FaceplantEnd>(e).map(|f| f.0) {
        Some(end) => {
            if now_of(w, e) >= end {
                crate::goals::end_goal(w, e, inst.slot);
            }
            false
        }
        None => true,
    }
}

fn stop(w: &mut World, e: Entity, _: Inst) {
    fs::set(w, e, FACEPLANTED, false);
    w.entity_mut(e).remove::<FaceplantEnd>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Config;

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 1, &Params::DEFAULT)];

    fn herd() -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(30, false, false);
        set_lazy_look(&mut h.world, false);
        Some(h)
    }
    /// A cow body whose only goal is the faceplant.
    fn fox(h: &mut Herd) -> Entity {
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.name = "minecraft:fox";
        sp.goals = Config { entries: &ENTRIES };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
        h.world.flush();
        e
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }

    /// A fox with its nose in the snow runs the faceplant for 40 game ticks from its start, and no longer after; with the
    /// flag down, the wake starts nothing (the control of the same test).
    #[test]
    fn faceplant_lasts_forty_ticks_only_while_the_flag_is_set() {
        let Some(mut h) = herd() else { return };
        let e = fox(&mut h);
        h.tick();
        fs::set(&mut h.world, e, 0, false);
        wake(&mut h.world, e, &MODULE);
        for _ in 0..4 {
            h.tick();
        }
        assert!(!running(&h, e), "the flag is down: no start");
        fs::set(&mut h.world, e, FACEPLANTED, true);
        wake(&mut h.world, e, &MODULE);
        let mut start = None;
        for _ in 0..4 {
            h.tick();
            if running(&h, e) && start.is_none() {
                start = Some(now_of(&h.world, e));
            }
        }
        let start = start.expect("the flag is up: the faceplant starts");
        while now_of(&h.world, e) < start + 39 {
            h.tick();
            assert!(running(&h, e), "still faceplanted at +{}", now_of(&h.world, e) - start);
        }
        while now_of(&h.world, e) < start + 41 {
            h.tick();
        }
        assert!(!running(&h, e), "over after 40 game ticks");
        assert!(!fs::has(&h.world, e, FACEPLANTED), "the flag is cleared when it stops");
    }
}
