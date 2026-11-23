use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_ecs_proto::behaviours::attached_face::Attached;
use horizon_ecs_proto::behaviours::teleport_on;
use horizon_ecs_proto::cow::*;
use horizon_core::geometry::Vec3;

/// The six neighbour steps, as `STEPS` of the projectile module (down, up, north, south, west, east).
const STEPS: [[i32; 3]; 6] = [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];

/// A shulker on the block (0, 0, 0) of a herd, its lid on `face`, with `block` placed at `at` (a block offset) and the block
/// of its own spot free of anything else. `None` without the vanilla data.
fn shulker(face: u8, blocks: &[([i32; 3], &'static str)]) -> Option<(Herd, Entity)> {
    if !have_data() {
        return None;
    }
    let mut h = Herd::with_slab_awake(60, false, false);
    for (p, name) in blocks {
        h.set_block(p[0], p[1], p[2], name);
    }
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:shulker").unwrap();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 13);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.entity_mut(e).insert(Attached { face, color: 0, raw: 0, eased: 0.0 });
    h.tick();
    Some((h, e))
}

/// `soul_sand_only_accepts_attachment_to_its_bottom`: the shulker on each of the six faces of the soul sand, and only the face
/// that has the soul sand above it (face 1) is accepted.
#[test]
fn soul_sand_only_accepts_attachment_to_its_bottom() {
    for (face, step) in STEPS.iter().enumerate() {
        let Some((h, e)) = shulker(face as u8, &[(*step, "minecraft:soul_sand")]) else {
            return;
        };
        assert_eq!(teleport_on::can_stay(&h.world, e), face == 1, "face {face}");
    }
}

/// `scaffolding_uses_the_shulkers_current_height_before_and_after_teleport`: scaffolding below and above the spot; the shulker
/// on the lower spot (height 62, its own cell) stays, and once teleported to height 64 it does not.
#[test]
fn scaffolding_uses_the_shulkers_current_height_before_and_after_teleport() {
    let Some((mut h, e)) = shulker(1, &[([0, -1, 0], "minecraft:scaffolding"), ([0, 1, 0], "minecraft:scaffolding")]) else {
        return;
    };
    h.teleport(e, Vec3::new(0.5, -2.0, 0.5));
    assert!(teleport_on::can_stay(&h.world, e));
    h.teleport(e, Vec3::new(0.5, 0.0, 0.5));
    assert!(!teleport_on::can_stay(&h.world, e));
}
