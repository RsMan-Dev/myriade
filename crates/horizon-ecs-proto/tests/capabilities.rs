//! The proof that an entity is what its capability components make of it: an inert object with only a physical body, and a
//! living entity without goals (an armour stand). Each system that has to be changed for these tests to pass is a
//! violation of "no capability supposes another" (see `suivi/ecs-portage/AUDIT-ESPECES.md`, section 9).
//! `HORIZON_TEST_RESOURCES` set.
mod common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;

/// The components of a physical body and nothing else: no life, no goals, no walking, no species.
fn inert(h: &mut Herd, x: f64, y: f64, z: f64) -> Entity {
    let (w, hgt) = (0.25_f32, 0.25_f32);
    let p = Vec3::new(x, y, z);
    let e = h
        .world
        .spawn((
            Pos(p),
            Vel(Vec3::ZERO),
            Body { width: w, height: hgt, step_height: 0.0, gravity: 0.04, speed: 0.0, fall_factor: 1.0 },
            Bounds(box_at(p, w, hgt)),
            Cell(u32::MAX),
            Contact { on_ground: false, horizontal: false, fall_distance: 0.0 },
            FluidState::default(),
            Moving,
        ))
        .id();
    enter_index(&mut h.world, e);
    e
}

/// It falls, comes to rest on the floor, and enters the spatial index.
#[test]
fn an_inert_object_with_only_a_body_falls_and_rests() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(8, true, false);
    let e = inert(&mut h, 0.5, 6.0, 0.5);
    for _ in 0..80 {
        h.tick();
    }
    let y = h.world.get::<Pos>(e).unwrap().0.y;
    assert!(y < 2.0, "the object did not fall (y = {y}): `travel` asks for more than a body");
    assert!(h.world.get::<Contact>(e).unwrap().on_ground, "the object did not land");
    assert!(h.world.get::<Cell>(e).unwrap().0 != u32::MAX, "the object is not in the index");
    assert!(h.world.get::<Moving>(e).is_none(), "the object at rest did not go to sleep");
}

/// The floor under it removed, it falls again (the block-change recheck needs no goals).
#[test]
fn an_inert_object_follows_the_block_changes_under_it() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(8, true, false);
    let e = inert(&mut h, 0.5, 1.0, 0.5);
    for _ in 0..40 {
        h.tick();
    }
    let rest = h.world.get::<Pos>(e).unwrap().0.y;
    h.set_block(0, 0, 0, "minecraft:air");
    for _ in 0..40 {
        h.tick();
    }
    let down = h.world.get::<Pos>(e).unwrap().0.y;
    assert!(down < rest - 0.5, "rest {rest}, after the floor went {down}: the block change did not wake the object");
}

/// A living entity with a body, a health and no goal: a hit hurts it, enough hits kill it, with no selector.
#[test]
fn a_living_entity_without_goals_takes_damage_and_dies() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(8, true, false);
    let e = inert(&mut h, 0.5, 1.0, 0.5);
    h.world.entity_mut(e).insert((Health(6.0), HurtAt { at: 0, last: 0.0 }));
    for _ in 0..5 {
        h.tick();
    }
    h.world.resource_mut::<DamageQueue>().add(e, 2.0, Cause::Entity);
    h.tick();
    let hp = h.world.get::<Health>(e).map(|x| x.0);
    assert_eq!(hp, Some(4.0), "the hit was not applied: `apply_damage` asks for more than a living entity");
    for _ in 0..4 {
        h.world.resource_mut::<DamageQueue>().add(e, 10.0, Cause::Entity);
        for _ in 0..12 {
            h.tick();
        }
    }
    assert!(h.world.get_entity(e).is_err(), "the entity did not die");
}

/// The framework's events: an object that falls lands (`landed`); a burning entity sets `ignited` and puts out (`extinguished`).
#[test]
fn the_framework_events_report_landing_and_fire() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::goals::Events;
    let mut h = Herd::with_slab(8, true, false);
    let e = inert(&mut h, 0.5, 5.0, 0.5);
    let mut landed = false;
    for _ in 0..60 {
        h.tick();
        landed |= h.world.resource::<Events>().landed.iter().any(|x| x == e);
    }
    assert!(landed, "no `landed` event for the object that fell");
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let z = spawn_species(&mut h.world, &table[2], 3.5, 3.5, 77);
    h.set_day(true);
    let (mut lit, mut out) = (false, false);
    for _ in 0..400 {
        h.tick();
        let ev = h.world.resource::<Events>();
        lit |= ev.ignited.iter().any(|x| x == z);
        out |= ev.extinguished.iter().any(|x| x == z);
        if h.world.get_entity(z).is_err() {
            break;
        }
    }
    assert!(lit, "no `ignited` event for the zombie in the sun");
    assert!(out || h.world.get_entity(z).is_err(), "the fire neither went out nor killed");
}

/// A configuration declares its capabilities and `spawn_species` creates those components and no others.
#[test]
fn spawn_makes_only_the_declared_capabilities() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::{behaviours::Cap, goals::GoalSet};
    let mut h = Herd::with_slab(8, true, false);
    let mut sp = h.world.resource::<SpeciesTable>().0[0];
    let mut spawn = |h: &mut Herd, caps: &'static [Cap], x: f64| {
        sp.caps = caps;
        spawn_species(&mut h.world, &sp, x, 0.5, 11)
    };
    let object = spawn(&mut h, &[Cap::Transform, Cap::Solid, Cap::Body], 0.5);
    let stand = spawn(&mut h, &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living], 2.5);
    let full = spawn(&mut h, &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Locomotion, Cap::Goals], 4.5);
    let has = |h: &Herd, e: Entity| {
        (
            h.world.get::<Health>(e).is_some(),
            h.world.get::<Yaw>(e).is_some() || h.world.get::<Stroll>(e).is_some() || h.world.get::<Nav>(e).is_some(),
            h.world.get::<GoalSet>(e).is_some() || h.world.get::<Rng>(e).is_some() || h.world.get::<ActiveUntil>(e).is_some(),
        )
    };
    assert_eq!(has(&h, object), (false, false, false), "the object has a body only");
    assert_eq!(has(&h, stand), (true, false, false), "the stand lives and has no goals nor walking");
    assert_eq!(has(&h, full), (true, true, true));
    for _ in 0..60 {
        h.tick();
    }
    assert!(h.world.get::<Contact>(object).unwrap().on_ground && h.world.get::<Contact>(stand).unwrap().on_ground);
}

/// An entity with only a position and a velocity (a projectile in flight, a display) is visited by no system of the
/// framework: after a hundred ticks next to creatures it is where it was, with the components it had.
#[test]
fn no_system_of_the_framework_visits_an_entity_with_only_a_position_and_a_velocity() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(8, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    for i in 0..4 {
        spawn_species(&mut h.world, &table[0], 0.5 + f64::from(i), 0.5, 5 + i as u64);
    }
    let p = Vec3::new(1.5, 5.0, 0.5);
    let e = h.world.spawn((Pos(p), Vel(Vec3::new(0.1, 0.0, 0.0)))).id();
    let (n, vel) = (h.world.entity(e).archetype().components().len(), Vec3::new(0.1, 0.0, 0.0));
    for _ in 0..100 {
        h.tick();
    }
    assert_eq!(h.world.get::<Pos>(e).unwrap().0, p, "a system moved the entity");
    assert_eq!(h.world.get::<Vel>(e).unwrap().0, vel, "a system changed its velocity");
    assert_eq!(h.world.entity(e).archetype().components().len(), n, "a system added or removed a component");
}
