//! The registry of the points of interest against Horizon's own classification (`horizon_world::poi`, which the `Mob` reads too) in the same
//! world: the points appear and go with the blocks, their tickets, the nearest first, the distance of a place to a village, the predicate.
//! `HORIZON_TEST_RESOURCES` set.
mod common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::poi::PoiRegistry;
use horizon_world::poi::PoiType;

fn place(h: &mut Herd, blocks: &[([i32; 3], &'static str)]) {
    for &(p, n) in blocks {
        h.set_block(p[0], p[1], p[2], n);
    }
    h.tick();
}

/// A job site, a bell and a lectern put in the world are points of interest of their type the tick after; taken away they go; the events say so.
#[test]
fn the_points_follow_the_blocks_and_say_so() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, true, false);
    h.tick();
    assert!(h.world.resource::<PoiRegistry>().is_empty());
    place(&mut h, &[([3, 1, 3], "minecraft:composter"), ([5, 1, 3], "minecraft:bell"), ([-4, 1, 6], "minecraft:lectern"), ([8, 1, 8], "minecraft:stone")]);
    h.tick();
    let r = h.world.resource::<PoiRegistry>();
    assert_eq!(r.len(), 3);
    assert_eq!(r.get([3, 1, 3]).unwrap().kind, PoiType::Farmer);
    assert_eq!(r.get([5, 1, 3]).unwrap().kind, PoiType::Meeting);
    assert_eq!(r.get([-4, 1, 6]).unwrap().kind, PoiType::Librarian);
    assert!(r.get([8, 1, 8]).is_none());
    assert_eq!(r.added().len(), 3, "the points that came: {:?}", r.added());
    place(&mut h, &[([3, 1, 3], "minecraft:air")]);
    h.tick();
    let r = h.world.resource::<PoiRegistry>();
    assert!(r.get([3, 1, 3]).is_none() && r.len() == 2);
    assert_eq!(r.removed(), &[[3, 1, 3]]);
}

/// A job site has one ticket, a bell 32: the second one to ask for the job site is refused; released, it can be taken again.
#[test]
fn the_tickets_are_the_types() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, true, false);
    place(&mut h, &[([3, 1, 3], "minecraft:composter"), ([5, 1, 3], "minecraft:bell")]);
    let mut r = h.world.resource_mut::<PoiRegistry>();
    assert!(r.acquire([3, 1, 3]));
    assert!(!r.acquire([3, 1, 3]), "a job site has one ticket");
    assert!((0..32).all(|_| r.acquire([5, 1, 3])));
    assert!(!r.acquire([5, 1, 3]), "a bell has 32");
    assert!(r.release([3, 1, 3]) && r.acquire([3, 1, 3]));
    assert!(r.in_range([3, 1, 3], 5, true, |_| true).is_empty(), "no point with a ticket free is left");
}

/// The points near a place, nearest first, and the distance of a place to a village in sections, as the sections of the points say it.
#[test]
fn the_search_and_the_distance_to_a_village() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(60, true, false);
    place(&mut h, &[([2, 1, 2], "minecraft:composter"), ([6, 1, 0], "minecraft:lectern"), ([-3, 1, -3], "minecraft:bell"), ([40, 1, 40], "minecraft:blast_furnace")]);
    let r = h.world.resource::<PoiRegistry>();
    let near = r.in_range([0, 1, 0], 10, false, |_| true);
    let kinds: Vec<_> = near.iter().map(|x| x.1).collect();
    assert_eq!(kinds, [PoiType::Farmer, PoiType::Meeting, PoiType::Librarian], "nearest first: {near:?}");
    assert_eq!(r.sections_to_village([0, 0, 0]), 0);
    assert_eq!(r.sections_to_village([3, 0, 0]), 2, "the blast furnace is in the section (2, 0, 2): two sections from (3, 0, 0)");
    assert!(r.is_village([20, 1, 20]), "a place one section from a village point is in the village");
    assert!(!r.is_village([-45, 1, -45]));
    assert_eq!(r.closest_section([3, 0, 3], 2), [2, 0, 2], "the section nearest a village that is nearer than where it stands");
}
