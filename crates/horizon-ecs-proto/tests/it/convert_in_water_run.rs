//! The zombie's conversion in water, tick by tick, against the counts of the jar's `ConversionTracker.tick` (`afflictionTime` +1 a tick with the eyes
//! under, the conversion starts when it reaches 600, `conversionTime` 300 goes down a tick and converts at < 0: tick 600 starts, tick 901 converts).
use horizon_ecs_proto::{
    behaviours::{convert_in_water::Soaking, convert_timer::Converting},
    cow::*,
    goals::Events,
    host_out::HostEvent,
};

fn zombie_with_eyes_under() -> (Herd, bevy_ecs::entity::Entity, u64) {
    let mut h = Herd::with_slab_awake(60, true, false);
    h.set_daylight(false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:zombie").expect("the zombie");
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 61);
    h.world.entity_mut(e).insert(FluidState { in_water: true, eye_in_water: true, ..Default::default() });
    // The change is the one `fluid_update` pushes during the tick `t0`; the goal reads it at `t0 + 1`.
    let t0 = now_of(&h.world, e);
    h.world.resource_mut::<Events>().eye_entered_water.push(e);
    (h, e, t0)
}
fn run_to(h: &mut Herd, e: bevy_ecs::entity::Entity, tick: u64) {
    while now_of(&h.world, e) < tick {
        h.tick();
    }
}
fn converted(h: &Herd, e: bevy_ecs::entity::Entity) -> bool {
    h.world.get_entity(e).is_err()
}

#[test]
fn the_conversion_starts_at_the_600th_tick_and_ends_301_ticks_later() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let (mut h, e, t0) = zombie_with_eyes_under();
    // The tick `t0` is the first with the eyes under (afflictionTime 1): the 600th is `t0 + 599`.
    run_to(&mut h, e, t0 + 598);
    assert!(h.world.get::<Soaking>(e).is_some_and(|s| !s.converting), "the count runs, the conversion has not begun");
    run_to(&mut h, e, t0 + 599);
    assert!(h.world.get::<Soaking>(e).is_some_and(|s| s.converting), "afflictionTime reached 600");
    assert!(h.world.get::<Converting>(e).is_some(), "the shake is synced");
    h.drain_host_out();
    // conversionTime 300 at the start tick, 0 at start + 300, -1 (the conversion) at start + 301.
    run_to(&mut h, e, t0 + 599 + 300);
    assert!(!converted(&h, e), "conversionTime is 0, not yet below");
    run_to(&mut h, e, t0 + 599 + 301);
    assert!(converted(&h, e), "conversionTime went below 0");
    let sounded = h.drain_host_out().into_iter().any(|ev| matches!(ev, HostEvent::LevelEvent { event: 1040, .. }));
    assert!(sounded, "the zombie's conversion event is 1040");
}

#[test]
fn the_eyes_out_of_the_water_cancel_the_count() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let (mut h, e, t0) = zombie_with_eyes_under();
    run_to(&mut h, e, t0 + 700);
    assert!(h.world.get::<Soaking>(e).is_some_and(|s| s.converting));
    h.world.entity_mut(e).insert(FluidState::default());
    h.world.resource_mut::<Events>().eye_left_water.push(e);
    h.tick();
    h.tick();
    assert!(h.world.get::<Soaking>(e).is_none(), "the count is gone");
    assert!(h.world.get::<Converting>(e).is_none(), "the shake stopped");
    run_to(&mut h, e, t0 + 1300);
    assert!(!converted(&h, e), "a cancelled conversion never ends");
}
