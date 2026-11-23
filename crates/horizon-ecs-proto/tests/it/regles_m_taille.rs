use crate::common::*;
use myriade_engine_lib::mobs::world::leaked_test_world_mut;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::cow::Body;
use horizon_ecs_proto::pose_state;
use horizon_ecs_proto::size::{self, SizeAnswer};

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

/// `EntityEvent.Size`: an `entity_size` answer replaces the box while the pose and vanilla's box hold; a change to either goes back to
/// vanilla's, and an answer that left the size unset changes nothing.
#[test]
fn a_size_answer_holds_until_the_pose_or_vanilla_size_changes() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let slime = h.spawn(mw, "minecraft:slime", [0.5, 1.0, 0.5], None).expect("a known species");
    let w = h.world_mut();
    let vanilla = size::held_box(w, slime).expect("a body");
    size::answer_size(w, slime, SizeAnswer { pose: 0, vanilla, size: Some([2.0, 3.0]) });
    assert_eq!(size::held_box(w, slime), Some([2.0, 3.0]), "the answer replaces the box");
    // A change of pose (a sleeping or swimming creature) is another pose: the answer no longer holds.
    pose_state::set(w, slime, Some(2));
    assert_eq!(size::held_box(w, slime), Some(vanilla), "another pose goes back to vanilla");
    pose_state::set(w, slime, None);
    assert_eq!(size::held_box(w, slime), Some([2.0, 3.0]), "back in the asked pose, it holds again");
    // A change of vanilla's size is another vanilla box: the answer no longer holds.
    size::set_size(w, slime, 2);
    let body = *w.get::<Body>(slime).expect("a body");
    assert_eq!(size::held_box(w, slime), Some([body.width, body.height]), "a new vanilla box goes back to vanilla's");
    // An answer that left the size unset changes nothing.
    let now = [body.width, body.height];
    size::answer_size(w, slime, SizeAnswer { pose: 0, vanilla: now, size: None });
    assert_eq!(size::held_box(w, slime), Some(now));
}
