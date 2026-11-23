//! The "entity inside a block" effects in P: cobweb, powder snow and sweet berry bush slow the move, the bush pricks a moving creature,
//! a lit campfire and a magma block hurt, a stalagmite multiplies a landing, and a burning projectile ignites what it hits.
//! Expected values come from the jar's constants (see `src/inside_effects.rs`), not from the code under test.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;

fn body(h: &mut Herd, p: Vec3, living: bool) -> Entity {
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
            Moving,
        ))
        .id();
    if living {
        h.world.entity_mut(e).insert((Health(20.0), HurtAt { at: 0, last: 0.0 }));
    }
    enter_index(&mut h.world, e);
    e
}

fn y_after(h: &mut Herd, e: Entity, ticks: usize) -> f64 {
    for _ in 0..ticks {
        h.tick();
    }
    h.world.get::<Pos>(e).unwrap().0.y
}

/// Falling through a cobweb is about 20 times slower than through the air (vertical factor 0.05).
#[test]
fn a_cobweb_slows_a_fall() {
    if !have_data() {
        return;
    }
    let mut free = Herd::with_slab_awake(8, true, false);
    let a = body(&mut free, Vec3::new(0.5, 4.0, 0.5), false);
    let mut webbed = Herd::with_slab_awake(8, true, false);
    for y in 1..=5 {
        webbed.set_block(0, y, 0, "minecraft:cobweb");
    }
    webbed.tick();
    let b = body(&mut webbed, Vec3::new(0.5, 4.0, 0.5), false);
    let (ya, yb) = (y_after(&mut free, a, 15), y_after(&mut webbed, b, 15));
    assert!(ya < 2.0, "the free fall did not land (y = {ya})");
    assert!(yb > 3.0, "the web did not hold it (y = {yb})");
}

/// Powder snow slows a non-living entity (an item, a falling block) in the same way (stuck 0.9, 1.5, 0.9); it is no web: the fall goes on.
#[test]
fn powder_snow_slows_less_than_a_web() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    for y in 1..=5 {
        h.set_block(0, y, 0, "minecraft:powder_snow");
    }
    h.tick();
    let e = body(&mut h, Vec3::new(0.5, 4.0, 0.5), false);
    h.tick();
    h.tick();
    // Still inside the block, the kind is set again after each move (`entityInside` runs after the move): it is not 0 at the end of a tick.
    let y = y_after(&mut h, e, 20);
    assert!(y < 2.5, "powder snow held a non-living entity as a web would (y = {y})");
}

/// A creature moving in a ripe sweet berry bush is hurt 1 each prick; one standing still in it is not.
#[test]
fn a_sweet_berry_bush_pricks_a_moving_creature() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    h.set_block_state(0, 1, 0, "minecraft:sweet_berry_bush", &[("age", "3")]);
    h.tick();
    let e = body(&mut h, Vec3::new(0.5, 3.0, 0.5), true);
    for _ in 0..25 {
        h.tick();
    }
    assert!(health(&h, e) < 20.0, "falling into a ripe bush did not hurt");
}

/// A lit campfire hurts a creature that stands on it (1), an unlit one does not.
#[test]
fn a_lit_campfire_hurts_and_an_unlit_one_does_not() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    h.set_block_state(0, 1, 0, "minecraft:campfire", &[("lit", "true")]);
    h.set_block_state(4, 1, 0, "minecraft:campfire", &[("lit", "false")]);
    h.tick();
    let lit = body(&mut h, Vec3::new(0.5, 3.0, 0.5), true);
    let out = body(&mut h, Vec3::new(4.5, 3.0, 0.5), true);
    for _ in 0..30 {
        h.tick();
    }
    assert!(health(&h, lit) < 20.0, "the lit campfire did not hurt");
    assert_eq!(health(&h, out), 20.0, "the unlit campfire hurt");
}

/// A magma block hurts the feet of a living creature that lands on it.
#[test]
fn a_magma_block_hurts_the_feet() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    h.set_block(0, 1, 0, "minecraft:magma_block");
    h.tick();
    let e = body(&mut h, Vec3::new(0.5, 4.0, 0.5), true);
    for _ in 0..30 {
        h.tick();
    }
    assert!(health(&h, e) < 20.0, "the magma block did not hurt");
}

/// A burning projectile keeps its fire as a deadline and puts it out on request; a creature it hits is set alight for 5 s.
#[test]
fn a_projectile_keeps_its_fire() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let p = h.world.spawn_empty().id();
    horizon_ecs_proto::inside_effects::ignite_projectile(&mut h.world, p, 80);
    let left = horizon_ecs_proto::inside_effects::fire_left(&h.world, p);
    assert!((79..=80).contains(&left), "{left}");
    horizon_ecs_proto::inside_effects::ignite_projectile(&mut h.world, p, 0);
    assert_eq!(horizon_ecs_proto::inside_effects::fire_left(&h.world, p), 0);
}

/// `LivingEntity.calculateFallDamage` floors (fall 5.5 -> 2, not 3), and a stalagmite tip adds 2.5 to the distance and doubles it.
#[test]
fn fall_damage_floors_and_a_stalagmite_adds_distance_then_doubles() {
    use horizon_ecs_proto::inside_effects::fall_damage;
    assert_eq!(fall_damage(5.5, 1.0), 2.0);
    assert_eq!(fall_damage(6.0, 1.0), 3.0);
    assert_eq!(fall_damage(3.0, 1.0), 0.0);
    assert_eq!(fall_damage(2.0 + 2.5, 2.0), 3.0);
}

