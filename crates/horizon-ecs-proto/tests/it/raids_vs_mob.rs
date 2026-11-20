//! The raids socle: the `Mob`'s own raid state machine (`horizon_client_lib::raid`) driven over the ECS world. A village put down in blocks (a
//! bell, job blocks), a raid at its centre: the countdown, a wave that spawns far away and walks to the village (`PathfindToRaid`); and the walk
//! itself against the `Mob`'s `PathfindToRaidGoal` (time to come within 45 blocks of the centre, over 12 raiders per side).
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, goals::set_lazy_look, poi::PoiRegistry, raids};

pub const POINTS: [([i32; 3], &str); 4] = [([0, 1, 0], "minecraft:bell"), ([4, 1, 0], "minecraft:composter"), ([-4, 1, 0], "minecraft:lectern"), ([0, 1, 6], "minecraft:blast_furnace")];

pub fn village_herd(half: i32) -> Herd {
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

/// The time a raider placed `far` blocks away takes to come within `NEAR` blocks of the centre: the Mob's goal and the module.
/// "Near" is 45 blocks. `PathfindToRaidGoal.canUse` and `canContinue` both end on `!level.isVillage(blockPosition)` (javap, server-26.3.jar): a
/// raider within one section (16 blocks) of the village stops walking to the raid near x = 32, so 30 blocks is never reached, by the Mob either (and on a diagonal it stops at 41.7 blocks, Mob run 6100, hence 45):
/// the Mob reaches it only when its `in_village` flag stays false (the flag is the game's to set). Here the Mob gets the flag from the same
/// village the ECS side stands in, so both stop at the same place and the same distance is measured on both.
const NEAR: f64 = 45.0;
/// One run, both sides in lockstep: the Mob draws the places to walk to (`PathfindToRaidGoal`) and each place its navigation heads for goes to the ECS
/// raider as its next draw (`ImposedSpots`); both are born facing yaw 0. The tick each side comes within `NEAR` blocks of the centre.
fn arrivals(id: u32, seed: u64, far: f64) -> (usize, usize) {
    let village = village_herd(110);
    let registry = village.world.resource::<PoiRegistry>();
    let mut r = MobRef::new(100, false);
    let mut m = mobs::Mob::new(id, mob_kind("minecraft:vindicator"), Vec3::new(far, 1.0, 0.5), 0.0);
    m.persistent = true;
    m.ai.no_action_time = 100;
    scope(&mut m);
    m.ai.targets.clear();
    r.mobs.push(m);
    r.searches.push(0);

    let mut h = village_herd(110);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:vindicator").unwrap();
    let e = spawn_species(&mut h.world, &sp, far, 0.5, seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    h.world.entity_mut(e).insert(Yaw(0.0));
    if let Some(mut hd) = h.world.get_mut::<horizon_ecs_proto::goals::Head>(e) {
        hd.body = 0.0;
    }
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<horizon_ecs_proto::goals::Entered>().0.push(e);
    h.world.insert_resource(horizon_ecs_proto::behaviours::turtle_go_home::ImposedSpots::default());
    let rid = raids::start(&mut h.world, [0, 2, 0], 5);
    raids::enlist(&mut h.world, e, raids::Raider { raid: Some(rid), wave: 1, leader: false }, true);

    let (mut m_at, mut e_at, mut last_target) = (None, None, None);
    for t in 0..2000 {
        let p = r.mobs[0].position;
        if m_at.is_none() && (p.x.powi(2) + p.z.powi(2)).sqrt() < NEAR {
            m_at = Some(t);
        }
        let q = h.world.get::<Pos>(e).unwrap().0;
        if e_at.is_none() && (q.x.powi(2) + q.z.powi(2)).sqrt() < NEAR {
            e_at = Some(t);
        }
        if m_at.is_some() && e_at.is_some() {
            break;
        }
        let s = mobs::raider::state_mut(&mut r.mobs[0]);
        s.active_raid = true;
        s.raid_center = [0, 2, 0];
        s.can_join_raid = true;
        s.raid_id = Some(1);
        s.in_village = registry.is_village([p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]);
        r.step(None);
        let target = r.mobs[0].ai.nav.target_pos;
        if target != last_target && let Some(b) = target {
            h.world.resource_mut::<horizon_ecs_proto::behaviours::turtle_go_home::ImposedSpots>().0.push_back([f64::from(b[0]) + 0.5, f64::from(b[1]), f64::from(b[2]) + 0.5]);
        }
        last_target = target;
        h.tick();
    }
    (m_at.unwrap_or(2000), e_at.unwrap_or(2000))
}

#[test]
fn the_walk_to_the_raid_takes_the_time_of_the_mob_s() {
    if !have_data() {
        return;
    }
    let runs: Vec<_> = (0..12).map(|k| arrivals(6000 + 10 * k, 40 + k as u64, 70.5)).collect();
    eprintln!("ticks to come within {NEAR} blocks (mob, ecs): {runs:?}");
    for (m, e) in &runs {
        assert!(*m < 2000 && *e < 2000, "both come: {runs:?}");
        assert_eq!(m, e, "{runs:?}");
    }
}
