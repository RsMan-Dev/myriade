//! Custom names: `NameTagItem.interactLivingEntity` and the name tag
//! (`EntityRenderer.submitNameTag`): above the head, facing the camera,
//! on a translucent black background, shown when the name is visible or the
//! mob is under the crosshair within its name-tag distance (64 by default), as its team's name tag
//! visibility allows, in the team's colours, with its score in the
//! objective displayed below names. Other players' names (on a LAN host
//! or a server) show whenever `LivingEntityRenderer.shouldShowName` allows,
//! and the player's own in third person with the `ownNameTag` extra.
use super::Game;
use horizon_client::mobs::Who;

impl Game<'_> {
    /// A named name tag names the mob under the crosshair.
    pub(super) fn name_mob(&mut self, id: u32) -> bool {
        // `NameTagItem`: the mob takes the tag's `custom_name` component, style and all.
        let Some((name, text)) = self
            .held()
            .filter(|h| &*h.item == "minecraft:name_tag")
            .and_then(|h| {
                let text = h.data()?.other("minecraft:custom_name");
                Some((
                    h.custom_name()?,
                    text.map(horizon_client::entity_nbt::name_text),
                ))
            })
        else {
            return false;
        };
        if !self.ecs.alive(id) {
            return false;
        }
        let entity_id = id;
        let entity_name = name.clone();
        // NameTagItem.interactLivingEntity: setCustomName with the tag's whole name, and the persistence it sets.
        let tag = match &text {
            Some(text) => horizon_client::scoreboard::component_tag(&text.component()),
            None => horizon_protocol::text::literal(&name),
        };
        let mut patch = horizon_core::nbt::Compound::new();
        patch.insert("CustomName".into(), tag);
        patch.insert("PersistenceRequired".into(), horizon_core::nbt::Tag::Byte(1));
        self.ecs_merge_mob(id, &patch);
        let Some(at) = self.ecs.position(id) else {
            return false;
        };
        // itemStack.shrink(1); `Player.interactOn` puts a creative player's
        // count back, which `consume_held` skips for them.
        self.consume_held();
        // `Mob.interact`: the tag consumed the use, so
        // `gameEvent(GameEvent.ENTITY_INTERACT, player)` at the mob.
        let ctx = self.who_context(Some(Who::Player), None);
        self.post_game_event("minecraft:entity_interact", at, ctx);
        self.swing_arm();
        if let (Some(player), Some(player_name)) =
            (self.lan_acting_uuid, self.lan_acting_name.clone())
        {
            self.native_player_name_entity(player, &player_name, entity_id, &entity_name);
        }
        true
    }
    /// The local player's profile name as others see it.
    pub(super) fn own_profile_name(&self) -> String {
        self.acting_player_name().to_owned()
    }
}
