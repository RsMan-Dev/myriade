use super::*;
use crate::game::native_entity_refs::tests::with_game;
use horizon_core::nbt::Tag;
use mod_api::{AttachmentQueries, AttachmentQueriesMut, AttachmentType};

static VALUE: AttachmentType<i32> = AttachmentType {
    namespace: "test",
    name: "mana",
    default: || 0,
    codec: None,
    copy_on_death: false,
    sync: false,
};

#[test]
#[ignore = "daily"]
fn tnt_with_real_lifetime_accepts_first_store() {
    with_game(|game| {
        game.options.dedicated = Some(Vec::new());
        game.native_level_opened(Dim::Overworld);
        game.summon_with_nbt("minecraft:tnt", [0.5, 70.0, 0.5], "{}")
            .unwrap();
        game.lan_assign_ids();
        let id = game.ecs.species_ids("minecraft:tnt")[0];
        assert_eq!(VALUE.set(game, Holder::Entity(id), 7), Ok(()));
    });
}
