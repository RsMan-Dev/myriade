//! The thrown eye of ender in flight (`EyeOfEnder.tick` and `updateDeltaMovement`, jar `projectile.EyeOfEnder`): the first two ticks
//! worked out by hand from the vanilla formulas (target 70 blocks up, so the eye always climbs), then its shatter on the 81st tick.
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::eye;

#[test]
fn an_eye_flies_towards_its_target_and_shatters_on_its_81st_tick() {
    let mut h = Herd::with_slab_awake(16, true, false);
    let e = eye::launch(&mut h.world, Vec3::new(0.5, 64.0, 0.5), Vec3::new(0.5, 70.0, 100.5), true);

    h.tick();
    assert_eq!(h.world.get::<Pos>(e).unwrap().0, Vec3::new(0.5, 64.0, 0.5), "tick 1 moves by a nil velocity");
    let v = h.world.get::<Vel>(e).unwrap().0;
    assert!((v.x).abs() < 1e-12 && (v.y - 0.015).abs() < 1e-12 && (v.z - 0.25).abs() < 1e-12, "tick 1 velocity {v:?}");

    h.tick();
    let (p, v) = (h.world.get::<Pos>(e).unwrap().0, h.world.get::<Vel>(e).unwrap().0);
    assert!((p.y - 64.015).abs() < 1e-9 && (p.z - 0.75).abs() < 1e-9, "tick 2 position {p:?}");
    assert!((v.y - 0.029_775).abs() < 1e-9 && (v.z - 0.498_75).abs() < 1e-9, "tick 2 velocity {v:?}");

    for _ in 3..=80 {
        h.tick();
    }
    assert!(h.world.get_entity(e).is_ok(), "the eye is still flying on its 80th tick");
    h.tick();
    assert!(h.world.get_entity(e).is_err(), "the eye shatters on its 81st tick");
}
