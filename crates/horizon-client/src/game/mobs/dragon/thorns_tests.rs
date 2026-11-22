//! Exercise the server's attack entry points with real enchantment data.
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
        execution_role::ExecutionRole::IntegratedServer,
        &Progress::new(),
    )
    .unwrap();
    let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
    options.resources = root.clone();
    options.execution_role = execution_role::ExecutionRole::IntegratedServer;
    let mut g = Game::new(
        &loaded.resources,
        loaded.pack,
        options,
        settings,
        Audio::default(),
    )
    .unwrap();
    g.options.dedicated = Some(Vec::new());
    g.world = World::new(g.res.facts().air, 1);
    g.player = Some(PlayerMotion::new(g.motion, Vec3::new(30.0, 64.0, 0.5)).unwrap());
    g.player.as_mut().unwrap().entity_id = 77;
    g.audio.capture = Some(Vec::new());
    test(&mut g);
}
fn thorny() -> ItemStack {
    let mut stack = ItemStack::new("minecraft:diamond_chestplate", 1);
    stack.set_crafting_enchantments(vec![("minecraft:thorns".into(), 255)]);
    stack
}
#[test]
#[ignore = "daily"]
fn dragon_takes_player_thorns_through_its_body() {
    with_game(|g| {
        for wing in [false, true] {
            g.player = Some(PlayerMotion::new(g.motion, Vec3::new(30.0, 64.0, 0.5)).unwrap());
            g.player.as_mut().unwrap().entity_id = 77;
            g.armor[1] = Some(thorny());
            let mut d = Dragon::new(Vec3::new(0.0, 70.0, 0.0), 0.0, 1);
            d.id = SPECIAL;
            g.dragon.replace(d);
            g.audio.capture.as_mut().unwrap().clear();
            if wing {
                g.dragon_wing_knock_back(Vec3::ZERO, false);
            } else {
                g.dragon_hurts_player("minecraft:mob_attack", 10.0);
            }
            let d = g.dragon.as_ref().unwrap();
            // DamageEntity rolls [1,5); the body's cut is damage/4 + min(damage,1).
            assert_eq!(d.health, 198.019_12);
            assert_eq!(
                d.last_source,
                Some(("minecraft:thorns", Some(77), Some(77)))
            );
            assert_eq!(d.hurt_by_player, Some((FightWho::Host, 0)));
            // Player.hurtArmor adds floor(max(1, damage / 4)) before
            // Thorns' two durability: 10-point head hit = 4, 5-point wing = 3.
            assert_eq!(
                g.armor[1].as_ref().unwrap().damage,
                if wing { 3 } else { 4 }
            );
            assert_eq!(
                g.audio
                    .capture
                    .as_ref()
                    .unwrap()
                    .iter()
                    .filter(|s| s.event == "minecraft:enchant.thorns.hit")
                    .count(),
                1
            );
        }
        // Head contacts run effects through a refused player hit; breath does not.
        g.dragon.as_mut().unwrap().invulnerable = 0;
        g.player.as_mut().unwrap().health.advance_clock();
        let health = g.player.as_ref().unwrap().health.health();
        g.dragon_hurts_player("minecraft:mob_attack", 1.0);
        assert_eq!(g.player.as_ref().unwrap().health.health(), health);
        // The refused head hit adds only Thorns wear to the wing's 3;
        // the refused breath hit adds neither ordinary armor wear nor Thorns.
        assert_eq!(g.armor[1].as_ref().unwrap().damage, 5);
        let before = g.dragon.as_ref().unwrap().health;
        g.dragon_hurts_player("minecraft:indirect_magic", 1.0);
        assert_eq!(g.dragon.as_ref().unwrap().health, before);
        assert_eq!(g.armor[1].as_ref().unwrap().damage, 5);

        // Move past knockBack's two-tick recent-attacker exclusion, but stay
        // inside the ten-tick damage cooldown so this is an actual refused hurt.
        for _ in 0..3 {
            g.player.as_mut().unwrap().health.advance_clock();
            g.world.game_time += 1;
        }
        // A refused wing still retaliates, but neither wears ordinary armor
        // nor retains its nonzero push or a new velocity-sync marker.
        let d = g.dragon.as_mut().unwrap();
        d.invulnerable = 0;
        d.last_source = None;
        let before = d.health;
        g.audio.capture.as_mut().unwrap().clear();
        let p = g.player.as_mut().unwrap();
        p.health.sync_velocity = false;
        let velocity = Vec3::new(0.25, -0.125, 0.75);
        p.velocity = velocity;
        g.dragon_wing_knock_back(Vec3::new(0.5, 0.2, -0.25), false);
        let p = g.player.as_ref().unwrap();
        assert_eq!(p.health.health(), health);
        assert_eq!(p.velocity, velocity);
        assert!(!p.health.sync_velocity);
        assert_eq!(g.armor[1].as_ref().unwrap().damage, 7);
        let d = g.dragon.as_ref().unwrap();
        assert!(d.health < before);
        assert_eq!(
            d.last_source,
            Some(("minecraft:thorns", Some(77), Some(77)))
        );
        // Attribution stores the dragon's age, not the world's game time.
        assert_eq!(d.hurt_by_player, Some((FightWho::Host, 0)));
        assert_eq!(
            g.audio
                .capture
                .as_ref()
                .unwrap()
                .iter()
                .filter(|s| s.event == "minecraft:enchant.thorns.hit")
                .count(),
            1
        );
    });
}
