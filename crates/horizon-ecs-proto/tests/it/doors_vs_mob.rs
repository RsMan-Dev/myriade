//! The door goals of a raider (`raider_open_door`, `interact_with_door`) against the `Mob`'s (`HORIZON_TEST_RESOURCES` set): a vindicator of an active
//! raid walks to the centre through a wall that only a door breaks: the door opens (positive witness), at the time the Mob opens it (12 per side).
use crate::common;
use common::*;
use horizon_client_lib::mobs::{self, MobEvent, MobWorld};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, goals::set_lazy_look, raids};

/// The wall is three sections from the bell: a raider within one section of a village stops walking to the raid (vanilla `isVillage`), as the ECS side's registry
/// says (the Mob's test state has no village).
const WALL_Z: i32 = -37;
const START_Z: f64 = -37.5;
/// A pen of one block around the start, its only way out the door in the north wall: whichever place the raider draws to walk toward the raid
/// (`PathfindToRaidGoal` draws it at random), the way there goes through the door, and the draw cannot change the walk to it.
fn walls(mut put: impl FnMut(i32, i32, i32)) {
    for y in 1..=3 {
        for x in -1..=1 {
            if x != 0 {
                put(x, y, WALL_Z);
            }
            put(x, y, WALL_Z - 2);
        }
        put(-1, y, WALL_Z - 1);
        put(1, y, WALL_Z - 1);
    }
}
/// The tick each side opened the door in a run where the Mob's draws are imposed on the ECS: the Mob draws the places to walk to (`PathfindToRaidGoal`),
/// and each place its navigation heads for goes to the ECS raider as the next draw (`ImposedSpots`), so the two walk the same way and the tick of the
/// opening is compared, not the luck of the draw.
fn opens(id: u32, seed: u64) -> (Option<usize>, Option<usize>) {
    let mut r = MobRef::new(100, false);
    r.w = Box::leak(Box::new(MobWorld { mob_griefing: false, ..r.w.clone() }));
    walls(|x, y, z| r.set_block(x, y, z, "minecraft:stone"));
    r.set_block(0, 1, WALL_Z, "minecraft:oak_door");
    r.set_block(0, 2, WALL_Z, "minecraft:oak_door");
    let mut m = mobs::Mob::new(id, mob_kind("minecraft:vindicator"), Vec3::new(0.5, 1.0, START_Z), 0.0);
    m.persistent = true;
    m.ai.no_action_time = 100;
    scope(&mut m);
    m.ai.targets.clear();
    r.mobs.push(m);
    r.searches.push(0);

    let mut h = Herd::with_slab_awake(100, false, false);
    let mut level = *h.world.resource::<Level>();
    level.mw = Mw(Box::leak(Box::new(MobWorld { mob_griefing: false, ..level.mw.0.clone() })));
    *h.world.resource_mut::<Level>() = level;
    set_lazy_look(&mut h.world, false);
    h.set_block(0, 1, 0, "minecraft:bell");
    walls(|x, y, z| h.set_block(x, y, z, "minecraft:stone"));
    for (y, half) in [(1, "lower"), (2, "upper")] {
        h.set_block_state(0, y, WALL_Z, "minecraft:oak_door", Box::leak(Box::new([("half", half), ("facing", "north"), ("open", "false")])));
    }
    h.tick();
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:vindicator").unwrap();
    let e = spawn_species(&mut h.world, &sp, 0.5, START_Z, seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    // The `Mob` is born facing yaw 0, the ECS animal with a drawn one: the turn it must make before walking is not what is compared.
    h.world.entity_mut(e).insert(Yaw(0.0));
    if let Some(mut hd) = h.world.get_mut::<horizon_ecs_proto::goals::Head>(e) {
        hd.body = 0.0;
    }
    h.world.resource_mut::<horizon_ecs_proto::goals::Entered>().0.push(e);
    h.world.insert_resource(horizon_ecs_proto::behaviours::turtle_go_home::ImposedSpots::default());
    let rid = raids::start(&mut h.world, [0, 2, 0], 5);
    raids::enlist(&mut h.world, e, raids::Raider { raid: Some(rid), wave: 1, leader: false }, true);

    let (mut m_open, mut e_open, mut last_target) = (None, None, None);
    for t in 0..900 {
        let s = mobs::raider::state_mut(&mut r.mobs[0]);
        s.active_raid = true;
        s.raid_center = [0, 2, 0];
        s.can_join_raid = true;
        s.raid_id = Some(1);
        s.raided = true; // `game/mobs/raids`: a vindicator with a raid within 96 blocks
        let before = r.events.len();
        r.step(None);
        if m_open.is_none() && r.events[before..].iter().any(|e| matches!(e, MobEvent::SetDoorOpen { open: true, .. })) {
            m_open = Some(t);
        }
        let target = r.mobs[0].ai.nav.target_pos;
        if target != last_target && let Some(b) = target {
            h.world.resource_mut::<horizon_ecs_proto::behaviours::turtle_go_home::ImposedSpots>().0.push_back([f64::from(b[0]) + 0.5, f64::from(b[1]), f64::from(b[2]) + 0.5]);
        }
        last_target = target;
        h.tick();
        let mw = horizon_ecs_proto::blocks::world_of(&h.world);
        if e_open.is_none() && [1, 2].iter().any(|&y| horizon_ecs_proto::blocks::property(mw, [0, y, WALL_Z], "open").as_deref() == Some("true")) {
            e_open = Some(t);
        }
        if m_open.is_some() && e_open.is_some() {
            break;
        }
    }
    (m_open, e_open)
}

/// Both sides read the same game rules and walk the same places (the Mob's draws imposed), the raider is in a pen of one block whose only way out
/// is the door, `mob_griefing` is off (no door-breaking), and the door opens on both sides at the SAME tick, run by run.
/// Both start facing yaw 0 (the `Mob` is born so; the ECS animal draws one, and the turn it then makes before walking cost it up to two ticks: that
/// was the whole gap seen earlier, with `interact_with_door`, which the jar's vindicator does not have, opening the door before the contact).
#[test]
fn a_raider_opens_the_door_in_its_way_when_the_mob_does() {
    if !have_data() {
        return;
    }
    let runs: Vec<_> = (0..12).map(|k| opens(7000 + 10 * k, 60 + k as u64)).collect();
    eprintln!("tick of the door opening (mob, ecs): {runs:?}");
    for (m, e) in &runs {
        let (m, e) = (m.expect("the Mob's raider opens it"), e.expect("the ECS raider opens it"));
        assert_eq!(m, e, "{runs:?}");
    }
}
