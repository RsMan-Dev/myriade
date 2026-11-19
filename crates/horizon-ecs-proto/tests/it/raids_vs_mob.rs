//! The raids socle: the `Mob`'s own raid state machine (`horizon_client_lib::raid`) driven over the ECS world. A village put down in blocks (a
//! bell, job blocks), a raid at its centre: the countdown, a wave that spawns far away and walks to the village (`PathfindToRaid`); and the walk
//! itself against the `Mob`'s `PathfindToRaidGoal` (time to come within 30 blocks of the centre, over 12 raiders per side).
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, goals::set_lazy_look, poi::PoiRegistry, raids};

const POINTS: [([i32; 3], &str); 4] = [([0, 1, 0], "minecraft:bell"), ([4, 1, 0], "minecraft:composter"), ([-4, 1, 0], "minecraft:lectern"), ([0, 1, 6], "minecraft:blast_furnace")];

fn village_herd(half: i32) -> Herd {
    let mut h = Herd::with_slab(half, false, false);
    set_lazy_look(&mut h.world, false);
    for (p, n) in POINTS {
        h.set_block(p[0], p[1], p[2], n);
    }
    h.tick();
    h
}

/// A raid at the village: after the countdown a wave appears around it (a ring of tens of blocks), and its raiders walk in.
#[test]
fn a_wave_walks_to_the_village() {
    if !have_data() {
        return;
    }
    let mut h = village_herd(160);
    assert!(h.world.resource::<PoiRegistry>().is_village([0, 0, 0]), "the village is in the registry");
    let id = raids::start(&mut h.world, [0, 2, 0], 77);
    let mut first_spawn = None;
    let mut dist_at = Vec::new();
    for t in 0..2600 {
        h.tick();
        let ps: Vec<f64> = h.world.query::<(&raids::Raider, &Pos)>().iter(&h.world).map(|(_, p)| (p.0.x.powi(2) + p.0.z.powi(2)).sqrt()).collect();
        if first_spawn.is_none() && !ps.is_empty() {
            first_spawn = Some(t);
        }
        if t % 400 == 0 && !ps.is_empty() {
            dist_at.push((t, ps.len(), ps.iter().sum::<f64>() / ps.len() as f64));
        }
    }
    eprintln!("raid {id}: first raiders at tick {first_spawn:?}; (tick, raiders, mean distance) {dist_at:?}");
    let first = first_spawn.expect("a wave spawned");
    assert!(first >= 250, "the countdown comes first: {first}");
    let last = dist_at.last().unwrap();
    assert!(dist_at[0].2 > 30.0, "they start far: {dist_at:?}");
    assert!(last.2 < 40.0, "and come to the village: {dist_at:?}");
}

/// The time a raider placed `far` blocks away takes to come within 30 blocks of the centre: the Mob's goal and the module.
fn mob_arrival(id: u32, far: f64) -> usize {
    let mut r = MobRef::new(100, false);
    let mut m = mobs::Mob::new(id, mob_kind("minecraft:vindicator"), Vec3::new(far, 1.0, 0.5), 0.0);
    m.persistent = true;
    m.ai.no_action_time = 100;
    scope(&mut m);
    m.ai.targets.clear();
    r.mobs.push(m);
    r.searches.push(0);
    for t in 0..2000 {
        let p = r.mobs[0].position;
        let near = (p.x.powi(2) + p.z.powi(2)).sqrt() < 30.0;
        if near {
            return t;
        }
        let s = mobs::raider::state_mut(&mut r.mobs[0]);
        s.active_raid = true;
        s.raid_center = [0, 2, 0];
        s.can_join_raid = true;
        s.raid_id = Some(1);
        r.step(None);
    }
    2000
}
fn ecs_arrival(seed: u64, far: f64) -> usize {
    let mut h = village_herd(110);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:vindicator").unwrap();
    let e = spawn_species(&mut h.world, &sp, far, 0.5, seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<horizon_ecs_proto::goals::Entered>().0.push(e);
    let id = raids::start(&mut h.world, [0, 2, 0], 5);
    h.world.entity_mut(e).insert(raids::Raider { raid: Some(id), wave: 1, leader: false });
    horizon_ecs_proto::behaviours::pathfind_to_raid::wake(&mut h.world, e);
    for t in 0..2000 {
        h.tick();
        let p = h.world.get::<Pos>(e).unwrap().0;
        if (p.x.powi(2) + p.z.powi(2)).sqrt() < 30.0 {
            return t;
        }
    }
    2000
}

#[test]
fn the_walk_to_the_raid_takes_the_time_of_the_mob_s() {
    if !have_data() {
        return;
    }
    let m: Vec<f64> = (0..12).map(|k| mob_arrival(6000 + 10 * k, 70.5) as f64).collect();
    let e: Vec<f64> = (0..12).map(|k| ecs_arrival(40 + k, 70.5) as f64).collect();
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    eprintln!("ticks to come within 30 blocks (mean): mob {:.0} ecs {:.0}; mob {m:?} ecs {e:?}", mean(&m), mean(&e));
    assert!(m.iter().all(|&t| t < 2000.0), "the Mob's raiders come");
    assert!(e.iter().all(|&t| t < 2000.0), "the ECS raiders come");
    assert!((mean(&m) / mean(&e) - 1.0).abs() < 0.3, "{} {}", mean(&m), mean(&e));
}
