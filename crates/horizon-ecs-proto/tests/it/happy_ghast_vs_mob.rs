//! A dry happy ghast heals one health every 600 ticks (`happy_ghast.rs` `ai_step`), up to its maximum of 20, in the same scene as
//! the rabbit test's herd (`HORIZON_SPECIES=minecraft:happy_ghast`, `HORIZON_TEST_RESOURCES` set).
//! NON FAIT : la comparaison avec le `Mob` vanille (le soin passe par `ai_step` de la classe de test, hors de portée d'ici) ; le test
//! de pilotage en vol (`fly_steer`, il faut construire un `Mount`) reste à écrire.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_ecs_proto::cow::*;

fn is(name: &str) -> bool {
    have_data() && !wrong_species(&[name])
}

/// Dry, the ghast regains one health at 600 ticks, and not before; it stops at its maximum of 20.
#[test]
fn a_dry_happy_ghast_heals_one_health_every_600_ticks() {
    if horizon_ecs_proto::cow::skip_unless_small_body("a_dry_happy_ghast_heals_one_health_every_600_ticks") {
        return;
    }
    if !is("minecraft:happy_ghast") {
        return;
    }
    let mut h = Herd::with_slab(60, true, false);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 7);
    h.world.get_mut::<Health>(e).unwrap().0 = 10.0;

    for _ in 0..599 {
        h.tick();
    }
    let before = h.world.get::<Health>(e).unwrap().0;
    assert_eq!(before, 10.0, "healed before 600 ticks: {before}");

    for _ in 0..2 {
        h.tick();
    }
    let after = h.world.get::<Health>(e).unwrap().0;
    eprintln!("dry heal: health {before} -> {after}");
    assert_eq!(after, 11.0, "one heal expected at tick 600, got {after}");
}

/// A ghast with a player on it holds still: its speed is zero while the player stays (`still_under_riders`, `Above`).
#[test]
fn a_happy_ghast_with_a_player_on_it_holds_still() {
    if horizon_ecs_proto::cow::skip_unless_small_body("a_happy_ghast_with_a_player_on_it_holds_still") {
        return;
    }
    if !is("minecraft:happy_ghast") {
        return;
    }
    let mut h = Herd::with_slab(60, true, false);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 7);
    h.world.entity_mut(e).insert(horizon_ecs_proto::behaviours::still_under_riders::Above);
    h.world.get_mut::<Vel>(e).unwrap().0 = horizon_core::geometry::Vec3::new(0.2, 0.0, 0.0);
    for _ in 0..20 {
        h.tick();
    }
    let v = h.world.get::<Vel>(e).unwrap().0;
    eprintln!("ridden: speed {:.4}", v.x.hypot(v.z));
    assert!(v.x.hypot(v.z) < 1e-6, "a ghast with a rider moved at {v:?}");
}
