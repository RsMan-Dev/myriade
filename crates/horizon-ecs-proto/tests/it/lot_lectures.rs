//! The piglin hunt's gate on a cooling piglin nearby, and the frog's leap while pregnant (`HORIZON_TEST_RESOURCES` set). Written without a
//! run. A piglin with no cooldown and a piglin still cooling down for 1000 ticks stand two blocks apart, a hoglin six blocks away: no
//! piglin in range hunts until the cooling one is due.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{behaviours::{admire::Admiring, breed::Carrying, celebrate::Celebrating, hunt_prey::{CannotHunt, Hunt}, walk_away::Avoid}, cow::*, goals::*, hostile::*, targeting::*};

#[test]
fn a_hunt_waits_for_the_cooldown_of_a_piglin_nearby_as_a_mob_does() {
    if !have_data() || wrong_species(&["minecraft:piglin"]) {
        return;
    }
    const COOLDOWN: i32 = 1000;
    let mut h = Herd::with_slab_awake(60, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let piglin = *table.iter().find(|s| s.name == "minecraft:piglin").unwrap();
    let hoglin = *table.iter().find(|s| s.name == "minecraft:hoglin").unwrap();
    let hunter_e = spawn_species(&mut h.world, &piglin, 0.5, 0.5, 11);
    let nearby_e = spawn_species(&mut h.world, &piglin, 2.5, 0.5, 12);
    let hoglin_e = spawn_species(&mut h.world, &hoglin, 6.5, 0.5, 13);
    for e in [hunter_e, nearby_e, hoglin_e] {
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.resource_mut::<Entered>().0.push(e);
    }
    let now = now_of(&h.world, hunter_e);
    h.world.get_mut::<Hunt>(hunter_e).unwrap().until = now;
    h.world.get_mut::<Hunt>(nearby_e).unwrap().until = now + COOLDOWN as u64;

    let mut ecs_first = None;
    for t in 1..=1200i64 {
        h.tick();
        if ecs_first.is_none() && [hunter_e, nearby_e].iter().any(|&e| h.world.get::<Target>(e).is_some()) {
            ecs_first = Some(t);
        }
    }
    let c = ecs_first.expect("the ECS hunts");
    eprintln!("hunt after the nearby cooldown: ecs {c}");
    assert!(c >= 900, "no hunt while the piglin nearby cools down: ECS {c}");
}

/// A piglin saved with `CannotHunt` never takes a hoglin as target (the piglin carries the `CannotHunt` marker), 600 ticks with the
/// hoglin six blocks away and the hunt due.
#[test]
fn a_piglin_that_cannot_hunt_takes_no_hoglin_as_a_mob_does() {
    if !have_data() || wrong_species(&["minecraft:piglin"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let piglin = *table.iter().find(|s| s.name == "minecraft:piglin").unwrap();
    let hoglin = *table.iter().find(|s| s.name == "minecraft:hoglin").unwrap();
    let hunter_e = spawn_species(&mut h.world, &piglin, 0.5, 0.5, 11);
    let hoglin_e = spawn_species(&mut h.world, &hoglin, 6.5, 0.5, 13);
    for e in [hunter_e, hoglin_e] {
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.resource_mut::<Entered>().0.push(e);
    }
    h.world.entity_mut(hunter_e).insert(CannotHunt);
    let now = now_of(&h.world, hunter_e);
    h.world.get_mut::<Hunt>(hunter_e).unwrap().until = now;

    for _ in 1..=600i64 {
        h.tick();
    }
    assert!(h.world.get::<Target>(hunter_e).is_none(), "the ECS with CannotHunt takes no target");
}

/// The leaps (take-offs from the ground) of a frog on a 3 x 3 island in a ring of void, in 600 ticks, the frog pregnant or not.
fn island_leaps(pregnant: bool) -> usize {
    let mut h = Herd::with_slab_awake(60, true, false);
    for x in -3..=3i32 {
        for z in -3..=3i32 {
            if x.abs().max(z.abs()) >= 2 {
                for y in -3..=0 {
                    h.set_block(x, y, z, "minecraft:air");
                }
            }
        }
    }
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let frog_sp = *table.iter().find(|s| s.name == "minecraft:frog").unwrap();
    let e = spawn_species(&mut h.world, &frog_sp, 0.5, 0.5, 21);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    if pregnant {
        h.world.entity_mut(e).insert(Carrying);
    }
    let (mut leaps, mut up) = (0usize, false);
    for _ in 0..600 {
        h.tick();
        let b = h.world.get::<Pos>(e).unwrap().0.y > 1.05;
        leaps += usize::from(b && !up);
        up = b;
    }
    leaps
}

/// A frog that is not pregnant leaps off its island, and a pregnant one does not.
#[test]
fn a_pregnant_frog_does_not_leap_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let free = island_leaps(false);
    assert!(free > 0, "a frog that is not pregnant leaps: ECS {free}");
    let pregnant = island_leaps(true);
    assert_eq!(pregnant, 0, "a pregnant frog does not leap");
}

/// A piglin in the ECS at `at`, immune to nothing: the overworld has no unsafe place for it in the ECS.
fn piglin_at(h: &mut Herd, at: Vec3, seed: u64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let piglin = *table.iter().find(|s| s.name == "minecraft:piglin").unwrap();
    let e = spawn_species(&mut h.world, &piglin, at.x, at.z, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    e
}

/// A piglin in Avoid (a point remembered to keep away from) takes no hoglin as target (`Avoid` is live).
#[test]
fn an_attacker_in_avoid_takes_no_hoglin_as_a_mob_does() {
    if !have_data() || wrong_species(&["minecraft:piglin"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let hunter_e = piglin_at(&mut h, Vec3::new(0.5, 1.0, 0.5), 11);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let hoglin = *table.iter().find(|s| s.name == "minecraft:hoglin").unwrap();
    let hoglin_e = spawn_species(&mut h.world, &hoglin, 6.5, 0.5, 13);
    h.world.get_mut::<ActiveUntil>(hoglin_e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(hoglin_e);
    let now = now_of(&h.world, hunter_e);
    h.world.entity_mut(hunter_e).insert(Avoid { at: [3.5, 1.0, 0.5], until: now + 300 });
    h.world.get_mut::<Hunt>(hunter_e).unwrap().until = now;
    for _ in 1..=300i64 {
        h.tick();
    }
    assert!(h.world.get::<Target>(hunter_e).is_none(), "the ECS in Avoid takes no target");
}

/// A soul campfire, lit, repels a piglin only inside the sphere of 8 blocks (3D from the block's bottom centre, `distance_sqr < 64`):
/// 6 blocks east and 4 north is inside (52), 6 east and 6 north is outside (72) although both are inside the scan box.
#[test]
fn a_lit_soul_campfire_repels_a_piglin_inside_the_sphere_only() {
    if !have_data() || wrong_species(&["minecraft:piglin"]) {
        return;
    }
    for (dz, repelled) in [(4, true), (6, false)] {
        let mut h = Herd::with_slab_awake(60, true, false);
        let p_e = piglin_at(&mut h, Vec3::new(0.5, 1.0, 0.5), 11);
        h.set_block(6, 1, dz, "minecraft:soul_campfire");
        for _ in 1..=40i64 {
            h.tick();
        }
        // The repellent is a walk away from the block (not the `Avoid` activity, which a hurt or a hunt sets): it is read from the way
        // gained from the campfire's bottom centre in 40 ticks.
        let fire = Vec3::new(6.5, 1.0, f64::from(dz) + 0.5);
        let gain = |from: Vec3, to: Vec3| to.distance_squared(fire).sqrt() - from.distance_squared(fire).sqrt();
        let start = Vec3::new(0.5, 1.0, 0.5);
        let ecs_avoids = gain(start, h.world.get::<Pos>(p_e).unwrap().0) > 3.0;
        assert_eq!(ecs_avoids, repelled, "the ECS, campfire at dz {dz}");
    }
}

/// A piglin whose target dies walks to the spot the target fell and stays within 2 blocks of it (`Celebrating`, started by the death
/// event).
#[test]
fn celebrate_walks_to_the_kill_spot_as_a_mob_does() {
    if !have_data() || wrong_species(&["minecraft:piglin"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let p_e = piglin_at(&mut h, Vec3::new(0.5, 1.0, 0.5), 11);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let hoglin_sp = *table.iter().find(|s| s.name == "minecraft:hoglin").unwrap();
    let hoglin_e = spawn_species(&mut h.world, &hoglin_sp, 6.5, 0.5, 13);
    h.world.get_mut::<ActiveUntil>(hoglin_e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(hoglin_e);
    // The piglin settles on the ground first: a path asked at the first tick of a creature in the air is refused (`on_ground`) and not asked again.
    for _ in 0..5 {
        h.tick();
    }
    // A target of a goal (as the hunt sets it): it ends with the goal, after the kill is heard; a target without a goal is cleared first.
    let hunt_slot = horizon_ecs_proto::goals::slot_of(&h.world, p_e, &horizon_ecs_proto::behaviours::hunt_prey::MODULE).unwrap();
    h.world.entity_mut(p_e).insert(Target::new(Who::Entity(hoglin_e), 0, hunt_slot));
    h.hit(hoglin_e, 1000.0, (6.5, 0.5));
    for _ in 1..=60i64 {
        h.tick();
    }
    let c = *h.world.get::<Celebrating>(p_e).expect("the ECS celebrates");
    let at = h.world.get::<Pos>(p_e).unwrap().0;
    let ecs_gap = ((at.x - c.spot[0]).powi(2) + (at.z - c.spot[2]).powi(2)).sqrt();
    assert!(ecs_gap <= 2.0, "the ECS walks to the spot: {ecs_gap}");
}

/// A kill is an event: the death of a hoglin wakes its targeters once, the list of kills is empty again the tick after, and a
/// tick without a death leaves it empty.
#[test]
fn a_kill_event_starts_the_party_of_the_targeter_once() {
    if !have_data() || wrong_species(&["minecraft:piglin"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let piglin = *table.iter().find(|s| s.name == "minecraft:piglin").unwrap();
    let hoglin = *table.iter().find(|s| s.name == "minecraft:hoglin").unwrap();
    let p_e = spawn_species(&mut h.world, &piglin, 0.5, 0.5, 11);
    let hoglin_e = spawn_species(&mut h.world, &hoglin, 6.5, 0.5, 13);
    for e in [p_e, hoglin_e] {
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.resource_mut::<Entered>().0.push(e);
    }
    // A target of a goal (as the hunt sets it): it ends with the goal, after the kill is heard; a target without a goal is cleared first.
    let hunt_slot = horizon_ecs_proto::goals::slot_of(&h.world, p_e, &horizon_ecs_proto::behaviours::hunt_prey::MODULE).unwrap();
    h.world.entity_mut(p_e).insert(Target::new(Who::Entity(hoglin_e), 0, hunt_slot));
    h.hit(hoglin_e, 1000.0, (6.5, 0.5));
    // The kill is written in the tick of the damage and heard in the next one (`Ev`: the events of the previous tick).
    h.tick();
    h.tick();
    assert!(h.world.get::<Celebrating>(p_e).is_some(), "the targeter of the victim celebrates at the death");
    h.tick();
    assert!(h.world.resource::<Events>().killed.is_empty(), "the kill list is drained each tick");
}

/// A piglin that sees gold in its off hand admires it (`Admiring` is set) within 20 ticks.
#[test]
fn admire_starts_on_the_gold_in_the_off_hand_as_a_mob_does() {
    if !have_data() || wrong_species(&["minecraft:piglin"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let p_e = piglin_at(&mut h, Vec3::new(0.5, 1.0, 0.5), 11);
    horizon_ecs_proto::equipment::set_slot(&mut h.world, p_e, horizon_ecs_proto::goals::Slot::Off, horizon_ecs_proto::items::item_id("minecraft:gold_ingot"));
    let mut ecs_at = None;
    for t in 1..=20i64 {
        h.tick();
        if ecs_at.is_none() && h.world.get::<Admiring>(p_e).is_some() {
            ecs_at = Some(t);
        }
    }
    assert!(ecs_at.is_some(), "the ECS admires the gold");
}

/// A baby hoglin grows up: in the ECS its `GrowAt` (the baby mark) is removed after its 200 ticks.
#[test]
fn a_baby_hoglin_loses_the_baby_when_it_grows_as_a_mob_does() {
    if !have_data() || wrong_species(&["minecraft:hoglin"]) {
        return;
    }
    const GROW: i32 = 200;
    let mut h = Herd::with_slab_awake(60, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let hoglin = *table.iter().find(|s| s.name == "minecraft:hoglin").unwrap();
    let e = spawn_species(&mut h.world, &hoglin, 0.5, 0.5, 13);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    horizon_ecs_proto::behaviours::follow_parent::make_baby(&mut h.world, e, GROW as u64);
    for _ in 0..=GROW {
        h.tick();
    }
    assert!(h.world.get::<GrowAt>(e).is_none(), "the ECS hoglin lost its baby mark");
}

/// A piglin looks at a player holding gold: the distance is to the player's feet (14 blocks, `range_sqr` 196), not to the eye. A player
/// 13.5 blocks away is looked at (`nearest_player_holding` finds the eye); 14.5 is not.
#[test]
fn look_at_loved_measures_to_the_feet_as_a_mob_does() {
    if !have_data() || wrong_species(&["minecraft:piglin"]) {
        return;
    }
    let gold = "minecraft:gold_ingot";
    for (dx, looked) in [(13.5, true), (14.5, false)] {
        let mut h = Herd::with_slab_awake(60, true, false);
        let p_e = piglin_at(&mut h, Vec3::new(0.5, 1.0, 0.5), 11);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5 + dx, y: 1.0, z: 0.5, main: horizon_ecs_proto::items::item_id("minecraft:gold_ingot"), off: 0, ..Default::default() });
        for _ in 1..=5i64 {
            h.tick();
        }
        let ecs_looks = horizon_ecs_proto::goals::nearest_player_holding(&h.world, p_e, &[gold], 14.0).is_some();
        assert_eq!(ecs_looks, looked, "the ECS at {dx} blocks");
    }
}
