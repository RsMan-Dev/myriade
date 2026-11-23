//! The effective damage-resistant component at its actual item-damage callers.
use super::*;
use crate::game::native_entity_refs::tests::with_game;
use crate::game::native_guest_turn_tests::{with_acting_guest, with_acting_guest_on, with_guests_at};
use horizon_client::chunk::Chunk;
use horizon_core::{geometry::Vec3, position::BlockPos};

fn stack(input: &str) -> ItemStack {
    let (item, patch) = input
        .split_once('[')
        .map_or((input, None), |(item, patch)| {
            (item, Some(patch.strip_suffix(']').unwrap()))
        });
    let item = horizon_core::identifier::Identifier::parse(item).unwrap();
    let mut stack = ItemStack::new(&item.to_string(), 1);
    if let Some(patch) = patch {
        if patch == "!damage_resistant" {
            stack.data_mut().set_other(
                "!minecraft:damage_resistant",
                Some(horizon_core::nbt::Tag::Compound(Default::default())),
            );
        } else {
            let value = patch.strip_prefix("damage_resistant=").unwrap();
            let (value, _) = horizon_core::nbt::parse_snbt(value).unwrap();
            stack
                .data_mut()
                .set_other("minecraft:damage_resistant", Some(value));
        }
    }
    stack
}

fn prepare_terrain(game: &mut Game<'_>) {
    game.world = World::new(game.res.facts().air, 1);
    game.world
        .chunks
        .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
    game.update_chunk_tickets();
    assert!(game.entity_ticking_area()(4, 4));
}

fn drop_stack(game: &mut Game<'_>, input: &str, position: [f64; 3]) -> u32 {
    game.spawn_item(stack(input), position, [0.0; 3], 100);
    let id = *game.ecs.item_ids().last().unwrap();
    game.ecs.set_no_gravity(id, true);
    game.ticked_entities_through = game.next_entity;
    id
}

fn lava(game: &mut Game<'_>) {
    let lava = game
        .pack
        .assets
        .registry
        .default_state("minecraft:lava")
        .unwrap();
    for y in 64..=68 {
        game.world.set(4, y, 4, lava).unwrap();
    }
}

#[test]
#[ignore = "daily"]
fn effective_resistance_matches_the_six_stack_five_damage_table() {
    with_game(|game| {
        let damage = [
            "minecraft:in_fire",
            "minecraft:lava",
            "minecraft:cactus",
            "minecraft:explosion",
            "minecraft:lightning_bolt",
        ];
        let cases = [
            ("netherite_sword", [false, false, true, true, true]),
            (
                "netherite_sword[!damage_resistant]",
                [true, true, true, true, true],
            ),
            (
                r##"stone[damage_resistant={types:"#minecraft:panic_environmental_causes"}]"##,
                [false, false, false, true, false],
            ),
            ("nether_star", [true, true, true, false, true]),
            (
                "nether_star[!damage_resistant]",
                [true, true, true, true, true],
            ),
            (
                r##"diamond[damage_resistant={types:"#minecraft:is_explosion"}]"##,
                [true, true, true, false, true],
            ),
        ];
        for (input, expected) in cases {
            let stack = stack(input);
            for (damage, expected) in damage.into_iter().zip(expected) {
                assert_eq!(
                    game.stack_can_be_hurt_by(&stack, damage),
                    Some(expected),
                    "{input} against {damage}"
                );
            }
        }
        // Identifier::parse also gives an empty namespace the minecraft default.
        for types in [":lava", "#:is_fire"] {
            let input = format!("stone[damage_resistant={{types:\"{types}\"}}]");
            assert_eq!(
                game.stack_can_be_hurt_by(&stack(&input), "minecraft:lava"),
                Some(false)
            );
        }
    });
}

#[test]
#[ignore = "daily"]
fn removed_netherite_resistance_dies_on_the_second_lava_hurt() {
    with_acting_guest(Vec3::new(8.5, 70.0, 8.5), |game| {
        prepare_terrain(game);
        lava(game);
        let id = drop_stack(game, "netherite_sword[!damage_resistant]", [4.5, 64.0, 4.5]);
        assert_eq!(game.ecs.item_views()[0].health, 5.0);
        game.tick_items();
        assert_eq!(game.ecs.item_ids()[0], id);
        assert_eq!(game.ecs.item_views()[0].health, 1.0);
        assert_eq!(game.ecs.item_views()[0].age, 1);
        game.tick_items();
        assert!(game.ecs.item_views().iter().all(|item| item.id != id));
    });
}

