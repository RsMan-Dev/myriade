//! The scoreboard's teams as alliances (greffe lot 38): a member of a team is allied to the others, a creature does not pick a team
//! mate as its target, and leaving the team ends the alliance. The refusal follows the Mob's `p.allies.contains` (`goals.rs:955`).
use crate::common::*;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup, PlayerFact};
use horizon_ecs_proto::bridge::teams::{allied, clear_team, set_team};
use horizon_ecs_proto::goals::Player;
use horizon_ecs_proto::targeting::{can_target, Who};

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

#[test]
fn a_player_in_the_team_is_allied_until_the_team_is_cleared() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let player = Who::Player(3);
    h.set_player(Player { id: 3, x: 2.0, y: 1.0, z: 2.0, ..Default::default() }, PlayerFact { alive: true, targetable: true });
    assert!(!allied(h.world(), Who::Entity(cow), player), "no team yet: no alliance");
    set_team(h.world_mut(), "red", &[player, Who::Entity(cow)]);
    assert!(allied(h.world(), Who::Entity(cow), player), "two members of one team are allied");
    clear_team(h.world_mut(), "red");
    assert!(!allied(h.world(), Who::Entity(cow), player), "a cleared team allies nobody");
}

#[test]
fn a_member_that_moves_to_another_team_leaves_the_first() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let other = Who::Player(9);
    set_team(h.world_mut(), "red", &[Who::Entity(cow)]);
    set_team(h.world_mut(), "blue", &[Who::Entity(cow), other]);
    set_team(h.world_mut(), "red", &[]);
    assert!(allied(h.world(), Who::Entity(cow), other), "the cow moved to blue with the player");
    set_team(h.world_mut(), "blue", &[other]);
    assert!(!allied(h.world(), Who::Entity(cow), other), "the cow left blue when the team was set without it");
}

#[test]
fn a_creature_does_not_target_its_team_mate_and_does_again_when_the_team_ends() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let a = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let b = h.spawn(mw, "minecraft:cow", [2.5, 1.0, 0.5], None).expect("a known species");
    assert!(can_target(h.world(), a, Who::Entity(b)), "two creatures with no team: a target");
    set_team(h.world_mut(), "herd", &[Who::Entity(a), Who::Entity(b)]);
    assert!(!can_target(h.world(), a, Who::Entity(b)), "the Mob refuses its ally (goals.rs:955)");
    clear_team(h.world_mut(), "herd");
    assert!(can_target(h.world(), a, Who::Entity(b)), "the alliance ends with the team");
}
