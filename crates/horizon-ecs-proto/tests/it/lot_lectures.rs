//! The piglin hunt's gate on a cooling piglin nearby, and the frog's leap while pregnant, against the real `Mob` (`HORIZON_TEST_RESOURCES`
//! set). Written without a run. A piglin with no cooldown and a piglin still cooling down for 1000 ticks stand two blocks apart, a hoglin
//! six blocks away: no piglin in range hunts until the cooling one is due, on the Mob and in the ECS alike.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::frog;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{behaviours::{admire::Admiring, breed::Carrying, celebrate::Celebrating, hunt_prey::{CannotHunt, Hunt}, walk_away::Avoid}, cow::*, goals::*, hostile::*, targeting::*};

#[test]
fn a_hunt_waits_for_the_cooldown_of_a_piglin_nearby_as_a_mob_does() {
    if !have_data() || wrong_species(&["minecraft:piglin"]) {
        return;
    }
    const COOLDOWN: i32 = 1000;
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab_awake(60, true, false);
    let hunter = r.add_id(Vec3::new(0.5, 1.0, 0.5), 11);
    let nearby = r.add_id(Vec3::new(2.5, 1.0, 0.5), 12);
    r.add_kind_id(Vec3::new(6.5, 1.0, 0.5), mob_kind("minecraft:hoglin"), 13);
    r.mobs[hunter].extra.ints[0] = 0;
    r.mobs[nearby].extra.ints[0] = COOLDOWN;

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

    let (mut mob_first, mut ecs_first) = (None, None);
    for t in 1..=1200i64 {
        r.step(None);
        h.tick();
        if mob_first.is_none() && [hunter, nearby].iter().any(|&i| r.mobs[i].ai.target.is_some()) {
            mob_first = Some(t);
        }
        if ecs_first.is_none() && [hunter_e, nearby_e].iter().any(|&e| h.world.get::<Target>(e).is_some()) {
            ecs_first = Some(t);
        }
    }
    let (m, c) = (mob_first.expect("the Mob hunts"), ecs_first.expect("the ECS hunts"));
    eprintln!("hunt after the nearby cooldown: mob {m} ecs {c}");
    assert!(m >= 900 && c >= 900, "no hunt while the piglin nearby cools down: Mob {m}, ECS {c}");
    assert!((m - c).abs() <= 5, "the hunt starts at the same tick: Mob {m}, ECS {c}");
}

/// A piglin saved with `CannotHunt` never takes a hoglin as target, on the Mob (`canHunt` false) and in the ECS (the piglin
/// carries the `CannotHunt` marker), 600 ticks with the hoglin six blocks away and the hunt due.
#[test]
fn a_piglin_that_cannot_hunt_takes_no_hoglin_as_a_mob_does() {
    if !have_data() || wrong_species(&["minecraft:piglin"]) {
        return;
    }
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab_awake(60, true, false);
    let hunter = r.add_id(Vec3::new(0.5, 1.0, 0.5), 11);
    r.add_kind_id(Vec3::new(6.5, 1.0, 0.5), mob_kind("minecraft:hoglin"), 13);
    r.mobs[hunter].extra.piglin.cannot_hunt = true;

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
        r.step(None);
        h.tick();
    }
    assert!(r.mobs[hunter].ai.target.is_none(), "the Mob with CannotHunt takes no target");
    assert!(h.world.get::<Target>(hunter_e).is_none(), "the ECS with CannotHunt takes no target");
}

