use super::*;
use mod_api::ServerHost;

use horizon_client::mobs::{
    BreedExperience,
    finalize::{Attr, set_base},
    goals::{Breed, Cx, Goal},
};
use horizon_core::random::{LegacyRandom, RandomSource};

#[test]
#[ignore = "daily"]
fn first_chicken_breed_matches_without_mods_and_with_an_empty_mod() {
    use horizon_client::mobs::{BreedExperience, MobEvent, goals::Breed};
    use horizon_core::random::LegacyRandom;
    use mod_api::{self, Hooks};
    struct Empty;
    #[mod_api::__subscriptions]
    impl Hooks for Empty {}
    super::super::native_entity_refs::tests::with_game(|game| {
        let kind = game
            .res
            .mob_types()
            .get("minecraft:chicken")
            .unwrap()
            .clone();
        let mut mate = Mob::new(2, kind.clone(), Vec3::new(0.5, 64.0, 0.0), 0.0);
        mate.extra.in_love = 600;
        let others = [mate.seen(None, game.items())];
        let run = || {
            let world = MobWorld {
                others: &others,
                ..game.mob_world_base(&[])
            };
            assert!(!world.asks_baby_spawn);
            let mut parent = Mob::new(1, kind.clone(), Vec3::new(0.0, 64.0, 0.0), 0.0);
            parent.random = horizon_core::random::EntityRandom::own(LegacyRandom::new(123));
            parent.persistent = true;
            parent.extra.in_love = 600;
            parent.extra.egg_time = 10_000;
            parent.ai.targets.clear();
            parent.ai.goals.clear();
            parent.ai.goals.add(2, Breed::new(1.0));
            let mut events = Vec::new();
            let mut test_level = horizon_client::health::TestLevel::new(321);
            let level = &mut test_level.level();
            for _ in 0..200 {
                // The fixture has no floor; keep the pair within breeding range.
                parent.position = Vec3::new(0.0, 64.0, 0.0);
                parent.tick(&world, level, &mut events);
                if let Some(xp) = events.iter().find_map(|event| match event {
                    MobEvent::Breed {
                        experience: BreedExperience::Drawn(Some(xp)),
                        ..
                    } => Some(*xp),
                    _ => None,
                }) {
                    return (parent.random.state(), xp);
                }
                events.clear();
            }
            panic!("chickens did not breed");
        };
        let no_mods = mod_platform::testing::with_mods(Vec::new().into(), run);
        let empty = vec![mod_api::ModEntry::new("empty", Box::new(Empty))].into();
        let unrelated = mod_platform::testing::with_mods(empty, run);
        assert_eq!(no_mods, unrelated);
    });
}

#[test]
#[ignore = "daily"]
fn empty_mod_never_defers_breeding_xp() {
    use mod_api::{self, Hooks};
    struct Empty;
    #[mod_api::__subscriptions]
    impl Hooks for Empty {}
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new("empty", Box::new(Empty))].into();
    mod_platform::testing::with_mods(mods, || {
        super::super::native_entity_refs::tests::with_game(|game| {
            assert!(!game.mob_world_base(&[]).asks_baby_spawn);
            let mut cancel = false;
            game.native_baby_entity_spawn(1, 2, None, None, "minecraft:chicken", &mut cancel);
            assert!(!cancel);
            assert!(!game.mob_world_base(&[]).asks_baby_spawn);
        });
    });
}

#[test]
fn the_player_who_fed_either_animal_is_credited() {
    let mut love = std::collections::HashMap::from([(1, Some(7)), (2, Some(9))]);
    // The parent's feeder first, and both animals are spent.
    assert_eq!(breeding_credit(&mut love, 1, 2), Some(Some(7)));
    assert!(love.is_empty());
    // Only the partner was fed: still credited.
    let mut love = std::collections::HashMap::from([(2, None)]);
    assert_eq!(breeding_credit(&mut love, 1, 2), Some(None));
    // Neither was fed by a player (a bred pair loaded from a save).
    assert_eq!(breeding_credit(&mut love, 1, 2), None);
}
