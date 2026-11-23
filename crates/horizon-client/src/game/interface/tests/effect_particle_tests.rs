//! `MobEffect.particleFactory`: Wind Charged, Weaving, Oozing and Infested
//! are registered with a particle of their own (`MobEffects`), and every
//! other effect an entity effect in its colour, faint when ambient. This is
//! the list `LivingEntity.tickEffects` swirls around the window's player and
//! around the mobs of this game.
use super::effect_particle;
use super::*;
use crate::game::*;
use horizon_client::effects::EffectInstance;
use horizon_client::motion::PlayerMotion;
use horizon_client::particles::{ParticleOptions, ParticleType};
use std::path::PathBuf;
use std::time::Instant;

/// The particle type an effect shows, or a panic naming the effect.
fn ty(effect: &str, ambient: bool) -> ParticleType {
    match effect_particle(effect, ambient) {
        Some(p) => p.ty,
        None => panic!("{effect} shows no particle"),
    }
}

#[test]
fn the_four_party_effects_show_their_own_particle_types() {
    // `WindChargedMobEffect` passes `ParticleTypes.SMALL_GUST`, `Weaving`
    // `ITEM_COBWEB`, `Oozing` `ITEM_SLIME` and `Infested` `INFESTED`.
    let own = [
        ("oozing", ParticleType::ItemSlime),
        ("weaving", ParticleType::ItemCobweb),
        ("wind_charged", ParticleType::SmallGust),
        ("infested", ParticleType::Infested),
    ];
    for (effect, want) in own {
        for ambient in [false, true] {
            assert_eq!(ty(effect, ambient), want, "{effect} ambient {ambient}");
            // The effects' ids also arrive namespaced.
            let named = format!("minecraft:{effect}");
            assert_eq!(ty(&named, ambient), want, "{named} ambient {ambient}");
        }
    }
}

#[test]
fn every_other_effect_stays_an_entity_effect_in_its_colour() {
    // `MobEffects.SPEED` is 0x33EBFF, `SLOWNESS` 0x8BAFE0; an ambient
    // instance's alpha is `AMBIENT_ALPHA` (38) of 255.
    let channel = |color: u32, shift: u32| ((color >> shift) & 255) as f32 / 255.0;
    let entity = |color: u32, alpha: f32| {
        ParticleOptions::colored(
            ParticleType::EntityEffect,
            [
                channel(color, 16),
                channel(color, 8),
                channel(color, 0),
                alpha,
            ],
        )
    };
    for (effect, color) in [("speed", 0x33_EBFF), ("slowness", 0x8B_AFE0)] {
        let shown = effect_particle(effect, false).unwrap();
        let want = entity(color, 1.0);
        assert_eq!(shown.ty, want.ty, "{effect}");
        assert_eq!(shown.color, want.color, "{effect}");
        let faint = effect_particle(effect, true).unwrap();
        assert_eq!(faint.color, entity(color, 38.0 / 255.0).color, "{effect}");
    }
}

/// A game with the prepared vanilla resources (`HORIZON_TEST_RESOURCES`): the
/// tests that use it are `#[ignore]`d, as CI has no resources.
fn with_game(test: impl FnOnce(&mut Game<'_>)) {
    let root = PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
    let settings = Settings::default();
    let loaded = load_resources(
        horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
        Vec::new(),
        None,
        &root,
        &settings,
        Instant::now(),
        crate::game::execution_role::ExecutionRole::DedicatedServer,
        &Progress::new(),
    )
    .unwrap();
    let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
    options.resources = root.clone();
    options.execution_role = execution_role::ExecutionRole::DedicatedServer;
    let mut g = Game::new(
        &loaded.resources,
        loaded.pack,
        options,
        settings,
    )
    .unwrap();
    g.world = World::new(g.res.facts().air, 1);
    test(&mut g);
}
