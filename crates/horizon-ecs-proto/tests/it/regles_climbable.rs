//! `LivingEntity.onClimbable` and `handleOnClimbable` for the ECS bodies, as the Mob's tests in `living_climbable_tests.rs` state them:
//! an equine and a flying monster are never on a climbable; a horse in vines is pushable; a horse falls through vines at full speed; a horse
//! pressed into a ladder gains no height. The expected values are the Mob's, not the code's.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::rules::climbable::NoClimb;

const EQUINES: [&str; 9] = [
    "minecraft:horse",
    "minecraft:donkey",
    "minecraft:mule",
    "minecraft:skeleton_horse",
    "minecraft:zombie_horse",
    "minecraft:camel",
    "minecraft:camel_husk",
    "minecraft:llama",
    "minecraft:trader_llama",
];

const FLIERS: [&str; 3] = ["minecraft:ghast", "minecraft:happy_ghast", "minecraft:phantom"];

/// A stone floor at y = 0 and a stone wall at z = 2 from y = 1 to 14; the column at z = 1 holds `block` (with its properties) when one is given.
fn wall(h: &mut Herd, block: Option<(&'static str, &'static [(&'static str, &'static str)])>) {
    for x in 0..=4 {
        for z in 0..=4 {
            h.set_block(x, 0, z, "minecraft:stone");
        }
        for y in 1..=14 {
            h.set_block(x, y, 2, "minecraft:stone");
            if let Some((name, props)) = block {
                h.set_block_state(x, y, 1, name, props);
            }
        }
    }
}

/// A body of the species `name` at (x, y, z), its activity kept on.
fn spawn(h: &mut Herd, name: &str, x: f64, y: f64, z: f64) -> Entity {
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == name).unwrap();
    let e = spawn_species(&mut h.world, &sp, x, z, 1);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.get_mut::<Pos>(e).unwrap().0.y = y;
    e
}

/// `Mob.onClimbable` is a plain false for an equine and for the flying monsters: they carry the exclusion, the cow does not.
#[test]
fn an_equine_and_a_flying_monster_are_never_on_a_climbable() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    wall(&mut h, Some(("minecraft:vine", &[("south", "true")])));
    let cow = spawn(&mut h, "minecraft:cow", 1.5, 2.0, 1.5);
    assert!(h.world.get::<NoClimb>(cow).is_none(), "a cow is excluded from the climbables");
    for name in EQUINES.iter().chain(FLIERS.iter()).copied() {
        let e = spawn(&mut h, name, 1.5, 2.0, 1.5);
        assert!(h.world.get::<NoClimb>(e).is_some(), "{name} is on a climbable");
    }
}

/// `LivingEntity.isPushable` is vetoed on a climbable: a horse standing in vines is pushable, a cow in the same vines is not.
#[test]
fn a_horse_in_vines_is_still_pushable() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    wall(&mut h, Some(("minecraft:vine", &[("south", "true")])));
    let horse = spawn(&mut h, "minecraft:horse", 1.5, 2.0, 1.5);
    let cow = spawn(&mut h, "minecraft:cow", 3.5, 2.0, 1.5);
    h.tick();
    assert!(horizon_ecs_proto::vehicle_push::pushable(&h.world, horse), "the horse in vines is not pushable");
    assert!(!horizon_ecs_proto::vehicle_push::pushable(&h.world, cow), "the cow in vines is pushable");
}

/// Drops a body from y = 12 in the column at x = 1 (vines or open air) and returns its fall of each tick until it lands.
fn drops(name: &str, vines: bool) -> Vec<f64> {
    let mut h = Herd::with_slab_awake(8, true, true);
    wall(&mut h, vines.then_some(("minecraft:vine", &[("south", "true")][..])));
    let e = spawn(&mut h, name, 1.5, 12.0, 1.1);
    h.world.get_mut::<Contact>(e).unwrap().on_ground = false;
    let mut out = Vec::new();
    while !h.world.get::<Contact>(e).unwrap().on_ground && out.len() < 200 {
        let before = h.world.get::<Pos>(e).unwrap().0.y;
        h.tick();
        out.push(before - h.world.get::<Pos>(e).unwrap().0.y);
    }
    out
}

/// A horse dropped through vines falls exactly as it does through open air; a cow in the same vines slides down at 0.15 a tick.
#[test]
fn a_horse_falls_through_vines_at_full_speed() {
    if !have_data() {
        return;
    }
    let open = drops("minecraft:horse", false);
    let vines = drops("minecraft:horse", true);
    let cow = drops("minecraft:cow", true);
    assert!(open.len() < 40 && open.iter().cloned().fold(0.0, f64::max) > 0.5, "open air: {open:?}");
    let limit = 0.15_f32 as f64;
    assert!(cow.len() > 3 && cow[2..cow.len() - 1].iter().all(|d| (d - limit).abs() < 1.0e-9), "a cow in the same vines slides down at 0.15: {cow:?}");
    assert_eq!(vines, open, "vines change the horse's fall");
}

/// A cow pressed into a ladder climbs it; an equine pressed into the same ladder gains no height.
#[test]
fn a_horse_pressed_into_a_ladder_gains_no_height() {
    if !have_data() {
        return;
    }
    let climbed = |name: &str| {
        let mut h = Herd::with_slab_awake(8, true, true);
        wall(&mut h, Some(("minecraft:ladder", &[("facing", "south")])));
        let e = spawn(&mut h, name, 1.5, 1.0, 1.0);
        let start = h.world.get::<Pos>(e).unwrap().0.y;
        let mut pressed = false;
        for _ in 0..10 {
            if let Some(mut d) = h.world.get_mut::<Drive>(e) {
                d.speed = 0.3;
                d.zza = 1.0;
            }
            h.tick();
            pressed |= h.world.get::<Contact>(e).unwrap().horizontal;
        }
        (pressed, h.world.get::<Pos>(e).unwrap().0.y - start)
    };
    let (cow_pressed, cow_rise) = climbed("minecraft:cow");
    assert!(cow_pressed, "the cow reached the wall");
    assert!(cow_rise > 0.5, "a cow climbs the ladder: {cow_rise}");
    for name in EQUINES {
        let (pressed, rise) = climbed(name);
        assert!(pressed, "{name} reached the wall");
        assert!(rise <= 0.0, "{name} climbed {rise}");
    }
}
