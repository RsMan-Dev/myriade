//! `timed_idle` (the warden's `DoNothing(Timed::new(30, 60))`, `mobs/warden_brain.rs:318`): a stay that starts holds the goal for its drawn
//! time, 30 to 60 ticks (one tick of grain at each end), and the stay ends by itself.
use bevy_ecs::prelude::*;
use horizon_ecs_proto::{
    behaviours::{timed_idle, Entry},
    cow::*,
    goals::*,
    species::Config,
};

#[test]
fn a_pause_lasts_between_its_min_and_max() {
    let mut h = Herd::with_slab_awake(60, true, false);
    let p: &'static timed_idle::Params = Box::leak(Box::new(timed_idle::Params { min: 30, max: 60, chance: 1.0 }));
    let entries: &'static [Entry] = Box::leak(vec![Entry::new(&timed_idle::MODULE, 10, p)].into_boxed_slice());
    let mut sp = *species();
    sp.goals = Config { entries };
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 70);
    // A species with no creature body (a minecart, an item, a display) has no zone clock: there is no goal to measure.
    let Some(mut active) = h.world.get_mut::<ActiveUntil>(e) else { return };
    active.0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    let (mut from, mut len) = (None, None);
    for t in 0..3000u32 {
        h.tick();
        let running = h.world.get::<GoalSet>(e).unwrap().is_running(&timed_idle::MODULE);
        match (from, running) {
            (None, true) => from = Some(t),
            (Some(s), false) => {
                len = Some(t - s);
                break;
            }
            _ => {}
        }
    }
    let len = len.expect("the pause started and ended within 3000 ticks");
    assert!((30..=61).contains(&len), "stay of {len} ticks");
}
