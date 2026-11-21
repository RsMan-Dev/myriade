//! The guard alert (`PiglinAi.angerNearbyPiglins`) and the player credit (`LivingEntity.lastHurtByPlayer`, 100 ticks), written, not run.
use crate::common;
use common::*;
use horizon_ecs_proto::{cow::*, credit, guard_alert, targeting::{Target, Who}};

fn piglin(h: &mut Herd, x: f64, z: f64) -> bevy_ecs::prelude::Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:piglin").unwrap();
    spawn_species(&mut h.world, &sp, x, z, 5)
}

/// A piglin at rest within 16 blocks of the player turns on it; one already fighting something keeps its target.
#[test]
fn an_idle_piglin_near_the_player_is_alerted_and_a_busy_one_is_not() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    h.set_players(vec![(3.5, 0.5)]);
    let idle = piglin(&mut h, 0.5, 0.5);
    let busy = piglin(&mut h, 1.5, 0.5);
    horizon_ecs_proto::targeting::set_target(&mut h.world, busy, Target::new(Who::Entity(idle), 0, 0));
    assert!(guard_alert::at_rest(&h.world, idle) && !guard_alert::at_rest(&h.world, busy));
    let n = guard_alert::alert(&mut h.world, 0, false);
    assert_eq!(n, 1);
    assert_eq!(h.world.get::<Target>(idle).map(|t| t.who), Some(Who::Player(0)));
    assert_eq!(h.world.get::<Target>(busy).map(|t| t.who), Some(Who::Entity(idle)));
}

/// A hit by a player credits it; a hit by nobody adds no credit.
#[test]
fn a_player_hit_credits_the_player_only() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = piglin(&mut h, 0.5, 0.5);
    credit::record(&mut h.world, e, None);
    assert!(!credit::killed_by_player(&h.world, e));
    credit::record(&mut h.world, e, Some(Who::Player(9)));
    assert!(credit::killed_by_player(&h.world, e));
    assert_eq!(credit::player_of(&h.world, e), Some(9));
}
