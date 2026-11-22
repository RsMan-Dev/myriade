use crate::game::native_entity_refs::tests::with_game;
use horizon_client::join_seq::Joined;

// Use the real NBT spawn loader, including its id allocation and join hooks,
// while every list is held by the production take/restore boundary.
macro_rules! restore_cases {
    ($($name:ident: $kind:literal, $nbt:literal, $($field:ident).+;)*) => {$ (
        #[test]
        #[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
        fn $name() {
            with_game(|game| {
                game.summon_with_nbt($kind, [0.5, 70.0, 0.5], $nbt).unwrap();
                game.lan_assign_ids();
                let prefix: Vec<_> = game$(.$field)+.iter().map(|e| (e.id, e.join_seq())).collect();
                assert_eq!(prefix.len(), 1);
                let taken = game.take_entities();
                game.summon_with_nbt($kind, [4.5, 70.0, 0.5], $nbt).unwrap();
                game.lan_assign_ids();
                let suffix: Vec<_> = game$(.$field)+.iter().map(|e| (e.id, e.join_seq())).collect();
                assert_eq!(suffix.len(), 1);
                assert!(prefix[0].1 < suffix[0].1);
                assert_ne!(prefix[0].0, suffix[0].0);
                game.restore_entities(taken);
                let restored: Vec<_> = game$(.$field)+.iter().map(|e| (e.id, e.join_seq())).collect();
                assert_eq!(restored, [prefix, suffix].concat());
            });
        }
    )*};
}

restore_cases! {
    restores_stand_spawns: "minecraft:armor_stand", "{}", armor_stands;
    restores_projectile_spawns: "minecraft:arrow", "{}", creatures.projectiles;
}
