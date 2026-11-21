//! The trident events of the ECS (`TridentEvent`), applied by the host's own functions: the blow (advancement, absorption and the hurt of
//! the player it reaches), the thunder, the landing and the return sounds, and the pickup (the stack back to its taker).
use horizon_core::random::RandomSource as _;
use super::super::Game;
use horizon_client::{enchant::EffectContext, items::ItemStack};
use horizon_core::random::LegacyRandom;
use horizon_ecs_proto::{host_out_trident::TridentEvent, targeting::Who};

impl Game<'_> {
    /// The enchantment bonus of a blow for the level's hit table: the damage the host's enchantments give a trident of `stack` striking
    /// a victim of species `kind`, over its base blow of 8 (Impaling reads the victim). It shares the level's enchantment and item tables.
    pub(in crate::game) fn trident_hit_bonus_fn(&self) -> Box<dyn Fn(&ItemStack, Who, &str) -> f32 + Send + Sync> {
        let enchants = world_stations!(self).enchantments.clone();
        let items = self.server_items.clone().or_else(|| self.world_tables.as_ref().map(|t| t.items.clone()));
        Box::new(move |stack, _victim, kind| {
            // No item table yet (the world's tables are staged later): no bonus until the bonus is set again.
            let Some(items) = items.as_deref() else { return 0.0 };
            let levels = stack.all_enchantments();
            let ctx = EffectContext {
                items: Some(items),
                this: Some(kind),
                attacker: Some("minecraft:player"),
                direct_attacker: Some("minecraft:trident"),
                is_direct: true,
                damage_type: Some("minecraft:trident"),
                ..Default::default()
            };
            let mut random = LegacyRandom::new(0);
            enchants.modify_damage(&levels, &ctx, &mut random, 8.0, &mut |_, _, _, value| value) - 8.0
        })
    }

    /// The connection of the guest whose entity id is `id`, None for this game's player or a mob.
    fn ecs_guest_conn(&self, who: Option<Who>) -> Option<u32> {
        let Some(Who::Player(id)) = who else { return None };
        self.lan.as_ref()?.hub.guests.iter().find(|g| g.entity_id as u32 == id).map(|g| g.conn)
    }

    pub(in crate::game) fn ecs_trident_event(&mut self, ev: TridentEvent) {
        match ev {
            TridentEvent::Hit { entity, by, victim, base } => {
                let id = self.ecs_id_of(entity);
                let Some(at) = self.ecs_view(id).map(|v| v.position) else { return };
                let guest = self.ecs_guest_conn(by);
                match victim {
                    Who::Player(_) => {
                        self.thrower_dealt_absorbed(guest);
                        match self.ecs_guest_conn(Some(victim)) {
                            Some(conn) => self.guest_hit_by(conn, |g| g.mob_hits_player(base, at, "minecraft:trident")),
                            None => self.mob_hits_player(base, at, "minecraft:trident"),
                        }
                    }
                    Who::Entity(e) => {
                        let vid = self.ecs_id_of(e);
                        if let Some(view) = self.ecs_host_view(vid) {
                            let direct = horizon_client::advancements::EntityView {
                                kind: "minecraft:trident".into(),
                                position: at,
                                ..Default::default()
                            };
                            self.thrower_dealt_absorbed(guest);
                            self.thrower_hit(guest, &view, direct, base);
                        }
                    }
                }
                self.projectile_landed_event(id, Some(crate::mobs::Who::Player), at);
                self.audio.play("minecraft:item.trident.hit", Some(at), 1.0, 1.0);
            }
            TridentEvent::Thunder { at, cause } => {
                let guest = self.ecs_guest_conn(cause);
                self.channel_lightning(at, guest);
            }
            TridentEvent::Landed { entity } => {
                let id = self.ecs_id_of(entity);
                if let Some(at) = self.ecs_view(id).map(|v| v.position) {
                    self.audio.play(horizon_client::entity_nbt::TRIDENT_HIT_GROUND, Some(at), 1.0, 1.0);
                }
            }
            TridentEvent::Returned { entity, .. } => {
                let id = self.ecs_id_of(entity);
                if let Some(at) = self.ecs_view(id).map(|v| v.position) {
                    self.audio.play("minecraft:item.trident.return", Some(at), 10.0, 1.0);
                }
            }
            TridentEvent::PickedUp { entity, by, stack } => {
                let id = self.ecs_id_of(entity);
                let Some(at) = self.ecs_view(id).map(|v| v.position) else { return };
                if let (Some(conn), Who::Player(eid)) = (self.ecs_guest_conn(Some(by)), by) {
                    let rest = self.as_guest(conn, |g| g.add_stack_to_inventory(stack.clone()));
                    if let Some(Some(rest)) = rest {
                        self.spawn_item(rest, at, [0.0; 3], 0);
                    }
                    if let Some(host) = self.lan.as_ref() {
                        host.hub.take_item(id as i32, eid as i32, 1, at);
                    }
                } else {
                    let rest = self.add_stack_to_inventory(stack);
                    if let Some(rest) = rest {
                        self.spawn_item(rest, at, [0.0; 3], 0);
                    }
                    let pitch = ((self.world.random.next_float() - self.world.random.next_float()) * 0.7 + 1.0) * 2.0;
                    self.audio.play("minecraft:entity.item.pickup", Some(at), 0.2, pitch);
                }
            }
        }
    }
}
