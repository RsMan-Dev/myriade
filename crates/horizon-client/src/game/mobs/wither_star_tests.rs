//! The wither's nether star: `WitherBoss.dropCustomDeathLoot` spawns it and
//! then calls `ItemEntity.setExtendedLifetime` (`age = -6000`), so the age
//! discard (`age >= 6000`) takes it at tick 12000 instead of 6000.
use crate::entities::ItemEntity;
use crate::items::ItemStack;
use horizon_core::geometry::Vec3;
use horizon_core::nbt::Tag;
use std::sync::Arc;

/// `ItemEntity.addAdditionalSaveData` saves `Age` as a short, so the star's
/// negative age is still there after a save and load.
#[test]
fn a_stars_negative_age_survives_item_nbt() {
    let mut star = ItemEntity::new(
        7,
        ItemStack::new("minecraft:nether_star", 1),
        Vec3::new(4.5, 200.0, 4.5),
        Vec3::ZERO,
        0.0,
    );
    star.age = -6000;
    let tag = crate::entity_nbt::item_entity_to_nbt(&star, [1, 2, 3, 4], None);
    assert_eq!(tag.get("Age"), Some(&Tag::Short(-6000)));
    let loaded = crate::entity_nbt::item_entity_from_nbt(&tag, 7, 0.0).unwrap();
    assert_eq!(loaded.age, -6000);
    assert_eq!(&*loaded.stack.item, "minecraft:nether_star");
}

/// Stack-only loot edits must not inherit a custom item entity's initial age.
#[test]
#[ignore = "daily"]
fn loot_stack_edits_leave_custom_drop_age_attached() {
    use crate::game::api_values::TryIntoApi;
    use std::sync::Mutex;

    #[derive(Clone, Copy)]
    enum Edit {
        Replace,
        Remove,
        Reorder,
    }
    struct Probe {
        edit: Edit,
        captured: Arc<Mutex<Vec<(mod_api::ItemStack, i32)>>>,
    }
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for Probe {
        fn loot_drops(
            &self,
            _: &mut dyn mod_api::ServerHost,
            event: &mut mod_api::LootDropsEvent<'_>,
        ) {
            let star = ItemStack::new("minecraft:nether_star", 1)
                .try_into_api()
                .unwrap();
            assert!(
                !event.drops.iter().any(|stack| stack.item() == star.item()),
                "custom death loot is not a table modifier input"
            );
            let stone = ItemStack::new("minecraft:stone", 1).try_into_api().unwrap();
            let dirt = ItemStack::new("minecraft:dirt", 1).try_into_api().unwrap();
            match self.edit {
                Edit::Replace => *event.drops = vec![stone],
                Edit::Remove => event.drops.clear(),
                Edit::Reorder => {
                    event.drops.extend([stone, dirt]);
                    event.drops.reverse();
                }
            }
        }
        fn living_drops(
            &self,
            _: &mut dyn mod_api::ServerHost,
            event: &mut mod_api::LivingDropsEvent<'_>,
        ) -> mod_api::Flow {
            *self.captured.lock().unwrap() = event
                .drops
                .iter()
                .map(|drop| (drop.stack.clone(), drop.age))
                .collect();
            mod_api::Flow::Continue
        }
    }

    for (edit, table_items) in [
        (Edit::Replace, vec!["minecraft:stone"]),
        (Edit::Remove, vec![]),
        (Edit::Reorder, vec!["minecraft:dirt", "minecraft:stone"]),
    ] {
        crate::game::native_entity_refs::tests::with_game(|game| {
            game.options.dedicated = Some(Vec::new());
            game.build_registries();
            // The fixture stops before world entry, which loads the death loot snapshot.
            game.load_server_loot();
            let captured = Arc::new(Mutex::new(Vec::new()));
            let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
                "custom-drop-age",
                Box::new(Probe {
                    edit,
                    captured: captured.clone(),
                }),
            )]
            .into();
            let wither = game
                .summon_with_nbt("minecraft:wither", [4.5, 200.0, 4.5], "{}")
                .unwrap()
                .expect("the wither is in the world");
            mod_platform::testing::with_mods(mods, || game.kill_target(wither));
            let mut expected: Vec<_> = table_items.into_iter().map(|item| (item, 0)).collect();
            expected.push(("minecraft:nether_star", -6000));
            let api_expected: Vec<_> = expected
                .iter()
                .map(|(item, age)| (ItemStack::new(item, 1).try_into_api().unwrap(), *age))
                .collect();
            assert_eq!(
                *captured.lock().unwrap(),
                api_expected,
                "LivingDrops sees complete records after table edits"
            );
            let spawned: Vec<_> = game
                .dropped
                .iter()
                .map(|drop| (&*drop.stack.item, drop.age))
                .collect();
            assert_eq!(
                spawned, expected,
                "spawned records preserve their captured ages"
            );
        });
    }
}
