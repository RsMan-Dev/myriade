use crate::common::*;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::brain::behaviours::start_attacking::{resolve_target, TargetAnswer, TargetAsked};
use horizon_ecs_proto::targeting::{Target, Who};

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

/// `LivingChangeTargetEvent` for the brain's `StartAttacking`: the host's answer keeps the target out, or sets the one it names when that
/// one can be attacked; any other answer leaves the creature without a target.
#[test]
fn start_attacking_brain_honors_cancel_and_redirect() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let breeze = h.spawn(mw, "minecraft:breeze", [0.5, 1.0, 0.5], None).expect("a known species");
    let other = h.spawn(mw, "minecraft:iron_golem", [2.5, 1.0, 0.5], None).expect("a known species");
    let proposed = Who::Player(1);
    for (answer, target) in [
        (TargetAnswer::Keep, None),
        (TargetAnswer::Set(Some(Who::Entity(other))), Some(Who::Entity(other))),
        (TargetAnswer::Set(Some(Who::Player(999))), None),
    ] {
        let w = h.world_mut();
        w.entity_mut(breeze).remove::<Target>().insert(TargetAsked(proposed));
        resolve_target(w, breeze, answer);
        assert!(w.get::<TargetAsked>(breeze).is_none(), "the question is answered once");
        assert_eq!(w.get::<Target>(breeze).map(|t| t.who), target, "{answer:?}");
    }
}
