//! The minecart events of the ECS (`CartMoveEvent`, `CartEvent`), applied by the host's own functions. The carts are simulated by P: the
//! host keeps no list of them, and an event names the cart by its ECS entity.
use super::super::Game;
use crate::ecs_bridge::cart_variants::CartAttacker;
use crate::game::commands::Target;
use crate::game::minecarts::CartKind;
use crate::mobs::Who;
use horizon_client::Result;
use horizon_ecs_proto::{cart_variants::HitResult, host_out_cart_move::CartMoveEvent, host_out_cart_variants::CartEvent, targeting};

impl Game<'_> {
    /// A blow on the cart `id` through the bridge: the damage, the break and the drop are P's. A fire lights a TNT cart first. True when hurt or broken.
    pub(in crate::game) fn hurt_cart_id(&mut self, id: u32, damage: f32, creative: bool, fire: bool, attacker: Option<CartAttacker>) -> bool {
        if fire {
            self.ecs.cart_ignite(id, None);
        }
        let hit = self.ecs_with(|ecs, mw| ecs.cart_hit(mw, id, damage, attacker, creative));
        matches!(hit, HitResult::Hurt | HitResult::Broke { .. })
    }

    /// The credit of a projectile's blow: its owner, a guest, or the host's player in place.
    pub(in crate::game) fn projectile_credit(&self, owner: Option<Who>, guest: Option<u32>) -> Option<CartAttacker> {
        match owner {
            Some(Who::Player) => guest.or_else(|| self.acting_player_uuid_id()).map(CartAttacker::Player),
            Some(Who::Mob(m)) => Some(CartAttacker::Mob(m)),
            None => None,
        }
    }

    /// The bridge's credit for a command or blast source that struck a cart: the host's player, a guest, or a mob.
    pub(in crate::game) fn target_credit(&self, by: Option<Target>) -> Option<CartAttacker> {
        match by? {
            Target::Player => self.acting_player_uuid_id().map(CartAttacker::Player),
            Target::Guest(g) => Some(CartAttacker::Player(g)),
            Target::Mob(m) => Some(CartAttacker::Mob(m)),
            _ => None,
        }
    }

    pub(in crate::game) fn ecs_cart_move_event(&mut self, ev: CartMoveEvent) {
        match ev {
            CartMoveEvent::Derail { entity } => {
                let id = self.ecs_id_of(entity);
                log::warn!("ecs: cart {id} derailed: its sound is not wired yet");
            }
            CartMoveEvent::PoweredBoost { entity } => {
                let id = self.ecs_id_of(entity);
                log::warn!("ecs: cart {id} boosted by a powered rail: its sound is not wired yet");
            }
        }
    }

    pub(in crate::game) fn ecs_cart_event(&mut self, ev: CartEvent) -> Result<()> {
        match ev {
            CartEvent::Fuse { entity } => {
                let id = self.ecs_id_of(entity);
                let Some(at) = self.ecs_view(id).map(|v| v.position) else { return Ok(()) };
                self.audio.play_on(horizon_client_core::sound_source::SOUND_SOURCES.iter().position(|s| *s == "block").unwrap_or(0), "minecraft:entity.tnt.primed", Some(at), 1.0, 1.0);
            }
            CartEvent::Explode { at, power, .. } => self.tnt_blast(at, power, None)?,
            CartEvent::Broke { entity, attacker, .. } => {
                let id = self.ecs_id_of(entity);
                // The drop is P's; only a player's blow angers the piglins of a container cart.
                let container = self.ecs.cart_kind(id).and_then(CartKind::from_id).is_some_and(|k| k.slots() > 0);
                if matches!(attacker, Some(targeting::Who::Player(_))) && container {
                    self.anger_piglins(true);
                }
            }
            CartEvent::HopperPulled { entity, .. } => {
                let id = self.ecs_id_of(entity);
                log::warn!("ecs: hopper cart {id} pulled an item: its pickup is not wired yet");
            }
            CartEvent::SpawnerTick { entity } => {
                let id = self.ecs_id_of(entity);
                log::warn!("ecs: cart {id} spawner tick: not wired yet");
            }
            CartEvent::CommandRun { entity, .. } => {
                let id = self.ecs_id_of(entity);
                self.perform_cart_command(id);
            }
            CartEvent::OpenContents { entity, player } => {
                let id = self.ecs_id_of(entity);
                self.open_cart_container(id, player);
            }
            CartEvent::Signal { entity, .. } => {
                let id = self.ecs_id_of(entity);
                log::warn!("ecs: cart {id} redstone signal: not wired yet");
            }
        }
        Ok(())
    }
}
