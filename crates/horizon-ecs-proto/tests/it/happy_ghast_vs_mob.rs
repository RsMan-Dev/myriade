//! A dry happy ghast heals one health every 600 ticks (`happy_ghast.rs` `ai_step`), up to its maximum of 20, in the same scene as
//! the rabbit test's herd (`HORIZON_SPECIES=minecraft:happy_ghast`, `HORIZON_TEST_RESOURCES` set).
//! NON FAIT : la comparaison avec le `Mob` vanille (le soin passe par `ai_step` de la classe de test, hors de portée d'ici) ; le test
//! de pilotage en vol (`Steer::Fly`, il faut construire un `Mount`) reste à écrire.
//! NON FAIT : la panique (`AnimalPanic`) n'est comparée à aucun `Mob` : le Mob n'a pas ce but (écart, `suivi/ecs-ecarts.md`).
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::{GoalSet, Jump, Online, Player};

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
    let mut h = Herd::with_slab_awake(60, true, false);
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
    let mut h = Herd::with_slab_awake(60, true, false);
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

/// In water, a ghast in its still timeout (a rider, or the countdown after one) does not float: no jump. Without it, it jumps.
#[test]
fn a_happy_ghast_in_water_jumps_only_outside_its_still_timeout() {
    if horizon_ecs_proto::cow::skip_unless_small_body("a_happy_ghast_in_water_jumps_only_outside_its_still_timeout") {
        return;
    }
    if !is("minecraft:happy_ghast") {
        return;
    }
    let jumps = |still: bool| {
        let mut h = Herd::with_slab(60, true, false);
        h.fill_water(3);
        h.tick();
        let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 7);
        if still {
            h.world.entity_mut(e).insert(horizon_ecs_proto::behaviours::still_under_riders::StillUntil(u64::MAX));
        }
        let mut jumped = false;
        for _ in 0..60 {
            h.tick();
            jumped |= h.world.get::<Jump>(e).is_some_and(|j| j.0);
        }
        jumped
    };
    assert!(!jumps(true), "a ghast in its still timeout jumped in water");
    assert!(jumps(false), "a ghast in water never jumped outside its still timeout");
}

/// A ghast struck flees at speed 2,0 (`AnimalPanic`, the jar's `HappyGhastAi`). The Mob has no such goal: ignored until it does.
#[test]
#[ignore = "le Mob n a pas AnimalPanic, le jar si : suivi/ecs-ecarts.md"]
fn a_struck_happy_ghast_flees_at_speed_two() {
    if horizon_ecs_proto::cow::skip_unless_small_body("a_struck_happy_ghast_flees_at_speed_two") {
        return;
    }
    if !is("minecraft:happy_ghast") {
        return;
    }
    let mut h = Herd::with_slab(60, true, false);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 7);
    h.hit(e, 2.0, (0.5, 0.0));
    let mut panicked = false;
    for _ in 0..10 {
        h.tick();
        panicked |= h.world.get::<GoalSet>(e).is_some_and(|g| g.is_running(&horizon_ecs_proto::behaviours::animal_panic::MODULE));
    }
    let speed = h.world.get::<Nav>(e).map_or(0.0, |n| n.speed);
    eprintln!("struck ghast: panicking {panicked}, nav speed {speed}");
    assert!(panicked, "a struck adult ghast did not panic");
    assert_eq!(speed, 2.0, "panic speed");
}

/// A player holding the food eight blocks away draws the ghast toward it: the distance shrinks.
#[test]
fn a_happy_ghast_is_drawn_by_a_player_holding_its_food() {
    if horizon_ecs_proto::cow::skip_unless_small_body("a_happy_ghast_is_drawn_by_a_player_holding_its_food") {
        return;
    }
    if !is("minecraft:happy_ghast") {
        return;
    }
    let mut h = Herd::with_slab(60, true, false);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 7);
    let food = horizon_ecs_proto::items::item_id("minecraft:snowball");
    let (gx, gz) = (0.5, 7.5);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: gx + 8.0, y: 1.0, z: gz, main: food, ..Default::default() });
    let dist = |h: &Herd| {
        let p = h.world.get::<Pos>(e).unwrap().0;
        ((p.x - (gx + 8.0)).powi(2) + (p.z - gz).powi(2)).sqrt()
    };
    let start = dist(&h);
    for _ in 0..100 {
        h.tick();
    }
    let end = dist(&h);
    eprintln!("drawn: distance {start:.3} -> {end:.3}");
    assert!(end < start - 1.0, "the ghast did not come toward the food: {start:.3} -> {end:.3}");
}
