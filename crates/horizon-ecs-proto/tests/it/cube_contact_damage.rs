//! The contact damage of the cubes (`AbstractCubeMob.playerTouch`, `push(Entity)`, `canDealDamage`, jar 26.3): a player whose box meets a
//! cube is hurt when the cube is not tiny (a magma cube bites at size 1); the damage is the size (+2 for a magma cube); an iron golem
//! that overlaps a cube is hurt. Expected values are the jar's, not read from the module.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::{cow::*, goals::*, hostile::PlayerState, size::set_size};

fn ecs_at(h: &mut Herd, name: &str, x: f64, seed: u64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    let e = horizon_ecs_proto::cow::spawn_species(&mut h.world, &sp, x, 0.5, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    e
}
/// The host's way to give a player (the touch is asked when a player comes or moves).
fn place_player(h: &mut Herd, x: f64) {
    let fact = horizon_ecs_proto::bridge::players::PlayerFact { alive: true, targetable: true, hero: None, ..Default::default() };
    horizon_ecs_proto::bridge::players::set_player(&mut h.world, Player { id: 1, x, y: 1.0, z: 0.5, ..Default::default() }, fact);
}
/// The damage of the first hit a player standing beside a cube of `size` takes in 40 ticks, if any.
fn first_hit(name: &str, size: i32) -> Option<f32> {
    let mut h = Herd::with_slab_awake(60, false, false);
    let c = ecs_at(&mut h, name, 0.5, 4242);
    set_size(&mut h.world, c, size);
    h.world.resource_mut::<PlayerState>().immortal = true;
    h.tick(); // the cube enters the index
    place_player(&mut h, 0.5);
    for _ in 0..40 {
        h.tick();
        if let Some(hit) = h.world.resource::<PlayerState>().hits.iter().find(|x| x.player == 1) {
            return Some(hit.amount);
        }
    }
    None
}

#[test]
fn a_cube_hurts_a_touching_player_by_its_size_unless_tiny() {
    if !have_data() {
        return;
    }
    // Slime: tiny (size 1) never bites; size 2 deals 2, size 3 deals 3.
    assert_eq!(first_hit("minecraft:slime", 1), None);
    assert_eq!(first_hit("minecraft:slime", 2), Some(2.0));
    assert_eq!(first_hit("minecraft:slime", 3), Some(3.0));
    // Magma cube: bites at size 1, with +2 (`MagmaCube.getAttackDamage`).
    assert_eq!(first_hit("minecraft:magma_cube", 1), Some(3.0));
    assert_eq!(first_hit("minecraft:magma_cube", 2), Some(4.0));
}

#[test]
fn a_player_out_of_reach_is_not_hurt() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, false, false);
    let c = ecs_at(&mut h, "minecraft:slime", 0.5, 4243);
    set_size(&mut h.world, c, 2);
    h.world.resource_mut::<PlayerState>().immortal = true;
    place_player(&mut h, 6.5);
    for _ in 0..20 {
        h.tick();
    }
    assert!(h.world.resource::<PlayerState>().hits.is_empty());
}

#[test]
fn a_cube_hurts_an_iron_golem_it_overlaps_but_not_a_tiny_one() {
    if !have_data() {
        return;
    }
    for (size, hurt) in [(1, false), (2, true)] {
        let mut h = Herd::with_slab_awake(60, false, false);
        let c = ecs_at(&mut h, "minecraft:slime", 0.5, 4244);
        set_size(&mut h.world, c, size);
        let g = ecs_at(&mut h, "minecraft:iron_golem", 0.5, 4245);
        let full = h.world.get::<Health>(g).unwrap().0;
        for _ in 0..40 {
            h.tick();
        }
        let now = h.world.get::<Health>(g).map_or(0.0, |x| x.0);
        assert_eq!(now < full, hurt, "size {size}: golem health {now} of {full}");
    }
}
