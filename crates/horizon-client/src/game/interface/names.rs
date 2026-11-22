//! Custom names: `NameTagItem.interactLivingEntity` and the name tag
//! (`EntityRenderer.submitNameTag`): above the head, facing the camera,
//! on a translucent black background, shown when the name is visible or the
//! mob is under the crosshair within its name-tag distance (64 by default), as its team's name tag
//! visibility allows, in the team's colours, with its score in the
//! objective displayed below names. Other players' names (on a LAN host
//! or a server) show whenever `LivingEntityRenderer.shouldShowName` allows,
//! and the player's own in third person with the `ownNameTag` extra.
use super::Game;
use horizon_client::mobs::{Mob, Who};

impl Game<'_> {
    /// A named name tag names the mob under the crosshair.
    pub(super) fn name_mob(&mut self, index: usize) -> bool {
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
        let m = &mut self.remote_entities[index];
        if !m.alive() {
            return false;
        }
        let entity_id = m.id;
        let entity_name = name.clone();
        // NameTagItem.interactLivingEntity: setCustomName with the tag's whole name.
        m.custom_name = Some(name);
        m.custom_name_text = text;
        m.persistent = true;
        let at = [m.position.x, m.position.y, m.position.z];
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

/// `Entity.DATA_CUSTOM_NAME` of a mob named `name`: its component with its style (a name
/// summoned or set as `{text: "..", color: "red"}` keeps its colour), else the plain text.
pub(super) fn name_component(m: &Mob, name: &str) -> horizon_core::nbt::Tag {
    match &m.custom_name_text {
        Some(text) => horizon_client::scoreboard::component_tag(&text.component()),
        None => horizon_protocol::text::literal(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mobs_name_is_sent_with_its_style() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/horizon-client")
            .join(horizon_core::VANILLA_VERSION);
        let Ok(types) = horizon_client::mobs::MobTypes::load(&root) else {
            return;
        };
        let cow = Arc::new(types.types["minecraft:cow"].clone());
        let mut m = Mob::new(1, cow, Vec3::new(0.0, 64.0, 0.0), 0.0);
        m.custom_name = Some("RED NAME".into());
        let plain = horizon_protocol::text::literal("RED NAME");
        assert_eq!(name_component(&m, "RED NAME"), plain);
        let styled = horizon_core::nbt::Tag::compound([
            ("text", horizon_core::nbt::Tag::String("RED NAME".into())),
            ("color", horizon_core::nbt::Tag::String("red".into())),
        ]);
        m.custom_name_text = Some(horizon_client::entity_nbt::name_text(&styled));
        let sent = horizon_client::entity_nbt::name_text(&name_component(&m, "RED NAME"));
        assert_eq!(sent, horizon_client::entity_nbt::name_text(&styled));
        assert_ne!(sent, horizon_client::entity_nbt::name_text(&plain));
    }

}
