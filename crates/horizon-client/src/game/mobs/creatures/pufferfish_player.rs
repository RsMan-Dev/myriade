//! Pufferfish.playerTouch: the ECS reports each touch of an inflated pufferfish (`MiscEvent::PufferSting`); the hurt, the sting and the
//! poison are applied here, to the player it names.
use super::super::Game;
use horizon_client::scoreboard::teams_allow_damage;
use horizon_protocol::packets::play::{Clientbound, GameEvent};

/// `ClientboundGameEventPacket.PUFFER_FISH_STING`.
const PUFFER_FISH_STING: u8 = 9;

impl Game<'_> {
    /// `Pufferfish.playerTouch` for the ECS pufferfish `fish` and the player of network id `player` (a guest or the own player).
    pub(in crate::game) fn ecs_puffer_sting(
        &mut self,
        fish: bevy_ecs::entity::Entity,
        player: u32,
        size: u8,
        silent: bool,
    ) {
        let id = self.ecs_id_of(fish);
        let guest = self
            .lan
            .as_ref()
            .and_then(|host| host.hub.guests.iter().find(|g| g.entity_id as u32 == player).map(|g| g.conn));
        if let Some(conn) = guest {
            self.as_guest(conn, |g| g.puffer_sting_own(id, size, silent));
        } else if self.player.as_ref().is_some_and(|me| me.entity_id as u32 == player) {
            self.puffer_sting_own(id, size, silent);
        }
    }
    /// The sting on the acting player: `doTeamsAllowDamage`, accepted player damage (1 + puff), the sting game event, then poison
    /// with this fish as source.
    fn puffer_sting_own(&mut self, id: u32, puff: u8, silent: bool) {
        if self.spectator() || self.player.as_ref().is_none_or(|p| p.health.dead_or_dying()) {
            return;
        }
        let Some(view) = self.ecs_view(id) else { return };
        if !teams_allow_damage(self.mob_team(id), self.player_team()) {
            return;
        }
        let puff = i32::from(puff);
        if !self.hurt_player_from(
            (1 + puff) as f32,
            Some(view.position),
            "minecraft:mob_attack",
            Some(Some(id)),
            false,
        ) {
            return;
        }
        // `if (!this.isSilent())` inside the `hurtServer` branch: before the poison.
        if !silent {
            self.puffer_fish_sting();
        }
        let changes = self.potions.effects.changes;
        self.give_player_effect("minecraft:poison", 60 * puff, 0);
        if self.potions.effects.changes != changes {
            let source = self.ecs_predicate_view(id);
            self.trigger_effects_changed_by(Some(source));
        }
    }
    /// `ServerPlayer.connection.send(ClientboundGameEventPacket(PUFFER_FISH_STING, 0.0F))`: the
    /// stung player's own client plays it (`ClientPacketListener.handleGameEvent`). A server has
    /// no player of its own, so the packet is the only way to reach it.
    fn puffer_fish_sting(&mut self) {
        self.guest_packets.push(Clientbound::GameEvent(GameEvent {
            event: PUFFER_FISH_STING,
            param: 0.0,
        }));
    }
}

