//! The door goals of a raider (`raider_open_door`, `interact_with_door`) against the `Mob`'s (`HORIZON_TEST_RESOURCES` set): a vindicator of an active
//! raid walks to the centre through a wall that only a door breaks: the door opens (positive witness), at the time the Mob opens it (6 per side).
use crate::common;
use common::*;
use horizon_client_lib::mobs::{self, MobEvent};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, goals::set_lazy_look, raids};

const WALL_Z: i32 = -6;
const HALF: i32 = 12;
fn walls(mut put: impl FnMut(i32, i32, i32)) {
    for x in -HALF..=HALF {
        if x != 0 {
            for y in 1..=3 {
                put(x, y, WALL_Z);
            }
        }
    }
}
fn mob_opens(id: u32) -> Option<usize> {
    let mut r = MobRef::new(100, false);
    walls(|x, y, z| r.set_block(x, y, z, "minecraft:stone"));
    r.set_block(0, 1, WALL_Z, "minecraft:oak_door");
    r.set_block(0, 2, WALL_Z, "minecraft:oak_door");
    let mut m = mobs::Mob::new(id, mob_kind("minecraft:vindicator"), Vec3::new(0.5, 1.0, -30.5), 0.0);
    m.persistent = true;
    m.ai.no_action_time = 100;
    scope(&mut m);
    m.ai.targets.clear();
    r.mobs.push(m);
    r.searches.push(0);
    for t in 0..900 {
        let s = mobs::raider::state_mut(&mut r.mobs[0]);
        s.active_raid = true;
        s.raid_center = [0, 2, 0];
        s.can_join_raid = true;
        s.raid_id = Some(1);
        let before = r.events.len();
        r.step(None);
        if r.events[before..].iter().any(|e| matches!(e, MobEvent::SetDoorOpen { open: true, .. })) {
            return Some(t);
        }
    }
    None
}
fn ecs_opens(seed: u64) -> Option<usize> {
    let mut h = Herd::with_slab(100, false, false);
    set_lazy_look(&mut h.world, false);
    h.set_block(0, 1, 0, "minecraft:bell");
    walls(|x, y, z| h.set_block(x, y, z, "minecraft:stone"));
    for (y, half) in [(1, "lower"), (2, "upper")] {
        h.set_block_state(0, y, WALL_Z, "minecraft:oak_door", Box::leak(Box::new([("half", half), ("facing", "north"), ("open", "false")])));
    }
    h.tick();
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:vindicator").unwrap();
    let e = spawn_species(&mut h.world, &sp, 0.5, -30.5, seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<horizon_ecs_proto::goals::Entered>().0.push(e);
    let id = raids::start(&mut h.world, [0, 2, 0], 5);
    h.world.entity_mut(e).insert(raids::Raider { raid: Some(id), wave: 1, leader: false });
    horizon_ecs_proto::behaviours::pathfind_to_raid::wake(&mut h.world, e);
    for t in 0..900 {
        h.tick();
        let mw = horizon_ecs_proto::blocks::world_of(&h.world);
        if horizon_ecs_proto::blocks::property(mw, [0, 1, WALL_Z], "open").as_deref() == Some("true") {
            return Some(t);
        }
    }
    None
}

#[test]
fn a_raider_opens_the_door_in_its_way_when_the_mob_does() {
    if !have_data() {
        return;
    }
    let m: Vec<Option<usize>> = (0..6).map(|k| mob_opens(7000 + 10 * k)).collect();
    let e: Vec<Option<usize>> = (0..6).map(|k| ecs_opens(60 + k)).collect();
    eprintln!("tick of the door opening: mob {m:?} ecs {e:?}");
    assert!(m.iter().all(Option::is_some), "the Mob's raiders open it");
    assert!(e.iter().all(Option::is_some), "the ECS raiders open it");
    let mean = |v: &[Option<usize>]| v.iter().map(|t| t.unwrap() as f64).sum::<f64>() / v.len() as f64;
    assert!((mean(&m) / mean(&e) - 1.0).abs() < 0.3, "{} {}", mean(&m), mean(&e));
}
