//! `visual_clear_between` (the server AI's sight, `stare_freeze.rs:126`) clips with the VISUAL shape, not the collision shape: powder snow
//! has an empty visual shape, a four-layer snow block's outline reaches 0.5 (its collider 0.375), a fence's outline is its 16-pixel post,
//! a full block blocks both. Expected values come from the client's origin tests (vanilla `getVisualShape`), not from the code under test.
use crate::common;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::{Herd, Level};

/// `(visual, collider)` for a ray along the segment `from`..`to`, the block at (0, 1, 0) (one above the slab floor).
fn ray(block: &'static str, props: &'static [(&'static str, &'static str)], from: Vec3, to: Vec3) -> (bool, bool) {
    let mut h = Herd::with_slab_awake(8, true, false);
    h.set_block_state(0, 1, 0, block, props);
    h.tick();
    let lvl = h.world.resource::<Level>();
    (lvl.mw.0.visual_clear_between(from, to), lvl.mw.0.clear_between(from, to, false))
}

/// The ray along +z through the block at height `height` above its floor.
fn flat(block: &'static str, props: &'static [(&'static str, &'static str)], height: f64) -> (bool, bool) {
    ray(block, props, Vec3::new(0.5, 1.0 + height, -0.5), Vec3::new(0.5, 1.0 + height, 0.5))
}

#[test]
fn powder_snow_is_seen_through() {
    if !have_data() {
        return;
    }
    for height in [0.1, 0.5, 1.2, 1.4] {
        let (seen, _) = flat("minecraft:powder_snow", &[], height);
        assert!(seen, "seen through powder snow at y = {height}");
    }
}

#[test]
fn snow_hides_what_its_collision_shape_does_not() {
    if !have_data() {
        return;
    }
    let (seen, walked) = flat("minecraft:snow", &[("layers", "4")], 0.45);
    assert!(!seen, "the outline reaches 0.5");
    assert!(walked, "the collision shape only reaches 0.375");
    let (seen, walked) = flat("minecraft:snow", &[("layers", "4")], 0.6);
    assert!(seen && walked, "both shapes are below 0.6");
}

#[test]
fn a_fence_is_seen_through_above_its_post() {
    if !have_data() {
        return;
    }
    for height in [1.2, 1.45] {
        let (seen, walked) = flat("minecraft:oak_fence", &[], height);
        assert!(seen && walked, "no ray queries the fence at y = {height}");
    }
    let (seen, walked) = flat("minecraft:oak_fence", &[], 0.5);
    assert!(!seen && !walked, "the post blocks both rays");
    let (seen, walked) = ray("minecraft:oak_fence", &[], Vec3::new(0.5, 2.2, 0.3), Vec3::new(0.5, 1.95, 0.9));
    assert!(seen, "the outline stops at y = 1.0");
    assert!(!walked, "the collision shape reaches y = 1.5");
}

#[test]
fn a_full_block_blocks_sight() {
    if !have_data() {
        return;
    }
    for height in [0.5, 0.9] {
        let (seen, walked) = flat("minecraft:stone", &[], height);
        assert!(!seen && !walked, "stone blocks sight at y = {height}");
    }
}
