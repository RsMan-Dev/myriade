//! Le travail et la reproduction du villageois (lot V4) sur un troupeau éveillé. Valeurs attendues lues dans le `Mob`, pas dans le code ECS.
use crate::common;
use bevy_ecs::prelude::Entity;
use common::*;
use horizon_ecs_proto::behaviours::food_stock::Food;
use horizon_ecs_proto::brain::behaviours::work_breed;
use horizon_ecs_proto::behaviours::act_phase::{ActPhase, RareState};
use horizon_ecs_proto::goals::{Entered, Jump, Look};
use horizon_ecs_proto::targeting::{set_target, Target};
use horizon_ecs_proto::{brain::*, cow::*};

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:villager"])
}

fn villager(h: &mut Herd) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:villager").unwrap();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 1);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    e
}

/// `Swim.tick` (`villager_brain.rs:2680`) : dans l'eau, le villageois saute avec une chance de 0.8 à chaque tick de sa nage (60 ticks).
#[test]
fn the_villager_in_water_jumps_about_four_ticks_in_five_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = villager(&mut h);
    let mut jumps = 0;
    for _ in 0..60 {
        h.world.get_mut::<FluidState>(e).unwrap().in_water = true;
        h.tick();
        if h.world.get::<Jump>(e).is_some_and(|j| j.0) {
            jumps += 1;
        }
    }
    assert!((30..=58).contains(&jumps), "sauts sur 60 ticks : {jumps}");
}

/// `canBreed` (`villager.rs:984`) : 12 points de nourriture suffisent, 11 non.
#[test]
fn the_villager_with_12_food_points_is_ready_to_breed_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = villager(&mut h);
    h.world.entity_mut(e).insert(Food(12));
    assert!(work_breed::ready(&h.world, e));
    h.world.entity_mut(e).insert(Food(11));
    assert!(!work_breed::ready(&h.world, e));
}

fn warden(h: &mut Herd) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:warden").unwrap();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 1);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    e
}

/// `SetWardenLookTarget` (`warden_brain.rs:738-760`) : sans cible, le Warden regarde le bloc de sa perturbation.
#[test]
fn the_warden_without_target_looks_at_its_disturbance_awake() {
    if !have_data() || wrong_species(&["minecraft:warden"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = warden(&mut h);
    h.world.get_mut::<RareState>(e).unwrap().disturbance = Some([10, 64, 10]);
    h.world.get_mut::<RareState>(e).unwrap().disturbance_until = u64::MAX;
    h.tick();
    let look = h.world.get::<Look>(e).unwrap().target;
    assert!((look[0] - 10.5).abs() < 1e-6 && (look[1] - 64.5).abs() < 1e-6 && (look[2] - 10.5).abs() < 1e-6, "regard : {look:?}");
}

/// `StopAttackingIfTargetInvalid` (`warden_brain.rs:975-995`) : une cible absente du monde est lâchée, le creusement est retardé.
#[test]
fn the_warden_drops_a_target_that_is_gone_awake() {
    if !have_data() || wrong_species(&["minecraft:warden"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = warden(&mut h);
    set_target(&mut h.world, e, Target::player(9999, 100));
    h.tick();
    assert!(h.world.get::<Target>(e).is_none());
    assert!(h.world.get::<ActPhase>(e).unwrap().dig_cooldown_until > now_of(&h.world, e));
}
