//! Thorns on the dragon go through its body part: a quarter of the roll plus at most one.
use super::*;
use crate::game::*;

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
        execution_role::ExecutionRole::DedicatedServer,
        &Progress::new(),
    )
    .unwrap();
    let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
    options.resources = root.clone();
    options.execution_role = execution_role::ExecutionRole::DedicatedServer;
    let mut g = Game::new(&loaded.resources, loaded.pack, options, settings).unwrap();
    g.world = World::new(g.res.facts().air, 1);
    g.player = Some(PlayerMotion::new(g.motion, Vec3::new(30.0, 64.0, 0.5)).unwrap());
    g.player.as_mut().unwrap().entity_id = 77;
    test(&mut g);
}

#[test]
#[ignore = "daily"]
fn dragon_takes_player_thorns_through_its_body() {
    with_game(|g| {
        let id = g
            .spawn_dragon_spawned([0.0, 70.0, 0.0], crate::game::creatures::SpawnSite::Plain)
            .unwrap();
        g.dragon_thorns(id, 1.0, 5.0);
        let (_, view) = g.dragon_views().into_iter().find(|(d, _)| *d == id).unwrap();
        // DamageEntity rolls [1,5); the body's cut is damage/4 + min(damage,1), so at most 2.
        assert!(view.health < 200.0 && view.health >= 200.0 - 2.0);
    });
}
