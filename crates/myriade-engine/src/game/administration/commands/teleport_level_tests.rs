//! A mod's `teleport_to_level` into another level, kind by kind
//! (`Entity.teleportCrossDimension`): what each kind keeps, and a kind it cannot move.
use crate::game::native_entity_refs::tests::with_game;
use crate::game::{Dim, Game};
use horizon_core::geometry::Vec3;
use horizon_core::nbt::Tag;


/// Summons `kind` with `nbt` at 0.5 80 0.5 in the Overworld, where `id` finds it, and sends
/// it to 3.5 70 -2.5 in the Nether by a mod's `teleport_to_level`: the call is true and the
/// Overworld holds it no more. The Nether is left in place.
fn sent_to_the_nether(
    game: &mut Game<'_>,
    kind: &str,
    nbt: &str,
    id: impl Fn(&Game<'_>) -> Option<u32>,
) {
    game.native_lifecycle.world_open_ready().unwrap();
    game.summon_with_nbt(kind, [0.5, 80.0, 0.5], nbt).unwrap();
    game.lan_assign_ids();
    let entity = game.entity(id(game).expect("summoned")).unwrap();
    assert!(game.entity_teleport_to_level(entity, "minecraft:the_nether", [3.5, 70.0, -2.5], None));
    assert!(id(game).is_none(), "the Overworld still holds it");
    game.enter_level(Dim::Nether).unwrap();
}

/// A primed TNT is recreated in the other level with its fuse and motion
/// (`PrimedTnt.restoreFrom`, `Relative.DELTA`).
#[test]
#[ignore = "daily"]
fn a_mod_sends_a_primed_tnt_to_another_level_with_its_fuse_and_motion() {
    with_game(|game| {
        sent_to_the_nether(
            game,
            "minecraft:tnt",
            "{fuse:200s,Motion:[0.1d,0.2d,-0.3d]}",
            |g| g.ecs.species_ids("minecraft:tnt").first().copied(),
        );
        let ids = game.ecs.species_ids("minecraft:tnt");
        assert_eq!(ids.len(), 1);
        let e = game.ecs.entity_of(ids[0]).unwrap();
        let saved = game.ecs.entity_nbt(e).unwrap();
        assert_eq!(saved.get(&"fuse".into()), Some(&Tag::Short(200)));
        let m = game.ecs.motion(ids[0]).unwrap();
        assert_eq!(m.position, [3.5, 70.0, -2.5]);
        assert_eq!(m.velocity, [0.1, 0.2, -0.3]);
    });
}

/// An experience orb is recreated in the other level with its value, count and motion.
#[test]
#[ignore = "daily"]
fn a_mod_sends_an_experience_orb_to_another_level_with_its_value_and_motion() {
    with_game(|game| {
        sent_to_the_nether(
            game,
            "minecraft:experience_orb",
            "{Value:7s,Count:2,Motion:[0.1d,0.2d,-0.3d]}",
            |g| g.ecs.orb_views().first().map(|o| o.id),
        );
        let views = game.ecs.orb_views();
        assert_eq!(views.len(), 1);
        let orb = views[0];
        assert_eq!((orb.value, orb.count), (7, 2));
        assert_eq!(orb.position, Vec3::new(3.5, 70.0, -2.5));
        assert_eq!(orb.velocity, Vec3::new(0.1, 0.2, -0.3));
    });
}

/// A firework rocket is recreated in the other level with its stack, owner and motion
/// (`Projectile.restoreFrom`).
#[test]
#[ignore = "daily"]
fn a_mod_sends_a_firework_to_another_level_with_its_owner_and_motion() {
    with_game(|game| {
        sent_to_the_nether(
            game,
            "minecraft:firework_rocket",
            r#"{LifeTime:40,Owner:[I;5,6,7,8],Motion:[0.1d,0.2d,-0.3d],FireworksItem:{id:"minecraft:firework_rocket",count:1}}"#,
            |g| g.ecs.species_ids("minecraft:firework_rocket").first().copied(),
        );
        let ids = game.ecs.species_ids("minecraft:firework_rocket");
        assert_eq!(ids.len(), 1);
        let m = game.ecs.motion(ids[0]).unwrap();
        assert_eq!(m.position, [3.5, 70.0, -2.5]);
        assert_eq!(m.velocity, [0.1, 0.2, -0.3]);
    });
}

/// Armor stands use Entity.teleportCrossDimension, keeping their position on arrival.
#[test]
#[ignore = "daily"]
fn a_mod_sends_an_armor_stand_to_another_level() {
    with_game(|game| {
        game.native_lifecycle.world_open_ready().unwrap();
        game.summon_with_nbt("minecraft:armor_stand", [0.5, 80.0, 0.5], "{}")
            .unwrap();
        game.lan_assign_ids();
        let stand = game.entity(game.armor_stand_list()[0].id).unwrap();
        assert!(game.entity_teleport_to_level(
            stand,
            "minecraft:the_nether",
            [3.5, 70.0, -2.5],
            None
        ));
        assert!(game.armor_stand_list().is_empty());
        game.enter_level(Dim::Nether).unwrap();
        assert_eq!(game.armor_stand_list().len(), 1);
        assert_eq!(game.armor_stand_list()[0].position, Vec3::new(3.5, 70.0, -2.5));
    });
}

#[test]
#[ignore = "daily"]
fn portal_arrival_effects_mod_items_merge_immediately() {
    with_game(|game| {
        game.native_lifecycle.world_open_ready().unwrap();
        game.spawn_item_own(
            myriade_engine::items::ItemStack::new("minecraft:diamond", 3),
            [0.5, 80.0, 0.5],
            10,
        );
        game.ecs.set_item_age(game.ecs.item_ids()[0], 7);
        { let e = game.ecs.entity_of(game.ecs.item_ids()[0]).unwrap(); game.ecs.set_velocity(e, Vec3::new(0.1, 0.2, -0.3)); }
        let item = game.entity(game.ecs.item_ids()[0]).unwrap();
        game.enter_level(Dim::Nether).unwrap();
        game.spawn_item_own(
            myriade_engine::items::ItemStack::new("minecraft:diamond", 5),
            [3.5, 70.0, -2.5],
            10,
        );
        game.ecs.set_item_age(game.ecs.item_ids()[0], 41);
        let waiting = game.ecs.item_ids()[0];
        game.enter_level(Dim::Overworld).unwrap();
        assert!(game.entity_teleport_to_level(
            item,
            "minecraft:the_nether",
            [3.5, 70.0, -2.5],
            None
        ));
        assert!(game.ecs.item_ids().is_empty());
        game.enter_level(Dim::Nether).unwrap();
        assert_eq!(game.ecs.item_ids().len(), 1);
        let survivor = game.ecs.item_views()[0].clone();
        assert_eq!(survivor.id, waiting);
        assert_eq!(
            (survivor.stack.count, survivor.age, survivor.ticks),
            (8, 7, 0)
        );
    });
}
