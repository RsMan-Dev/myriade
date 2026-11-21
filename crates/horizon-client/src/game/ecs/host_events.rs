//! The outputs the ECS hands back to the host (`HostEvent`), applied by the host's own functions: one arm per variant, none left to `_`.
use super::super::{Game, Result};
use super::super::mobs::creatures::section_steps::teleport_player;
use bevy_ecs::entity::Entity;
use horizon_client::particles::{ParticleOptions, ParticleType};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::host_out_vehicle_push::VehiclePushEvent;
use horizon_ecs_proto::targeting::Who;

impl Game<'_> {
    /// The network id of `e`, linked on first use from the one counter (`Entity.nextEntityId`).
    pub(in crate::game) fn entity_id(&mut self, e: Entity) -> u32 {
        let next = &mut self.next_entity;
        self.ecs.id_for(e, &mut || {
            *next += 1;
            *next
        })
    }

    pub(in crate::game) fn host_events(&mut self, evs: Vec<HostEvent>) -> Result<()> {
        for ev in evs {
            match ev {
                HostEvent::SetBlock { pos, state } => self.level_set(pos, state)?,
                HostEvent::Sound { name, at, volume, pitch } => {
                    self.audio.play(&horizon_client::mobs::entity_sounds::full_id(name), Some(at), volume, pitch)
                }
                HostEvent::Particles { kind, at, spread, speed, count } => {
                    if let Some(ty) = ParticleType::from_name(kind) {
                        let options = ParticleOptions::simple(ty);
                        self.particles.send_particles(&options, at, spread, speed, count);
                        self.lan_level_particles(&options, at, spread, speed, count, None);
                    }
                }
                HostEvent::EntityEvent { entity, code } => {
                    let id = self.entity_id(entity);
                    if let Some(at) = self.ecs.position(id) {
                        // Ravager.roar (event 69) also pushes the armor stands in reach, with mob griefing on. The box is the
                        // ravager's (1.95 wide, 2.2 high) inflated by 4, from its position: an approximation of its bounds.
                        if code == 69 && self.ui.rules.bool_at(horizon_client::gamerules::vanilla::MOB_GRIEFING) {
                            let (min, max) = (Vec3::new(at[0] - 4.975, at[1] - 4.0, at[2] - 4.975), Vec3::new(at[0] + 4.975, at[1] + 6.2, at[2] + 4.975));
                            self.roar_pushes_armor_stands(Aabb::new(min, max), at);
                        }
                        self.lan_entity_event(id as i32, code as i8, at);
                    }
                }
                HostEvent::Darkness { at, radius } => self.apply_darkness(at, radius),
                // Boss bars of the mobs: `add_bar`/`remove_bar` take a text name and the bar's value: not wired yet (gap).
                HostEvent::BossBar { .. } => log::warn!("ecs: boss bar event not wired yet"),
                HostEvent::PushPlayer { player, velocity } => {
                    self.with_player(player, |g| g.push_own_player(Vec3::new(velocity[0], velocity[1], velocity[2])));
                }
                HostEvent::ParrotToShoulder { parrot, player: _ } => {
                    let id = self.entity_id(parrot);
                    self.parrot_to_shoulder(id)
                }
                // The effects and colour of the cloud are not in the event: not wired yet (gap).
                HostEvent::Cloud { .. } => log::warn!("ecs: area effect cloud event not wired yet"),
                // `display::set` (horizon-ecs-proto display.rs:106) emits this, but `Modified` is pushed only when a `NetId` is added
                // (bridge/ids.rs:33), so `drain_modified` does not return the entity: the resend is not wired (gap).
                HostEvent::DisplayUpdate { .. } => log::warn!("ecs: display update not wired yet"),
                HostEvent::LevelEvent { event, at, data } => self.level_event_at(event, at, data),
                HostEvent::GameEvent { name, at } => {
                    let ctx = self.who_context(None, None);
                    self.post_game_event(name, at, ctx)
                }
                HostEvent::GameEventBy { entity, name, at, ground, sneaking, dampens } => {
                    let id = self.entity_id(entity);
                    self.post_move_event(id, name, at, ground, sneaking, dampens)
                }
                HostEvent::Xp { at, amount } => self.add_orb(at, amount),
                HostEvent::Pearl { player, to, damage, endermite } => {
                    self.with_player(player, |g| g.pearl_lands(to, damage, endermite));
                }
                HostEvent::PullEntity { entity, velocity } => {
                    let id = self.entity_id(entity);
                    self.add_mob_velocity(id, Vec3::new(velocity[0], velocity[1], velocity[2]));
                }
                HostEvent::ChestOpen { chest, open, entity } => {
                    let id = self.entity_id(entity);
                    self.golem_chest_open(chest, open, id)
                }
                HostEvent::ChestSlot { chest, slot, stack } => self.golem_chest_slot(chest, slot, stack),
                HostEvent::GolemState { entity, state } => {
                    let id = self.entity_id(entity);
                    if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
                        m.extra.variant3 = state;
                    }
                }
                HostEvent::Arrow(ev) => self.ecs_arrow_event(ev),
                HostEvent::Inside(ev) => self.ecs_inside_event(ev),
                HostEvent::BoatEffect(ev) => self.ecs_boat_effect_event(ev),
                HostEvent::BoatHurt(ev) => self.ecs_boat_hurt_event(ev),
                HostEvent::Boat(ev) => self.ecs_boat_event(ev),
                HostEvent::CartMove(ev) => self.ecs_cart_move_event(ev),
                HostEvent::Cart(ev) => self.ecs_cart_event(ev)?,
                HostEvent::Falling(ev) => self.ecs_falling(ev),
                HostEvent::Firework(ev) => self.apply_firework_events([ev]),
                HostEvent::Frame(ev) => self.ecs_frame_events(vec![ev]),
                HostEvent::General(ev) => self.ecs_general_events([ev]),
                HostEvent::Hook(ev) => self.ecs_hook_event(ev),
                HostEvent::Misc(ev) => self.ecs_misc_events([ev]),
                HostEvent::Seat(ev) => self.ecs_seat_event(ev),
                HostEvent::Tnt(ev) => self.tnt_event(ev)?,
                HostEvent::Trident(ev) => self.ecs_trident_event(ev),
                HostEvent::VehiclePush(VehiclePushEvent::PushPlayer { player: Who::Player(id), impulse, .. }) => {
                    self.with_player(id, |g| g.push_own_player(Vec3::new(impulse[0], impulse[1], impulse[2])));
                }
                // A push on an entity that is not a player: P applied it to its own components (`vehicle_push`), nothing is left to play.
                HostEvent::VehiclePush(VehiclePushEvent::PushPlayer { .. }) => {}
                HostEvent::Equipment(ev) => self.ecs_equipment_event(ev),
                HostEvent::Item(ev) => self.ecs_item_event(ev),
                HostEvent::Using(ev) => self.ecs_using_event(ev),
                HostEvent::Death { entity, cause, attacker, at } => self.ecs_death_event(entity, cause, attacker, at),
                HostEvent::ConversionAsk { entity, into } => self.ecs_conversion_asked(entity, into),
                // The `Some` case reached the mob handler through `apply`; a cleared target is told to `living_change_target`.
                HostEvent::TargetChanged { entity, target: None, previous } => self.ecs_target_cleared(entity, previous),
                HostEvent::TargetChanged { target: Some(_), .. } => {}
                HostEvent::TotemAsk { entity, off_hand, cause, attacker } => self.ecs_totem_asked(entity, off_hand, cause, attacker),
                HostEvent::Spawned { entity, natural } => self.ecs_spawned_event(entity, natural),
                HostEvent::Tamed { entity, owner } => self.ecs_tamed_event(entity, owner),
                // P applied the damage; the mods' `hurt_enemy` hears the stack the attacker holds (only emitted while one listens).
                HostEvent::HitDealt { attacker, victim, .. } => self.ecs_hit_dealt(attacker, victim),
                HostEvent::Unleashed { entity } => self.ecs_unleashed_event(entity),
            }
        }
        Ok(())
    }

    /// Runs `f` on the player `id` as this `Game` holds it: the host, or the guest whose entity it is (swapped in for the call).
    pub(in crate::game) fn with_player<R>(&mut self, id: u32, f: impl FnOnce(&mut Self) -> R) -> Option<R> {
        if self.player_entity_id_by_connection(None) == Some(id as i32) {
            return Some(f(self));
        }
        let conns: Vec<u32> = self.lan.as_ref().map_or_else(Vec::new, |h| h.hub.online_players().map(|g| g.conn).collect());
        for conn in conns {
            if self.as_guest(conn, |g| g.player_entity_id_by_connection(Some(conn)) == Some(id as i32)) == Some(true) {
                return self.as_guest(conn, f);
            }
        }
        None
    }

    fn push_own_player(&mut self, v: Vec3) {
        if let Some(p) = self.player.as_mut() {
            p.velocity = p.velocity.add_vector(v);
        }
    }

    /// A velocity added to a creature (the `MobEvent::Push` arm, as `Game.mobs` still holds it).
    fn add_mob_velocity(&mut self, id: u32, v: Vec3) {
        if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
            m.velocity = m.velocity.add_vector(v);
        }
    }

    /// `ThrownEnderpearl`: 5% endermite where the player stood (when monsters spawn), the teleport, 5 damage. The ECS
    /// has already rolled the endermite, so `endermite` is that roll.
    fn pearl_lands(&mut self, to: [f64; 3], damage: f32, endermite: bool) {
        let Some(from) = self.player.as_ref().map(|pl| pl.position) else { return };
        if endermite && self.ui.rules.bool_at(horizon_client::gamerules::vanilla::SPAWN_MONSTERS) && self.ui.difficulty != 0 {
            self.spawn_mob_raw("minecraft:endermite", [from.x, from.y, from.z], false);
        }
        teleport_player(self, Vec3::new(to[0], to[1], to[2]));
        self.hurt_player_by("minecraft:ender_pearl", damage);
        self.audio.play("minecraft:entity.player.teleport", Some(to), 1.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use crate::game::game_tests::tests::server_game;
    use horizon_client::chunk::Chunk;
    use horizon_ecs_proto::host_out::HostEvent;
    use horizon_world::registry::BlockStateId;
    use std::sync::Arc;

    /// Vanilla's stone is the state 1; the event writes it through `level_set`, so the chunk holds it.
    #[test]
    fn a_set_block_event_changes_the_block_of_the_chunk() {
        let mut resources = None;
        let mut game = server_game(&mut resources);
        game.world.chunks.insert((0, 0), Arc::new(Chunk::empty(0, 0)));
        game.host_events(vec![HostEvent::SetBlock { pos: [8, 65, 8], state: BlockStateId(1) }]).unwrap();
        assert_eq!(game.world.block(8, 65, 8).0, 1);
    }
}
