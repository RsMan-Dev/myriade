#![cfg(test)]

use super::native_entity_refs::tests::with_game;
use super::{Dim, commands::Target};

#[test]
#[ignore = "daily"]
fn cushion_get_entity_matches_vanilla_membership_oracle() {
    with_game(|game| {
        game.summon_with_nbt("minecraft:cushion", [0.5, 70.0, 0.5], "{}")
            .unwrap();
        game.lan_assign_ids();
        let id = game.cushion_ids()[0];
        assert!(
            matches!(game.level_entity(id as i32), Some(Target::Cushion(found)) if found == id)
        );
        assert!(game.level_entity(0).is_none());
        assert!(game.level_entity(i32::MAX).is_none());
        game.enter_level(Dim::Nether).unwrap();
        assert!(game.level_entity(id as i32).is_none());
        game.enter_level(Dim::Overworld).unwrap();
        assert!(
            matches!(game.level_entity(id as i32), Some(Target::Cushion(found)) if found == id)
        );
    });
}