/// The leaps (take-offs from the ground) of a frog on a 3 x 3 island in a ring of void, in 600 ticks, on the Mob and in the ECS, the frog
/// pregnant or not.
fn island_leaps(pregnant: bool) -> (usize, usize) {
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab_awake(60, true, false);
    for x in -3..=3i32 {
        for z in -3..=3i32 {
            if x.abs().max(z.abs()) >= 2 {
                for y in -3..=0 {
                    r.set_block(x, y, z, "minecraft:air");
                    h.set_block(x, y, z, "minecraft:air");
                }
            }
        }
    }
    let i = r.add_kind_id(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:frog"), 21);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let frog_sp = *table.iter().find(|s| s.name == "minecraft:frog").unwrap();
    let e = spawn_species(&mut h.world, &frog_sp, 0.5, 0.5, 21);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    if pregnant {
        frog::set_pregnant(&mut r.mobs[i], true);
        h.world.entity_mut(e).insert(Carrying);
    }
    let (mut mob_leaps, mut ecs_leaps) = (0usize, 0usize);
    let (mut mob_up, mut ecs_up) = (false, false);
    for _ in 0..600 {
        r.mobs[i].ai.no_action_time = 0;
        r.step(None);
        h.tick();
        let (a, b) = (r.mobs[i].position.y > 1.05, h.world.get::<Pos>(e).unwrap().0.y > 1.05);
        mob_leaps += usize::from(a && !mob_up);
        ecs_leaps += usize::from(b && !ecs_up);
        (mob_up, ecs_up) = (a, b);
    }
    (mob_leaps, ecs_leaps)
}

/// A frog that is not pregnant leaps off its island, and a pregnant one does not, on the Mob and in the ECS alike.
#[test]
fn a_pregnant_frog_does_not_leap_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let (free_mob, free_ecs) = island_leaps(false);
    assert!(free_mob > 0 && free_ecs > 0, "a frog that is not pregnant leaps: Mob {free_mob}, ECS {free_ecs}");
    let (mob, ecs) = island_leaps(true);
    assert_eq!((mob, ecs), (0, 0), "a pregnant frog does not leap");
}

