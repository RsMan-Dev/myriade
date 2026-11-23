//! Le mouvement et la chute d'un corps vivant : `Entity.getBlockSpeedFactor` (le facteur du bloc lerpé vers 1 par `movement_efficiency`,
//! `LivingEntity.getBlockSpeedFactor`) et `LivingEntity.calculateFallDamage` (la distance sûre et le multiplicateur des attributs).
//! Valeurs du jar : sable des âmes 0,4 ; défauts `movement_efficiency` 0, `safe_fall_distance` 3, `fall_damage_multiplier` 1.
use bevy_ecs::prelude::Entity;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::attr_mod::{touched, AttrMods};
use horizon_ecs_proto::cow::fluids::{air_consumed, OxygenBonus};
use horizon_ecs_proto::cow::{block_speed, spawn_species, Body, Herd, Level, Pos, SpeciesTable, Vel};
use horizon_ecs_proto::inside_effects::fall_damage_safe;

fn cow(h: &mut Herd) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let species = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
    spawn_species(&mut h.world, &species, 4.5, 4.5, 5)
}

#[test]
fn a_living_body_on_soul_sand_takes_the_block_factor_unless_it_is_efficient() {
    let mut h = Herd::with_slab(4, false, false);
    h.set_block(1, 0, 0, "minecraft:soul_sand");
    let e = cow(&mut h);
    let mut body = *h.world.get::<Body>(e).unwrap();
    let pos = Pos(Vec3::new(1.5, 1.0, 0.5));
    let moving = Vel(Vec3::new(0.1, 0.0, 0.0));
    let still = Vel(Vec3::new(0.0, 0.0, 0.0));
    let level = h.world.resource::<Level>();
    assert!((block_speed(level, &pos, &body, &moving, false) - 0.4).abs() < 1e-6);
    body.movement_efficiency = 1.0;
    assert!((block_speed(level, &pos, &body, &moving, false) - 1.0).abs() < 1e-6);
    body.movement_efficiency = 0.0;
    assert_eq!(block_speed(level, &pos, &body, &still, false), 1.0);
    assert!((block_speed(level, &pos, &body, &still, true) - 0.4).abs() < 1e-6);
}

#[test]
fn the_safe_fall_distance_and_the_damage_multiplier_are_the_landing_attributes() {
    let mut h = Herd::with_slab(4, false, false);
    let e = cow(&mut h);
    let mods = vec![("safe_fall_distance".to_string(), 10.0, Vec::new()), ("fall_damage_multiplier".to_string(), 2.0, Vec::new())];
    h.world.entity_mut(e).insert(AttrMods(mods));
    touched(&mut h.world, e, "safe_fall_distance");
    touched(&mut h.world, e, "fall_damage_multiplier");
    let body = *h.world.get::<Body>(e).unwrap();
    assert_eq!((body.safe_fall, body.fall_mult), (10.0, 2.0));
    assert_eq!(fall_damage_safe(8.0, body.safe_fall, body.fall_mult), 0.0);
    assert_eq!(fall_damage_safe(8.0, 3.0, 1.0), 5.0);
    assert_eq!(fall_damage_safe(8.0, 3.0, 2.0), 10.0);
}

#[test]
fn the_oxygen_bonus_spares_the_air_on_the_draws_under_one_over_bonus_plus_one() {
    let mut h = Herd::with_slab(4, false, false);
    let e = cow(&mut h);
    h.world.entity_mut(e).insert(AttrMods(vec![("oxygen_bonus".to_string(), 1.0, Vec::new())]));
    touched(&mut h.world, e, "oxygen_bonus");
    assert_eq!(h.world.get::<OxygenBonus>(e).unwrap().0, 1.0);
    let bonus = h.world.get::<OxygenBonus>(e).unwrap().0;
    assert_eq!(air_consumed(bonus, 0.4), 1);
    assert_eq!(air_consumed(bonus, 0.6), 0);
    assert_eq!(air_consumed(0.0, 0.99), 1);
}
