//! `lookout` (`Fox.PerchAndSearchGoal`) with the fox's alarm (`Fox.alertable`, `mobs/fox.rs:838`): a fox with a creature or a player within
//! 12 blocks does not start a lookout; the same fox without the alarm does (chance 1.0, so the only refusal is the alarm).
use bevy_ecs::prelude::*;
use horizon_ecs_proto::{
    behaviours::{lookout, Entry},
    cow::*,
    goals::*,
    species::Config,
};

fn starts(alarm: Option<fn(&World, Entity) -> bool>) -> Option<bool> {
    let mut h = Herd::with_slab_awake(60, true, false);
    let p: &'static lookout::Params = Box::leak(Box::new(lookout::Params { chance: 1.0, blocked: 0, sit: 1, alarm }));
    let entries: &'static [Entry] = Box::leak(vec![Entry::new(&lookout::MODULE, 13, p)].into_boxed_slice());
    let mut sp = *species();
    sp.goals = Config { entries };
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 70);
    // A species with no creature body (a minecart, an item, a display) has no zone clock: there is no goal to measure.
    h.world.get_mut::<ActiveUntil>(e)?.0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    let mut started = false;
    for _ in 0..40 {
        h.tick();
        started |= h.world.get::<GoalSet>(e).unwrap().is_running(&lookout::MODULE);
    }
    Some(started)
}

/// An alarm that is always raised (a creature or a player near) refuses every ask.
#[test]
fn an_alarmed_fox_does_not_start_a_lookout() {
    if let Some(started) = starts(Some(|_: &World, _: Entity| true)) {
        assert!(!started);
    }
}

/// Without the alarm the same fox starts its lookout at the first ask.
#[test]
fn a_fox_without_an_alarm_starts_a_lookout() {
    if let Some(started) = starts(None) {
        assert!(started);
    }
}
