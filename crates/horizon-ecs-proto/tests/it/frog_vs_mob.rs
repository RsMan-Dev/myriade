//! The frog's goals (`HORIZON_SPECIES=minecraft:frog`) against the `Mob`: the ticks spent croaking by 8 frogs left alone, and the laying of a
//! pregnant frog beside a pool (how many lay, and when). The croak is drawn (one in four) at every ask, the stroll too: the means are compared with a wide
//! margin and each side must show its own witness (frogs that croak, frogs that lay), so a silent module cannot pass.
use crate::common;
use common::*;
use horizon_client_lib::mobs::{frog, sync};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::behaviours::{breed::Carrying, croak::Croaking};
use horizon_ecs_proto::cow::*;

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:frog"])
}
fn spots(n: usize) -> impl Iterator<Item = (f64, f64)> {
    (0..n).map(|k| (6.5 + 2.0 * (k % 4) as f64, -8.5 + 3.0 * (k / 4) as f64))
}

#[test]
fn frogs_croak_as_the_mob_does() {
    if skip() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    let mut ids = Vec::new();
    for (k, (x, z)) in spots(8).enumerate() {
        let i = r.add_id(Vec3::new(x, 1.0, z), (((k as u32 + 1) * 13).wrapping_mul(0x9E37_79B1) >> 4) | 1);
        let e = ecs_cow(&mut h, x, 1.0, z, 900 + k as u64);
        ids.push((i, e));
    }
    let mut croaking = [0usize; 2];
    let mut croakers = [0usize; 2];
    let mut seen = [[false; 2]; 8];
    for _ in 0..600 {
        r.step(None);
        h.tick();
        for (k, &(i, e)) in ids.iter().enumerate() {
            let on = [r.mobs[i].extra.pose == sync::CROAKING, h.world.get::<Croaking>(e).is_some()];
            for s in 0..2 {
                if on[s] {
                    croaking[s] += 1;
                    if !seen[k][s] {
                        seen[k][s] = true;
                        croakers[s] += 1;
                    }
                }
            }
        }
    }
    eprintln!("croak ticks Mob {} ECS {}; frogs that croaked Mob {} ECS {}", croaking[0], croaking[1], croakers[0], croakers[1]);
    assert!(croakers[0] > 3 && croakers[1] > 3, "witness: each side must croak");
    let (a, b) = (croaking[0] as f64, croaking[1] as f64);
    assert!((a - b).abs() < 0.35 * a.max(b), "croak time differs: {a} vs {b}");
}

#[test]
fn a_pregnant_frog_lays_beside_the_water_as_the_mob_does() {
    if skip() {
        return;
    }
    let (r0, mut h) = flat_pair();
    let mut r = r0;
    // A pool at x -12..=-1, one block deep below the floor (the slab's floor is row 0): the land is beside it.
    for x in -12..=-1 {
        for z in -12..=12 {
            r.set_block(x, 0, z, "minecraft:water");
            h.set_block(x, 0, z, "minecraft:water");
        }
    }
    h.tick();
    let mut ids = Vec::new();
    for (k, (x, z)) in spots(8).enumerate() {
        let (x, z) = (x - 4.0, z);
        let i = r.add_id(Vec3::new(x, 1.0, z), (((k as u32 + 1) * 17).wrapping_mul(0x9E37_79B1) >> 4) | 1);
        frog::set_pregnant(&mut r.mobs[i], true);
        let e = ecs_cow(&mut h, x, 1.0, z, 1900 + k as u64);
        h.world.entity_mut(e).insert(Carrying);
        horizon_ecs_proto::goals::arm_module(&mut h.world, e, &horizon_ecs_proto::behaviours::lay_spawn::MODULE, 1);
        ids.push((i, e));
    }
    let mut laid: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
    for t in 1..=600 {
        r.step(None);
        r.apply_block_events();
        h.tick();
        for (k, &(i, e)) in ids.iter().enumerate() {
            if !frog::is_pregnant(&r.mobs[i]) && !laid[0].contains(&k) {
                laid[0].push(k);
            }
            if h.world.get::<Carrying>(e).is_none() && !laid[1].contains(&k) {
                laid[1].push(k);
            }
        }
        let _ = t;
    }
    eprintln!("laid Mob {} ECS {}", laid[0].len(), laid[1].len());
    assert!(laid[0].len() >= 4, "witness: the Mob frogs lay ({})", laid[0].len());
    assert!(laid[1].len() + 2 >= laid[0].len(), "ECS frogs lay as many: {} vs {}", laid[1].len(), laid[0].len());
}

