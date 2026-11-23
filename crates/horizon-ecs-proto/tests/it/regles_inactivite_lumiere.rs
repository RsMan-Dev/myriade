//! Règle de l'inactivité selon la lumière, reprise de `monster_no_action_tests.rs` : `Monster.updateNoActionTime` compte 3 par
//! tick dans un endroit plus clair que 0.5 (blaze, vex : toujours), 1 dans le noir. Le compte est ÉCHANTILLONNÉ aux échéances du
//! cercle (pas de coût par tick) : après 150 ticks, le premier échantillon (tick 100) a crédité 3 × 100 (clair) ou 1 × 100 (noir).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::despawn::{admit, idle_for, rate_light, DespawnDef};

/// A creature with the rule of a monster that goes away, its light count set as `lit` says; the count after 150 ticks.
fn count_after_150(day: bool, lit: bool) -> Option<i32> {
    let mut h = Herd::with_slab_awake(8, true, false);
    h.set_daylight(day);
    let e = ecs_cow(&mut h, 0.5, 1.0, 0.5, 3);
    admit(&mut h.world, e, DespawnDef::MOB, 1e9);
    rate_light(&mut h.world, e, lit);
    for _ in 0..150 {
        h.tick();
    }
    idle_for(&h.world, e)
}

#[test]
fn a_blaze_counts_three_a_tick_whatever_the_light() {
    if !have_data() {
        return;
    }
    assert_eq!(count_after_150(false, true), Some(300), "lit = always 3 a tick (sampled at tick 100)");
}

#[test]
fn a_zombie_in_the_dark_counts_one_a_tick() {
    if !have_data() {
        return;
    }
    assert_eq!(count_after_150(false, false), Some(100), "dark: 1 a tick, sampled at tick 100");
}
