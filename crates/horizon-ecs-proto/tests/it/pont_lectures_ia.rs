//! The AI reads of the host bridge (greffe L0, lot 41): the target, the last attacker, the owner, the restriction and the walk, read
//! by entity from the ECS world. The fixtures are those of `bridge_host`.
use crate::common::*;
use myriade_engine_lib::mobs::world::leaked_test_world_mut;
use horizon_ecs_proto::{
    behaviours::move_towards_restriction::Restriction,
    bridge::host::{EcsWorld, HostSetup, HurtSource, PlayerFact},
    goals::Player,
    tame,
    targeting::{Target, Who, clear_target, set_target},
};

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

fn fact() -> PlayerFact {
    PlayerFact { alive: true, targetable: true, hero: None, ..Default::default() }
}

fn player(id: u32, x: f64, z: f64) -> Player {
    Player { id, x, y: 1.0, z, ..Default::default() }
}

#[test]
fn target_is_read_then_cleared() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert_eq!(h.ai_target(e), None);
    set_target(h.world_mut(), e, Target::new(Who::Player(9), 20, 1));
    assert_eq!(h.ai_target(e), Some(Who::Player(9)));
    clear_target(h.world_mut(), e);
    assert_eq!(h.ai_target(e), None);
}

#[test]
fn last_attacker_is_read_after_a_hit() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    h.set_player(player(3, 1.0, 1.0), fact());
    let attacker = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let victim = h.spawn(mw, "minecraft:cow", [2.5, 1.0, 0.5], None).expect("a known species");
    let id = h.id_for(attacker, &mut || 50);
    h.hurt(mw, victim, 1.0, HurtSource::Entity(id));
    h.step(mw);
    assert_eq!(h.last_hurt_by(victim), Some(Who::Entity(attacker)));
}

#[test]
fn vex_owner_entity_is_read() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let owner = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let vex = h.spawn(mw, "minecraft:cow", [2.5, 1.0, 0.5], None).expect("a known species");
    tame::set_owner(h.world_mut(), vex, Who::Entity(owner));
    assert_eq!(h.vex_owner(vex), Some(owner));
}

#[test]
fn restriction_tells_inside_from_outside() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(!h.has_restriction(e));
    assert!(h.is_within_restriction(e, [40.5, 1.0, 40.5]), "no restriction: anywhere");
    h.world_mut().entity_mut(e).insert(Restriction { center: [0, 1, 0], radius: 6, wanted: None });
    assert!(h.has_restriction(e));
    assert_eq!(h.restrict_center(e), Some([0, 1, 0]));
    assert_eq!(h.restrict_radius(e), Some(6));
    assert!(h.is_within_restriction(e, [0.5, 1.0, 0.5]));
    assert!(!h.is_within_restriction(e, [20.5, 1.0, 0.5]));
}

#[test]
fn navigation_is_done_without_a_path() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.nav_is_done(e));
    assert_eq!(h.wanted(e), None);
}