#[test]
#[ignore = "daily"]
fn environmental_tag_stone_keeps_five_health_for_forty_lava_ticks() {
    with_acting_guest(Vec3::new(8.5, 70.0, 8.5), |game| {
        prepare_terrain(game);
        lava(game);
        let id = drop_stack(
            game,
            r##"stone[damage_resistant={types:"#minecraft:panic_environmental_causes"}]"##,
            [4.5, 64.0, 4.5],
        );
        for _ in 0..40 {
            game.tick_items();
            assert_eq!(game.ecs.item_ids()[0], id);
            assert_eq!(game.ecs.item_views()[0].health, 5.0);
        }
        assert_eq!(game.ecs.item_views()[0].age, 40);
    });
}

#[test]
#[ignore = "daily"]
fn a_mod_items_default_cactus_resistance_keeps_five_health_for_ten_ticks() {
    // Give the owning resource load a private mod-item report and registry, as
    // native_components' changed-default fixture does. Shared resources are read-only.
    let root = tempfile::tempdir().unwrap();
    let source = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() != "items.json" && entry.file_name() != "command-registries.json" {
            #[cfg(unix)]
            std::os::unix::fs::symlink(entry.path(), root.path().join(entry.file_name())).unwrap();
            #[cfg(not(unix))]
            panic!("this resource-overlay fixture requires Unix symlinks");
        }
    }
    let mut defaults: serde_json::Value =
        serde_json::from_slice(&std::fs::read(source.join("items.json")).unwrap()).unwrap();
    defaults["fixture:cactus_item"] = defaults["minecraft:stone"].clone();
    defaults["fixture:cactus_item"]["minecraft:damage_resistant"] =
        serde_json::json!({"types": "minecraft:cactus"});
    std::fs::write(
        root.path().join("items.json"),
        serde_json::to_vec(&defaults).unwrap(),
    )
    .unwrap();
    let mut registries: serde_json::Value =
        serde_json::from_slice(&std::fs::read(source.join("command-registries.json")).unwrap())
            .unwrap();
    registries["items"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!("fixture:cactus_item"));
    std::fs::write(
        root.path().join("command-registries.json"),
        serde_json::to_vec(&registries).unwrap(),
    )
    .unwrap();
    with_guests_at(root.path().to_path_buf(), |game| {
        with_acting_guest_on(game, Vec3::new(8.5, 70.0, 8.5), |game| {
        prepare_terrain(game);
        let cactus = game
            .pack
            .assets
            .registry
            .default_state("minecraft:cactus")
            .unwrap();
        game.world.set(4, 64, 4, cactus).unwrap();
        drop_stack(game, "minecraft:stone", [5.08, 64.0, 4.5]);
        game.tick_items();
        assert_eq!(game.ecs.item_views()[0].health, 4.0, "the tick really touches cactus");
        game.ecs_clear_items();
        let id = drop_stack(game, "fixture:cactus_item", [5.08, 64.0, 4.5]);
        assert!(
            game.ecs.item_views()[0].stack.data().is_none(),
            "resistance comes from defaults"
        );
        for _ in 0..10 {
            // Keep the item touching the cactus block while outside its inset collision box.
            game.ecs.set_item_position(game.ecs.item_ids()[0], Vec3::new(5.08, 64.0, 4.5));
            { let e = game.ecs.entity_of(game.ecs.item_ids()[0]).unwrap(); game.ecs.set_velocity(e, Vec3::ZERO); }
            game.tick_items();
            assert_eq!(game.ecs.item_ids()[0], id);
            assert_eq!(game.ecs.item_views()[0].health, 5.0);
        }
        assert_eq!(game.ecs.item_views()[0].age, 10);
        });
    });
}

#[test]
#[ignore = "daily"]
fn damage_command_uses_effective_item_resistance() {
    with_acting_guest(Vec3::new(8.5, 70.0, 8.5), |game| {
        prepare_terrain(game);
        game.build_registries();
        for (input, damage, expected_health) in [
            ("netherite_sword", "lava", 5),
            ("netherite_sword[!damage_resistant]", "lava", 1),
            (
                r##"stone[damage_resistant={types:"#minecraft:panic_environmental_causes"}]"##,
                "lava",
                5,
            ),
            ("nether_star", "explosion", 5),
            ("nether_star[!damage_resistant]", "explosion", 1),
            (
                r##"diamond[damage_resistant={types:"#minecraft:is_explosion"}]"##,
                "explosion",
                5,
            ),
        ] {
            game.ecs_clear_items();
            drop_stack(game, input, [4.5, 64.0, 4.5]);
            let command = format!("damage @e[type=minecraft:item,limit=1] 4 minecraft:{damage}");
            let success = game.as_console(|game| game.run_command(&command));
            assert_eq!(success, expected_health == 1, "{input}: {command}");
            assert_eq!(
                game.ecs.item_views()[0].health, expected_health as f32,
                "{input}: {command}"
            );
        }
    });
}
