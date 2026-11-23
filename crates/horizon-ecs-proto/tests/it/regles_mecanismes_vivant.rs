//! The landing rules of `LivingEntity` the living tests fixed on the old Mob, now on the ECS: the dust of a landing
//! (`checkFallDamage`), the fall sounds of a hurt landing (`causeFallDamage`, `playBlockFallSound`, the monster pair), the distance a
//! block takes off a fall (`Block.fallOn`), and the step sound of a walk by emission (`playStepSound`).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::bridge::host::HostSetup;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::landing_trample::{landing_dust_at, landing_dust_count};

macro_rules! walk {
    ($h:ident, $mw:ident, $e:ident, $ticks:expr) => {{
        let mut out = Vec::new();
        for _ in 0..$ticks {
            goals::set_velocity($h.world_mut(), $e, Vec3::new(0.2, -0.08, 0.0));
            $h.world_mut().entity_mut($e).insert(Moving);
            out.extend($h.step($mw));
        }
        out
    }};
}

fn stone_host() -> (&'static myriade_engine_lib::mobs::MobWorld<'static>, horizon_ecs_proto::bridge::host::EcsWorld) {
    awake_host_on_stone(&HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 })
}

fn sounds(evs: &[HostEvent]) -> Vec<(String, f32)> {
    evs.iter()
        .filter_map(|ev| match ev {
            HostEvent::Sound { name, volume, .. } => Some((name.to_string(), *volume)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_landing_throws_up_dust_by_the_power_of_the_fall() {
    // Within the safe distance of 3, or by a hair over it: power 0.
    assert_eq!(landing_dust_count(2.5, 3.0), None);
    assert_eq!(landing_dust_count(3.0, 3.0), None);
    // A fall of 7: power 4, 150 * (0.2 + 4 / 15) = 70.
    assert_eq!(landing_dust_count(7.0, 3.0), Some(70));
    // A long fall is capped at 150 * 2.5.
    assert_eq!(landing_dust_count(60.0, 3.0), Some(375));
    // A mob that takes no fall damage still has a safe distance of its own.
    assert_eq!(landing_dust_count(7.0, 10.0), None);
}

#[test]
fn landing_dust_is_pulled_to_the_block_it_landed_on() {
    let on = Vec3::new(4.3, 65.0, 7.6);
    assert_eq!(landing_dust_at(on, [4, 7]), on);
    // The mob stands over block (4, 7) but its feet block is (5, 7).
    let straddling = landing_dust_at(Vec3::new(5.1, 65.0, 7.5), [4, 7]);
    assert_eq!(straddling, Vec3::new(5.0, 65.0, 7.5));
}

/// A creature dropped from `from` onto a stone block: the host events of the fall.
fn drop_on_stone(from: f64) -> Vec<HostEvent> {
    let mut h = Herd::with_slab_awake(8, true, false);
    h.set_block(0, 0, 0, "minecraft:stone");
    h.tick();
    ecs_cow(&mut h, 0.5, from, 0.5, 5);
    let mut heard = Vec::new();
    for _ in 0..40 {
        h.tick();
        heard.extend(h.drain_host_out());
    }
    heard
}

/// The health left to a creature dropped from `from` onto `block`, after the fall and 40 ticks.
fn health_after_drop(block: &str, from: f64) -> f32 {
    let mut h = Herd::with_slab_awake(8, true, false);
    h.set_block(0, 0, 0, block);
    h.tick();
    let e = ecs_cow(&mut h, 0.5, from, 0.5, 5);
    for _ in 0..40 {
        h.tick();
    }
    health(&h, e)
}

#[test]
fn a_hurt_landing_plays_the_fall_sound_then_the_landed_blocks_fall_sound() {
    if !have_data() {
        return;
    }
    // A fall of about 5 blocks: 2 of damage, the small fall sound at full volume, then the stone's fall sound at half volume.
    let names = sounds(&drop_on_stone(6.0));
    assert!(names.iter().any(|(n, v)| n == "entity.generic.small_fall" && *v == 1.0), "{names:?}");
    assert!(names.iter().any(|(n, v)| n.contains("stone") && *v < 1.0 && *v > 0.0), "the stone's fall sound at half volume: {names:?}");
}

#[test]
fn a_landing_throws_up_block_dust_of_the_landed_block() {
    if !have_data() {
        return;
    }
    let dust = drop_on_stone(6.0).iter().any(|ev| matches!(ev, HostEvent::BlockParticles { count, .. } if *count > 0));
    assert!(dust, "a fall of about 5 blocks throws up dust of the stone it landed on");
}

#[test]
fn a_fall_within_the_safe_distance_plays_no_fall_sound() {
    if !have_data() {
        return;
    }
    let fell = drop_on_stone(2.0).iter().any(|ev| matches!(ev, HostEvent::Sound { name, .. } if name.ends_with("_fall")));
    assert!(!fell, "a landing without damage plays no fall sound");
}

#[test]
fn a_monster_landing_says_the_hostile_fall_sound() {
    if !have_data() {
        return;
    }
    let (mw, mut h) = stone_host();
    let e = h.spawn(mw, "minecraft:zombie", [0.5, 6.0, 0.5], None).expect("a known species");
    let names: Vec<String> = sounds(&walk!(h, mw, e, 60)).into_iter().map(|(n, _)| n).collect();
    assert!(names.iter().any(|n| n == "entity.hostile.small_fall"), "{names:?}");
    assert!(!names.iter().any(|n| n == "entity.generic.small_fall"), "{names:?}");
}

#[test]
fn a_shelf_mushroom_takes_half_the_distance_of_the_fall() {
    if !have_data() {
        return;
    }
    // A fall of about 7: 4 of damage on stone, and 0 on a shelf mushroom whose `fallDistanceReduction` 0.5 halves it to 3.5.
    let stone = health_after_drop("minecraft:stone", 8.0);
    let mushroom = health_after_drop("minecraft:shelf_mushroom", 8.0);
    assert!(mushroom > stone, "stone {stone}, shelf mushroom {mushroom}");
}

#[test]
fn a_sheep_steps_on_stone_at_fifteen_hundredths_of_the_block_volume() {
    if !have_data() {
        return;
    }
    let (mw, mut h) = stone_host();
    h.set_move_events(true);
    let e = h.spawn(mw, "minecraft:sheep", [-7.5, 1.0, 0.5], None).expect("a known species");
    let _ = walk!(h, mw, e, 40);
    let steps: Vec<f32> = sounds(&walk!(h, mw, e, 20)).into_iter().filter(|(n, _)| n.contains("stone")).map(|(_, v)| v).collect();
    assert!(!steps.is_empty(), "a walk steps at least once");
    assert!(steps.iter().all(|v| (v - 0.15).abs() < 1e-4), "{steps:?}");
}

#[test]
fn a_bat_a_silverfish_a_squid_and_a_glow_squid_walk_without_a_step_sound() {
    if !have_data() {
        return;
    }
    for kind in ["minecraft:bat", "minecraft:silverfish", "minecraft:squid", "minecraft:glow_squid"] {
        let (mw, mut h) = stone_host();
        h.set_move_events(true);
        let e = h.spawn(mw, kind, [-7.5, 1.0, 0.5], None).expect("a known species");
        let _ = walk!(h, mw, e, 40);
        let steps: Vec<(String, f32)> = sounds(&walk!(h, mw, e, 20)).into_iter().filter(|(n, _)| n.contains("stone")).collect();
        assert!(steps.is_empty(), "{kind}: {steps:?}");
    }
}

/// `LivingEntity.floatInLiquidWhileRidden`: a ridden mount in `minecraft:can_float_while_ridden`, deeper in water than its jump threshold,
/// gains 0.04 upward each tick; the same horse with no rider sinks to the floor.
#[test]
fn a_ridden_horse_floats_in_deep_water() {
    if !have_data() {
        return;
    }
    let setup = HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 };
    let (mw, mut h) = awake_host_with(&setup, |blocks, registry| {
        let water = registry.default_state("minecraft:water").unwrap();
        for x in -2..=2 {
            for z in -2..=2 {
                for y in 1..=3 {
                    blocks.set(x, y, z, water).unwrap();
                }
            }
        }
    });
    let rider = h.spawn(mw, "minecraft:villager", [0.5, 2.5, 0.5], None).expect("a known species");
    let ridden = h.spawn(mw, "minecraft:horse", [0.5, 1.5, 0.5], None).expect("a known species");
    let empty = h.spawn(mw, "minecraft:horse", [3.5, 1.5, 0.5], None).expect("a known species");
    assert!(h.mount(mw, rider, ridden), "the rider mounts the horse");
    for _ in 0..40 {
        h.step(mw);
    }
    let y = |e| h.world().get::<Pos>(e).map(|p| p.0.y).expect("a living horse");
    assert!(y(ridden) > y(empty), "ridden {} empty {}", y(ridden), y(empty));
}

/// `Entity.collide` with `getEntityCollisions`: a zombie dropped onto a living shulker lands on its top, not on the stone under it.
#[test]
fn a_falling_zombie_lands_on_a_living_shulker() {
    if !have_data() {
        return;
    }
    let (mw, mut h) = stone_host();
    let _shulker = h.spawn(mw, "minecraft:shulker", [0.5, 1.0, 0.5], None).expect("a known species");
    let zombie = h.spawn(mw, "minecraft:zombie", [0.5, 6.0, 0.5], None).expect("a known species");
    for _ in 0..60 {
        h.step(mw);
    }
    let y = h.world().get::<Pos>(zombie).map(|p| p.0.y).expect("a living zombie");
    assert!(y > 1.5, "the zombie stands on the shulker, not the stone: y {y}");
}

/// `Entity.collide`: the world border's walls stop a mob too. A sheep walking east in a border of size 4 centred on 0 stops at the east
/// wall (x = 2), its box half-width (0.45) off it.
#[test]
fn the_world_border_stops_a_mob() {
    if !have_data() {
        return;
    }
    let border = horizon_core::geometry::border::BorderGeometry::new([0.0, 0.0], 4.0, 29_999_984).expect("a border");
    let (mw, mut h) = awake_host_in_border(&HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }, border);
    let e = h.spawn(mw, "minecraft:sheep", [-1.5, 1.0, 0.5], None).expect("a known species");
    let _ = walk!(h, mw, e, 60);
    let x = h.world().get::<Pos>(e).map(|p| p.0.x).expect("a living sheep");
    assert!(x > -1.5 && x <= 1.6, "the sheep walks to the east wall and stops there: x {x}");
}
