//! The dragon's breath as a sleeping area cloud (`AreaEffectCloud.serverTick` and `WAIT_TIME`, jar `AreaEffectCloud`): inert for 20
//! ticks, then its radius grows by `(grown - radius) / duration` each tick from its 20th, and it goes at `WAIT + duration`.
use bevy_ecs::prelude::*;
use horizon_ecs_proto::breath_cloud::{self, Live};
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::dragon::breath::{Cloud, CloudSpec, WAIT};

#[test]
fn a_breath_sleeps_for_its_wait_then_grows_and_goes_at_its_end() {
    let mut h = Herd::with_slab(16, true, false);
    let spec = CloudSpec { radius: 5.0, grown: 8.0, duration: 200, amplifier: 0 };
    let e = breath_cloud::spawn(&mut h.world, spec, [0.5, 64.0, 0.5]);

    for _ in 0..(WAIT - 2) {
        h.tick();
    }
    assert!(h.world.get::<Live>(e).is_none(), "inert during its wait");
    assert_eq!(h.world.get::<Cloud>(e).unwrap().radius, 5.0);

    for _ in 0..22 {
        h.tick();
    }
    // Ticks 20 to 40 grow it: 21 steps of 3 / 200.
    let r = h.world.get::<Cloud>(e).unwrap().radius;
    assert!((r - (5.0 + 21.0 * 0.015)).abs() < 1e-4, "radius {r}");

    for _ in 0..179 {
        h.tick();
    }
    assert!(h.world.get_entity(e).is_ok(), "alive on tick 219");
    h.tick();
    assert!(h.world.get_entity(e).is_err(), "gone at WAIT + duration, tick 220");
}
