//! Règle de l'effet de chute lente : `LivingEntity.getEffectiveGravity`, la gravité d'un corps qui tombe (vitesse verticale ≤ 0) est plafonnée
//! à 0,01 ; un corps sans l'effet garde sa gravité de 0,08. Valeurs du jar : `(0 - g) * 0,98` après un tick de chute depuis le repos.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::effects::{self, Kind};

fn falling_body(h: &mut Herd, slow: bool) -> Entity {
    let p = Vec3::new(0.5, 10.0, 0.5);
    let e = h
        .world
        .spawn((
            Pos(p),
            Vel(Vec3::ZERO),
            Body { width: 0.6, height: 1.8, step_height: 0.6, speed: 0.0, fall_factor: 1.0, safe_fall: 3.0, fall_mult: 1.0, movement_efficiency: 0.0, object: false, orb: false, tnt: false, lava_walker: false },
            Gravity(0.08),
            Bounds(box_at(p, 0.6, 1.8)),
            Cell(u32::MAX, [i32::MIN; 8], u32::MAX),
            Contact { on_ground: false, horizontal: false, fall_distance: 0.0, stuck: 0 },
            FluidState::default(),
            Drive::default(),
            Moving,
            Health(20.0),
            HurtAt { at: 0, last: 0.0 },
        ))
        .id();
    enter_index(&mut h.world, e);
    if slow {
        assert!(effects::add(&mut h.world, e, Kind::SlowFalling, 0, 600));
    }
    e
}

#[test]
fn a_falling_body_without_slow_falling_takes_the_full_gravity_of_its_type() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    h.tick();
    let e = falling_body(&mut h, false);
    h.tick();
    let vy = h.world.get::<Vel>(e).expect("a velocity").0.y;
    assert!((vy - (0.0 - 0.08) * 0.98).abs() < 1e-6, "vertical speed {vy}");
}

#[test]
fn a_falling_body_with_slow_falling_has_its_gravity_capped_at_one_hundredth() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    h.tick();
    let e = falling_body(&mut h, true);
    h.tick();
    let vy = h.world.get::<Vel>(e).expect("a velocity").0.y;
    assert!((vy - (0.0 - 0.01) * 0.98).abs() < 1e-6, "vertical speed {vy}");
}
