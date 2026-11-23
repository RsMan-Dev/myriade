//! Règle du piétinement, reprise de `farmland_event_tests.rs` (valeurs inchangées) : une chute sur du labour le transforme en
//! terre (`FarmlandBlock.fallOn`), avec l'événement de bloc posté par l'entité ; un corps trop fin (width² · height ≤ 0.512) ne le fait pas.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::host_out::HostEvent;

fn body(h: &mut Herd, p: Vec3, width: f32, height: f32) -> Entity {
    let e = h
        .world
        .spawn((
            Pos(p),
            Vel(Vec3::ZERO),
            Body { width, height, step_height: 0.6, speed: 0.0, fall_factor: 1.0, safe_fall: 3.0, fall_mult: 1.0, movement_efficiency: 0.0, object: false, orb: false, tnt: false, lava_walker: false },
            Gravity(0.08),
            Bounds(box_at(p, width, height)),
            Cell(u32::MAX, [i32::MIN; 8], u32::MAX),
            Contact { on_ground: false, horizontal: false, fall_distance: 0.0, stuck: 0 },
            FluidState::default(),
            Moving,
            Health(20.0),
            HurtAt { at: 0, last: 0.0 },
        ))
        .id();
    enter_index(&mut h.world, e);
    e
}

/// Drops a body on the farmland at (0, 0, 0) and returns the name of that block and the game events the host heard.
fn drop_on_farmland(width: f32, height: f32) -> (String, Vec<HostEvent>) {
    let mut h = Herd::with_slab_awake(8, true, false);
    h.set_block(0, 0, 0, "minecraft:farmland");
    h.tick();
    body(&mut h, Vec3::new(0.5, 4.0, 0.5), width, height);
    let mut heard = Vec::new();
    for _ in 0..15 {
        h.tick();
        heard.extend(h.drain_host_out());
    }
    let mw = h.world.resource::<Level>().mw.0;
    let name = mw.facts.get(mw.world.block(0, 0, 0)).name.to_string();
    (name, heard)
}

#[test]
fn a_creature_falling_onto_farmland_turns_it_to_dirt_and_says_so() {
    if !have_data() {
        return;
    }
    let (name, heard) = drop_on_farmland(0.6, 1.8);
    assert_eq!(name, "minecraft:dirt");
    assert!(
        heard.iter().any(|ev| matches!(ev, HostEvent::GameEventBy { name: "minecraft:block_change", .. })),
        "the block_change event names the entity that trampled it"
    );
}

#[test]
fn a_body_too_thin_to_trample_leaves_the_farmland() {
    if !have_data() {
        return;
    }
    let (name, _) = drop_on_farmland(0.2, 0.4);
    assert_eq!(name, "minecraft:farmland", "width² · height = 0.016 is not above 0.512");
}

#[test]
fn every_body_on_the_turned_farmland_is_set_on_its_top() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    h.tick();
    let a = ecs_cow(&mut h, 0.3, 0.9375, 0.3, 5);
    let b = ecs_cow(&mut h, 0.7, 0.9375, 0.7, 6);
    horizon_ecs_proto::landing_trample::push_entities_up(&mut h.world, 0, 0, 0);
    for e in [a, b] {
        let y = h.world.get::<Pos>(e).unwrap().0.y;
        assert!((y - 1.0).abs() < 1e-6, "Block.pushEntitiesUp: the feet go to the top of the block (y = {y})");
    }
}