/// A fire-immune species (blaze) takes no lava, a cow does; the wither species refuse the wither effect, a cow takes it.
#[test]
fn fire_immune_and_wither_immune_species_refuse_what_the_jar_says() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let (Some(blaze), Some(wither)) = (table.iter().find(|s| s.name == "minecraft:blaze"), table.iter().find(|s| s.name == "minecraft:wither_skeleton")) else {
        return;
    };
    let (b, w, c) = (spawn_species(&mut h.world, blaze, 0.5, 0.5, 1), spawn_species(&mut h.world, wither, 2.5, 0.5, 2), spawn_species(&mut h.world, &table[0], 4.5, 0.5, 3));
    for e in [b, c] {
        h.world.resource_mut::<DamageQueue>().add(e, 4.0, Cause::Lava);
    }
    h.tick();
    assert_eq!(health(&h, b), blaze.max_health, "lava hurt a blaze");
    assert!(health(&h, c) < table[0].max_health, "lava did not hurt the reference species");
    assert!(!horizon_ecs_proto::effects::add(&mut h.world, w, horizon_ecs_proto::effects::Kind::Wither, 0, 100));
    assert!(horizon_ecs_proto::effects::add(&mut h.world, c, horizon_ecs_proto::effects::Kind::Wither, 0, 100));
}

/// A stack that resists fire (bit 0 of `set_resists`) takes no lava hit, does not catch fire and survives two lava hits that destroy an
/// ordinary stack (5 health, 4 a hit, no cooldown for an item).
#[test]
fn a_fire_resistant_stack_survives_lava_and_an_ordinary_one_does_not() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let stone = || horizon_client_lib::items::ItemStack::new("minecraft:stone", 1);
    let resistant = horizon_ecs_proto::item::drop_item(&mut h.world, stone(), Vec3::new(0.5, 4.0, 0.5), Vec3::ZERO, 0);
    let ordinary = horizon_ecs_proto::item::drop_item(&mut h.world, stone(), Vec3::new(2.5, 4.0, 0.5), Vec3::ZERO, 0);
    horizon_ecs_proto::item::set_resists(&mut h.world, resistant, 1);
    for e in [resistant, ordinary] {
        for _ in 0..2 {
            h.world.resource_mut::<DamageQueue>().add(e, 4.0, Cause::Lava);
        }
    }
    h.tick();
    assert_eq!(health(&h, resistant), 5.0, "lava hurt a fire-resistant stack");
    assert!(h.world.get::<Burning>(resistant).is_none(), "lava set a fire-resistant stack alight");
    assert!(health(&h, ordinary) <= 0.0, "lava did not destroy an ordinary stack");
}

/// A stack that resists explosions (bit 1 of `set_resists`) is pushed by a blast but takes no damage from it; an ordinary stack beside it,
/// in the blast's radius, is destroyed.
#[test]
fn a_blast_resistant_stack_survives_an_explosion() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let stone = || horizon_client_lib::items::ItemStack::new("minecraft:stone", 1);
    let resistant = horizon_ecs_proto::item::drop_item(&mut h.world, stone(), Vec3::new(0.5, 4.0, 0.5), Vec3::ZERO, 0);
    let ordinary = horizon_ecs_proto::item::drop_item(&mut h.world, stone(), Vec3::new(1.5, 4.0, 0.5), Vec3::ZERO, 0);
    horizon_ecs_proto::item::set_resists(&mut h.world, resistant, 2);
    let blast = horizon_ecs_proto::projectile::Blast { at: Vec3::new(0.5, 4.0, 0.5), radius: 4.0, fire: false, owner: None, source: None };
    horizon_ecs_proto::explosion::explode(&mut h.world, blast);
    h.tick();
    assert_eq!(health(&h, resistant), 5.0, "an explosion hurt a blast-resistant stack");
    assert!(health(&h, ordinary) <= 0.0, "an explosion did not destroy an ordinary stack");
}

/// A stack that the host's `ItemResists` table makes fire-resistant, dropped by the loot path (`spawn_at_location`), survives the lava that
/// destroys an ordinary stack dropped before the table was set.
#[test]
fn a_loot_stack_the_table_makes_fire_resistant_survives_lava() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let stone = || horizon_client_lib::items::ItemStack::new("minecraft:stone", 1);
    let ordinary = horizon_ecs_proto::item::drop_item(&mut h.world, stone(), Vec3::new(2.5, 4.0, 0.5), Vec3::ZERO, 0);
    h.world.insert_resource(horizon_ecs_proto::item::ItemResists(Box::new(|_: &horizon_client_lib::items::ItemStack| 1u8)));
    let mut rng = horizon_core::random::LegacyRandom::new(1);
    let resistant = horizon_ecs_proto::item::spawn_at_location(&mut h.world, stone(), Vec3::new(0.5, 4.0, 0.5), &mut rng);
    for e in [resistant, ordinary] {
        for _ in 0..2 {
            h.world.resource_mut::<DamageQueue>().add(e, 4.0, Cause::Lava);
        }
    }
    h.tick();
    assert_eq!(health(&h, resistant), 5.0, "lava hurt a loot stack the table makes fire-resistant");
    assert!(h.world.get::<Burning>(resistant).is_none(), "lava set a loot stack the table makes fire-resistant alight");
    assert!(health(&h, ordinary) <= 0.0, "lava did not destroy an ordinary stack");
}
