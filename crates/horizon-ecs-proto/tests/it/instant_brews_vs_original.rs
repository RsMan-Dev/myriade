//! The instantaneous brews against the client's `instant` (`horizon-client-render/src/effects.rs`, `HealOrHarmMobEffect`): a splash that
//! hits a creature harms it `6 << level` whole, and a lingering cloud harms at half strength. The origin's amounts are transcribed below
//! for levels 0, 1 and 2 (the cloud takes its brew from `POTIONS`, which has level 0 only).
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    cow::{DamageQueue, Herd},
    cloud_tick,
    effect_cloud::CloudSpec,
    effects::{self, Brew},
};

/// `instant` of the client at full strength (a splash's hit creature) and at half strength (a cloud), for `base << level`.
fn origin_splash(level: i32) -> f32 {
    (1.0 * f64::from(6i32 << level) + 0.5) as i32 as f32
}
fn origin_cloud(level: i32) -> f32 {
    (0.5 * f64::from(6i32 << level) + 0.5) as i32 as f32
}

/// The damage queued to `e` since the last call, then cleared.
fn queued(h: &mut Herd, e: Entity) -> Vec<f32> {
    let hits = std::mem::take(&mut h.world.resource_mut::<DamageQueue>().0);
    hits.iter().filter(|x| x.target == e).map(|x| x.amount).collect()
}

#[test]
fn a_splash_harms_the_creature_it_hits_for_the_origin_amount_at_each_level() {
    if horizon_ecs_proto::cow::skip_unless_default_species("a_splash_harms_the_creature_it_hits_for_the_origin_amount_at_each_level") {
        return;
    }
    if !have_data() {
        return;
    }
    let (_, mut h) = flat_pair();
    let hit = ecs_cow(&mut h, 0.5, 1.0, 0.5, 5);
    h.tick();
    let at = Vec3::new(0.5, 1.0, 0.5);
    let brews: [&'static [Brew]; 3] = [&[Brew::Harm(0)], &[Brew::Harm(1)], &[Brew::Harm(2)]];
    for (level, brew) in brews.into_iter().enumerate() {
        effects::splash(&mut h.world, at, Some(hit), brew, None);
        assert_eq!(queued(&mut h, hit), vec![origin_splash(level as i32)], "level {level}");
    }
}

#[test]
fn a_lingering_harming_cloud_harms_at_half_strength_as_the_origin_does() {
    if horizon_ecs_proto::cow::skip_unless_default_species("a_lingering_harming_cloud_harms_at_half_strength_as_the_origin_does") {
        return;
    }
    if !have_data() {
        return;
    }
    let (_, mut h) = flat_pair();
    let hit = ecs_cow(&mut h, 0.5, 1.0, 0.5, 5);
    h.tick();
    let spec = CloudSpec { wait: 0, duration: -1, ..CloudSpec::new(2.0, "minecraft:harming", None) };
    cloud_tick::spawn(&mut h.world, Vec3::new(0.5, 1.0, 0.5), spec);
    for _ in 0..5 {
        h.tick();
    }
    assert_eq!(queued(&mut h, hit), vec![origin_cloud(0)], "the cloud's first dose at tick 5");
}