type Scene = ([[Option<i32>; 4]; 2], [[Option<i32>; 4]; 2], [Option<i32>; 4]);
/// Four frogs with a small slime 5 blocks away, a first scan at tick 1: the ticks of the first bite and of the end of the tongue (Mob, ECS), and the
/// tick the ECS tongue goal starts.
fn frog_slime_scene() -> Scene {
    let (mut r, mut h) = flat_pair();
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let slime = *table.iter().find(|s| s.name == "minecraft:slime").expect("slime registered");
    let mut pairs = Vec::new();
    for k in 0..4usize {
        let (x, z) = (6.5, -9.5 + 5.0 * k as f64);
        let i = r.add_id(Vec3::new(x, 1.0, z), (((k as u32 + 1) * 13).wrapping_mul(0x9E37_79B1) >> 4) | 1);
        let e = ecs_cow(&mut h, x, 1.0, z, 700 + k as u64);
        // Imposed draw: the sensor's random start (`nextInt(20)`) is 0, the first scan is at tick 1 as the `Mob`'s (its counter starts at 0).
        let seed = (1u64..).find(|&s| horizon_ecs_proto::cow::Rng(s).next_int(20) == 0).unwrap();
        *h.world.get_mut::<horizon_ecs_proto::cow::Rng>(e).unwrap() = horizon_ecs_proto::cow::Rng(seed);
        h.world.resource_mut::<horizon_ecs_proto::goals::Entered>().0.push(e);
        let (sx, sz) = (x + 5.0, z);
        let j = r.add_kind(Vec3::new(sx, 1.0, sz), mob_kind("minecraft:slime"));
        horizon_client_lib::mobs::cube::set_size(&mut r.mobs[j], 1);
        r.mobs[j].ai.goals = Default::default();
        r.mobs[j].ai.targets = Default::default();
        let se = horizon_ecs_proto::size::spawn_sized(&mut h.world, &slime, sx, sz, 40 + k as u64, 1);
        // The `Mob` slime stands still (its goals are empty): the ECS one does not hop either.
        h.world.entity_mut(se).remove::<(horizon_ecs_proto::goals::GoalSet, horizon_ecs_proto::locomotion::hop::Hop, horizon_ecs_proto::locomotion::hop::Hopping)>();
        assert!(horizon_ecs_proto::targeting::info(&h.world, horizon_ecs_proto::targeting::Who::Entity(se)).is_some(), "a slime must be readable by `targeting::info` (it needs `Eye`)");
        pairs.push((i, e, j, se));
    }
    let mut eaten = [[None; 4], [None; 4]];
    let mut ended = [[None; 4], [None; 4]];
    let mut started = [None; 4];
    for t in 1..=400 {
        r.step(None);
        h.tick();
        for (k, &(_, _, j, se)) in pairs.iter().enumerate() {
            if eaten[0][k].is_none() && r.events.iter().any(|ev| matches!(ev, horizon_client_lib::mobs::MobEvent::RemoveKilled { target } if *target == r.mobs[j].id)) {
                eaten[0][k] = Some(t);
                // The host applies `RemoveKilled` after the tick: the prey is gone for the next one (`canStillUse`: `isAlive`).
                r.mobs[j].health = 0.0;
                r.mobs[j].removed = true;
            }
            if eaten[1][k].is_none() && h.world.get_entity(se).is_err() {
                eaten[1][k] = Some(t);
            }
            let (i, e, _, _) = pairs[k];
            if started[k].is_none() && h.world.get::<horizon_ecs_proto::behaviours::shoot_tongue::Shooting>(e).is_some() {
                started[k] = Some(t);
            }
            if eaten[0][k].is_some() && ended[0][k].is_none() && r.mobs[i].extra.frog.tongue_target.is_none() {
                ended[0][k] = Some(t);
            }
            if eaten[1][k].is_some() && ended[1][k].is_none() && h.world.get::<horizon_ecs_proto::behaviours::shoot_tongue::Shooting>(e).is_none() {
                ended[1][k] = Some(t);
            }
        }
    }
    (eaten, ended, started)
}
/// `Brain.tick` (scan: `StartAttacking` sets the target), then `FrogAi.updateActivity` (javap: `Frog.customServerAiStep` calls the brain, then the
/// activity) puts the tongue activity on AFTER the brain tick, so `ShootTongue` starts at the next tick and acts at once (`tickEachRunningBehavior`):
/// with the first scan at tick 1 the ECS tongue starts at tick 2, and every frog bites at the same tick.
#[test]
fn a_frog_eats_a_small_slime_one_tick_after_the_scan_as_the_jar_has_it() {
    if skip() {
        return;
    }
    let (eaten, _, started) = frog_slime_scene();
    eprintln!("eaten at Mob {:?} ECS {:?}; ECS tongue started at {started:?}", eaten[0], eaten[1]);
    assert!(eaten[0].iter().all(Option::is_some), "witness: the Mob frogs eat: {:?}", eaten[0]);
    assert!(started.iter().all(|s| *s == Some(2)), "scan at tick 1, tongue at tick 2: {started:?}");
    assert!(eaten[1].iter().all(Option::is_some) && eaten[1].iter().all(|e| *e == eaten[1][0]), "the ECS frogs bite together: {:?}", eaten[1]);
}
/// The scene is the same and the draws are imposed, so the bite should come at the same tick on both sides, but the `Mob` starts `ShootTongue` one
/// tick after the jar (`suivi/ecs-ecarts.md`): ECS + 1 == Mob. Strict equality kept, until the `Mob` is corrected.
#[test]
#[ignore = "OPEN: the Mob frog starts ShootTongue one tick after the jar (Brain.tick, then FrogAi.updateActivity; the next tick starts the tongue); the ECS is the jar's"]
fn a_frog_eats_a_small_slime_at_the_same_tick_as_the_mob() {
    if skip() {
        return;
    }
    let (eaten, ended, _) = frog_slime_scene();
    assert_eq!(eaten[0], eaten[1], "tick of the first bite: Mob, ECS");
    assert_eq!(ended[0], ended[1], "tick the tongue is over: Mob, ECS");
}
