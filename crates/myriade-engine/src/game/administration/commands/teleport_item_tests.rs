use crate::game::native_entity_refs::tests::with_game;
use horizon_core::geometry::Vec3;

#[test]
#[ignore = "daily"]
fn relative_item_teleports_keep_only_relative_horizontal_motion() {
    with_game(|game| {
        game.session.registries.permission = 4;
        game.spawn_item_own(
            myriade_engine::items::ItemStack::new("minecraft:stone", 1),
            [0.0, 64.0, 0.0],
            10,
        );
        { let e = game.ecs.entity_of(game.ecs.item_ids()[0]).unwrap(); game.ecs.set_velocity(e, Vec3::new(1.0, -2.0, 3.0)); }
        assert!(game.run_command("tp @e[type=minecraft:item] ~ 70 12"));
        assert_eq!(game.ecs.item_views()[0].velocity, Vec3::new(1.0, 0.0, 0.0));
        { let e = game.ecs.entity_of(game.ecs.item_ids()[0]).unwrap(); game.ecs.set_velocity(e, Vec3::new(1.0, -2.0, 3.0)); }
        assert!(game.run_command("tp @e[type=minecraft:item] 12 ~ ~"));
        assert_eq!(game.ecs.item_views()[0].velocity, Vec3::new(0.0, 0.0, 3.0));
        assert!(game.run_command("tp @e[type=minecraft:item] 10 70 10"));
        assert_eq!(game.ecs.item_views()[0].velocity, Vec3::ZERO);
    });
}

/// `/tp` into another level takes an item too (`TeleportCommand` ->
/// `Entity.teleportTo(ServerLevel, ..)` -> `teleportCrossDimension`), and its
/// transition has no delta and no `Relative.DELTA`: the item arrives still.
#[test]
#[ignore = "daily"]
fn an_item_teleported_into_another_level_arrives_there_still() {
    with_game(|game| {
        game.session.registries.permission = 4;
        game.spawn_item_own(
            myriade_engine::items::ItemStack::new("minecraft:stick", 3),
            [0.0, 64.0, 0.0],
            10,
        );
        { let e = game.ecs.entity_of(game.ecs.item_ids()[0]).unwrap(); game.ecs.set_velocity(e, Vec3::new(1.0, -2.0, 3.0)); }
        assert!(game.run_command(
            "execute in minecraft:the_nether run tp @e[type=minecraft:item] 1.5 70 1.5"
        ));
        assert!(game.ecs.item_ids().is_empty());
        game.enter_level(crate::game::Dim::Nether).unwrap();
        assert_eq!(game.ecs.item_ids().len(), 1);
        let item = game.ecs.item_views()[0].clone();
        assert_eq!(
            (&*item.stack.item, item.stack.count),
            ("minecraft:stick", 3)
        );
        assert_eq!(item.position, Vec3::new(1.5, 70.0, 1.5));
        assert_eq!(item.velocity, Vec3::ZERO);
    });
}

#[test]
#[ignore = "daily"]
fn portal_arrival_effects_command_items_merge_immediately() {
    with_game(|game| {
        game.build_registries();
        game.set_allow_commands(true);
        game.spawn_item_own(
            myriade_engine::items::ItemStack::new("minecraft:stick", 5),
            [0.5, 70.0, 0.5],
            10,
        );
        game.ecs.set_item_age(game.ecs.item_ids()[0], 7);
        game.ecs.set_item_pickup_delay(game.ecs.item_ids()[0], 30);
        let incoming_uuid = game.uuid_for(game.ecs.item_ids()[0]);
        game.enter_level(crate::game::Dim::Nether).unwrap();
        game.spawn_item_own(
            myriade_engine::items::ItemStack::new("minecraft:stick", 3),
            [1.5, 70.0, 1.5],
            10,
        );
        game.ecs.set_item_age(game.ecs.item_ids()[0], 41);
        game.enter_level(crate::game::Dim::Overworld).unwrap();
        assert!(game.run_command(
            "execute in minecraft:the_nether run tp @e[type=minecraft:item] 1.5 70 1.5"
        ));
        assert!(game.ecs.item_ids().is_empty());
        game.enter_level(crate::game::Dim::Nether).unwrap();
        assert_eq!(game.ecs.item_ids().len(), 1);
        let item = game.ecs.item_views()[0].clone();
        assert_eq!((item.stack.count, item.age, item.pickup_delay), (8, 7, 30));
        assert_eq!(item.velocity, Vec3::ZERO);
        assert_eq!(item.ticks, 0);
        assert_eq!(game.ecs.uuid_words(item.id), Some(incoming_uuid));
    });
}