/// Each piglin test below builds the same pair of worlds: the Mob (`MobRef`) and the ECS (`Herd`), the piglin at the same spot in both.
fn piglin_pair(r: &mut MobRef, h: &mut Herd, at: Vec3, seed: u64) -> (usize, Entity) {
    let i = r.add_id(at, seed as u32);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let piglin = *table.iter().find(|s| s.name == "minecraft:piglin").unwrap();
    let e = spawn_species(&mut h.world, &piglin, at.x, at.z, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    (i, e)
}

/// A piglin in Avoid (a point remembered to keep away from) takes no hoglin as target, on the Mob and in the ECS (`Avoid` is live).
#[test]
fn an_attacker_in_avoid_takes_no_hoglin_as_a_mob_does() {
    if !have_data() || wrong_species(&["minecraft:piglin"]) {
        return;
    }
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab_awake(60, true, false);
    let (hunter, hunter_e) = piglin_pair(&mut r, &mut h, Vec3::new(0.5, 1.0, 0.5), 11);
    r.add_kind_id(Vec3::new(6.5, 1.0, 0.5), mob_kind("minecraft:hoglin"), 13);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let hoglin = *table.iter().find(|s| s.name == "minecraft:hoglin").unwrap();
    let hoglin_e = spawn_species(&mut h.world, &hoglin, 6.5, 0.5, 13);
    h.world.get_mut::<ActiveUntil>(hoglin_e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(hoglin_e);
    let mut ttl = horizon_client_lib::mobs::brain::Ttl::default();
    ttl.set(300);
    r.mobs[hunter].extra.piglin.avoid = Some(horizon_client_lib::mobs::piglin::AvoidTarget {
        who: horizon_client_lib::mobs::Who::Mob(13),
        at: Vec3::new(3.5, 1.0, 0.5),
        ttl,
    });
    r.mobs[hunter].extra.piglin.activity = horizon_client_lib::mobs::piglin::Activity::Avoid;
    let now = now_of(&h.world, hunter_e);
    h.world.entity_mut(hunter_e).insert(horizon_ecs_proto::behaviours::walk_away::Avoid { at: [3.5, 1.0, 0.5], until: now + 300 });
    h.world.get_mut::<Hunt>(hunter_e).unwrap().until = now;
    for _ in 1..=300i64 {
        r.step(None);
        h.tick();
    }
    assert!(r.mobs[hunter].ai.target.is_none(), "the Mob in Avoid takes no target");
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
        let mut r = MobRef::new(60, false);
        let mut h = Herd::with_slab_awake(60, true, false);
        let (p, p_e) = piglin_pair(&mut r, &mut h, Vec3::new(0.5, 1.0, 0.5), 11);
        r.set_block_props(6, 1, dz, "minecraft:soul_campfire", &[("lit", "true")]);
        h.set_block(6, 1, dz, "minecraft:soul_campfire");
        for _ in 1..=40i64 {
            r.step(None);
            h.tick();
        }
        let mob_avoids = r.mobs[p].extra.piglin.activity == horizon_client_lib::mobs::piglin::Activity::Avoid;
        let ecs_avoids = h.world.get::<Avoid>(p_e).is_some();
        assert_eq!(mob_avoids, repelled, "the Mob, campfire at dz {dz}");
        assert_eq!(ecs_avoids, repelled, "the ECS, campfire at dz {dz}");
    }
}

/// A piglin whose target dies walks to the spot the target fell and stays within 2 blocks of it, on the Mob (`home`, `CELEBRATION`)
/// and in the ECS (`Celebrating`, started by the death event).
#[test]
fn celebrate_walks_to_the_kill_spot_as_a_mob_does() {
    if !have_data() || wrong_species(&["minecraft:piglin"]) {
        return;
    }
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab_awake(60, true, false);
    let (p, p_e) = piglin_pair(&mut r, &mut h, Vec3::new(0.5, 1.0, 0.5), 11);
    let hoglin = r.add_kind_id(Vec3::new(6.5, 1.0, 0.5), mob_kind("minecraft:hoglin"), 13);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let hoglin_sp = *table.iter().find(|s| s.name == "minecraft:hoglin").unwrap();
    let hoglin_e = spawn_species(&mut h.world, &hoglin_sp, 6.5, 0.5, 13);
    h.world.get_mut::<ActiveUntil>(hoglin_e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(hoglin_e);
    r.mobs[p].ai.target = Some(horizon_client_lib::mobs::Who::Mob(r.mobs[hoglin].id));
    h.world.entity_mut(p_e).insert(Target { who: Who::Entity(hoglin_e), ..Default::default() });
    r.hit(hoglin, 1000.0, [6.5, 1.0, 0.5]);
    h.hit(hoglin_e, 1000.0, (6.5, 0.5));
    for _ in 1..=60i64 {
        r.step(None);
        h.tick();
    }
    let spot = r.mobs[p].extra.home.expect("the Mob celebrates at a spot");
    let mob_gap = ((r.mobs[p].position.x - f64::from(spot[0]) - 0.5).powi(2) + (r.mobs[p].position.z - f64::from(spot[2]) - 0.5).powi(2)).sqrt();
    let c = *h.world.get::<Celebrating>(p_e).expect("the ECS celebrates");
    let at = h.world.get::<Pos>(p_e).unwrap().0;
    let ecs_gap = ((at.x - c.spot[0]).powi(2) + (at.z - c.spot[2]).powi(2)).sqrt();
    assert!(mob_gap <= 2.0, "the Mob walks to the spot: {mob_gap}");
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
    h.world.entity_mut(p_e).insert(Target { who: Who::Entity(hoglin_e), ..Default::default() });
    h.hit(hoglin_e, 1000.0, (6.5, 0.5));
    h.tick();
    assert!(h.world.get::<Celebrating>(p_e).is_some(), "the targeter of the victim celebrates at the death");
    h.tick();
    assert!(h.world.resource::<Events>().killed.is_empty(), "the kill list is drained each tick");
}

/// A piglin that sees gold in its off hand admires it: on the Mob (`Activity::Admire`) and in the ECS (`Admiring` is set) at the same tick.
#[test]
fn admire_starts_on_the_gold_in_the_off_hand_as_a_mob_does() {
    if !have_data() || wrong_species(&["minecraft:piglin"]) {
        return;
    }
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab_awake(60, true, false);
    let (p, p_e) = piglin_pair(&mut r, &mut h, Vec3::new(0.5, 1.0, 0.5), 11);
    r.mobs[p].extra.set_off_hand(Some("minecraft:gold_ingot".into()));
    r.mobs[p].extra.using_item = 1;
    let mut mob_at = None;
    let mut ecs_at = None;
    for t in 1..=20i64 {
        r.step(None);
        h.tick();
        if mob_at.is_none() && r.mobs[p].extra.piglin.activity == horizon_client_lib::mobs::piglin::Activity::Admire {
            mob_at = Some(t);
        }
        if ecs_at.is_none() && h.world.get::<Admiring>(p_e).is_some() {
            ecs_at = Some(t);
        }
    }
    assert!(mob_at.is_some(), "the Mob admires the gold");
    assert!(ecs_at.is_some(), "the ECS admires the gold");
    assert!((mob_at.unwrap() - ecs_at.unwrap()).abs() <= 1, "the admiration starts at the same tick");
}

/// A baby hoglin grows up as a Mob does: its `age` climbs to 0 and it is no longer a baby; in the ECS its `GrowAt` (the baby mark) is removed.
#[test]
fn a_baby_hoglin_loses_the_baby_when_it_grows_as_a_mob_does() {
    if !have_data() || wrong_species(&["minecraft:hoglin"]) {
        return;
    }
    const GROW: i32 = 200;
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab_awake(60, true, false);
    let hog = r.add_kind_id(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:hoglin"), 13);
    r.mobs[hog].age = -GROW;
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let hoglin = *table.iter().find(|s| s.name == "minecraft:hoglin").unwrap();
    let e = spawn_species(&mut h.world, &hoglin, 0.5, 0.5, 13);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    horizon_ecs_proto::behaviours::follow_parent::make_baby(&mut h.world, e, GROW as u64);
    for _ in 0..=GROW {
        r.step(None);
        h.tick();
    }
    assert!(!r.mobs[hog].is_baby(), "the Mob's baby hoglin has grown");
    assert!(h.world.get::<GrowAt>(e).is_none(), "the ECS hoglin lost its baby mark");
}

/// A piglin looks at a player holding gold: the distance is to the player's feet (14 blocks, `range_sqr` 196), not to the eye. A player
/// 13.5 blocks away is looked at on the Mob (`LookAtLoved` runs) and in the ECS (`nearest_player_holding` finds the eye); 14.5 is not.
#[test]
fn look_at_loved_measures_to_the_feet_as_a_mob_does() {
    if !have_data() || wrong_species(&["minecraft:piglin"]) {
        return;
    }
    let gold = "minecraft:gold_ingot";
    for (dx, looked) in [(13.5, true), (14.5, false)] {
        let mut r = MobRef::new(60, false);
        let mut h = Herd::with_slab_awake(60, true, false);
        let (p, p_e) = piglin_pair(&mut r, &mut h, Vec3::new(0.5, 1.0, 0.5), 11);
        let player = player_view(Vec3::new(0.5 + dx, 1.0, 0.5), Some(gold));
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5 + dx, y: 1.0, z: 0.5, main: gold_ingot_id(), off: 0, ..Default::default() });
        for _ in 1..=5i64 {
            r.step(Some(&player));
            h.tick();
        }
        let mob_looks = r.names(p).iter().any(|n| *n == "LookAtLoved");
        let ecs_looks = horizon_ecs_proto::goals::nearest_player_holding(&h.world, p_e, &[gold], 14.0).is_some();
        assert_eq!(mob_looks, looked, "the Mob at {dx} blocks");
        assert_eq!(ecs_looks, looked, "the ECS at {dx} blocks");
    }
}
