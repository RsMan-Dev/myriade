//! The outputs the ECS hands back to the host (`HostEvent`), applied by the host's own functions: one arm per variant, none left to `_`.
use super::super::{Game, Result};
use super::super::creatures::section_steps::teleport_player;
use bevy_ecs::entity::Entity;
use myriade_engine::particles::{ParticleOptions, ParticleType};
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
                HostEvent::BlockPlace { entity, at, state } => {
                    let id = self.entity_id(entity);
                    let ctx = self.mob_context(id, Some(state));
                    self.post_game_event("minecraft:block_place", at, ctx)
                }
                HostEvent::BlockBreak { at, drop, breaker } => {
                    let by = breaker.map(|e| self.ecs_id_of(e));
                    if let Err(error) = self.destroy_block_as(at, drop, by) {
                        log::warn!("an entity's block break failed: {:?}", error.message);
                    }
                }
                HostEvent::Sound { name, at, volume, pitch, source } => {
                    let name = myriade_engine::mobs::entity_sounds::full_id(&name);
                    match source {
                        Some(source) => self.audio.play_on(source, &name, Some(at), volume, pitch),
                        None => self.audio.play(&name, Some(at), volume, pitch),
                    }
                }
                HostEvent::Particles { kind, at, spread, speed, count } => {
                    if let Some(ty) = ParticleType::from_name(kind) {
                        let options = ParticleOptions::simple(ty);
                        self.lan_level_particles(&options, at, spread, speed, count, None);
                    }
                }
                // The bridge turns it into `MobEvent::LandingDust` first: nothing is left here.
                HostEvent::BlockParticles { .. } => {}
                HostEvent::EntityEvent { entity, code } => {
                    let id = self.entity_id(entity);
                    if let Some(at) = self.ecs.position(id) {
                        self.lan_entity_event(id as i32, code as i8, at);
                    }
                }
                HostEvent::ComposterWork { entity, pos } => {
                    let id = self.entity_id(entity);
                    self.villager_compost_items_ecs(id, pos);
                }
                HostEvent::Darkness { at, radius } => self.apply_darkness(at, radius),
                HostEvent::BossBar { id, name, progress, color, remove } => self.on_boss_bar(id, name, progress, color, remove),
                // The dragon's phase is read through its view (`Game::dragon_views`): nothing to apply here.
                HostEvent::DragonPhase { .. } => {}
                HostEvent::Fight(e) => self.on_fight_event(e),
                HostEvent::PushPlayer { player, velocity } => {
                    self.with_player(player, |g| g.push_own_player(Vec3::new(velocity[0], velocity[1], velocity[2])));
                }
                HostEvent::ParrotToShoulder { parrot, player: _ } => {
                    let id = self.entity_id(parrot);
                    self.parrot_to_shoulder(id)
                }
                // The effects and colour of the cloud are not in the event: not wired yet (gap).
                HostEvent::Cloud { .. } => {}
                HostEvent::CloudHit { at, amount, amplifier, players, mobs } => {
                    let mobs: Vec<u32> = mobs.into_iter().map(|m| self.entity_id(m)).collect();
                    self.ecs_cloud_hit(at, amount, amplifier, players, mobs)
                }
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
                HostEvent::BedOccupied { pos, occupied } => self.set_bed_occupied(pos, occupied),
                HostEvent::CuredZombieVillager { villager, player } => self.ecs_cured_zombie_villager(villager, player),
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
                HostEvent::RaiderDied { raid, id, by, wave, leader, health } => self.raider_died(raid, id, by, wave, leader, health),
                HostEvent::RaiderJoined { raid, id, wave, leader, loaded } => self.ecs_raider_joined(raid, id, wave, leader, loaded),
                HostEvent::RaiderLeads { raid, id, wave } => self.ecs_raider_leads(raid, id, wave),
                HostEvent::RaiderHurt { raid, id, before } => self.ecs_raider_hurt(raid, id, before),
                HostEvent::Hurt { id, damage, cause, direct, at, .. } => self.ecs_hurt_event(id, damage, cause, direct, at),
                HostEvent::DeathAnimation { id, at } => self.lan_entity_event(id as i32, 3, at),
                HostEvent::DeathSmoke { id, at, .. } => {
                    self.lan_entity_event(id as i32, 60, at);
                }
                HostEvent::ConversionAsk { entity, into } => self.ecs_conversion_asked(entity, into),
                // The target change reached the mob handler through `apply`.
                HostEvent::TargetChanged { .. } => {}
                HostEvent::TargetAsk { entity, .. } => self.ecs_target_asked(entity),
                HostEvent::TotemAsk { entity, .. } => self.ecs_totem_asked(entity),
                HostEvent::Spawned { .. } => {}
                HostEvent::Tamed { .. } => {}
                // P applied the damage: nothing more to tell.
                HostEvent::HitDealt { .. } => {}
                HostEvent::Unleashed { .. } => {}
                HostEvent::Swing { entity, off_hand } => self.ecs_swing_event(entity, off_hand),
                HostEvent::HeldConsume { entity, stack, seed, silent } => self.ecs_held_consume_event(entity, stack, seed, silent),
                HostEvent::HeldUsed { player, item } => self.ecs_held_used(player, item),
                HostEvent::HeldWorn { player, item, amount } => self.ecs_held_worn(player, item, amount),
                HostEvent::HeldGiven { player, stack } => self.ecs_held_given(player, stack),
                HostEvent::BecomeBlock { entity, block, at, pose, yaw, name, lead } => self.ecs_become_block(entity, block, at, pose, yaw, name, lead),
                HostEvent::HeldSwap { player, from, to } => self.ecs_held_swap(player, from, to),
                HostEvent::PlayerGameEvent { player, name, at } => {
                    self.with_player(player, |g| {
                        let ctx = g.who_context(Some(myriade_engine::mobs::Who::Player), None);
                        g.post_game_event(name, at, ctx)
                    });
                }
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

    /// `HostEvent::HeldWorn`: the tool `item` of the player's hand (main first) takes `amount` uses of wear.
    fn ecs_held_worn(&mut self, player: u32, item: u32, amount: u32) {
        let Some(name) = horizon_ecs_proto::items::item_name(item) else { return };
        self.with_player(player, |g| {
            // `ItemStack.hurtAndBreak`: a creative player's tool takes no wear; the hand that holds it is the one that breaks.
            if g.creative {
                return;
            }
            for hand in [myriade_types::Hand::Main, myriade_types::Hand::Off] {
                if g.with_hand(hand, |g| g.held().is_some_and(|s| &*s.item == name)) {
                    g.with_hand(hand, |g| {
                        let slot = g.acting_hand_slot();
                        g.damage_held_in_slot(amount, slot);
                    });
                    return;
                }
            }
        });
    }

    /// `HostEvent::HeldGiven`: `Player.addItem` (what does not fit is lost, the result is ignored as in vanilla's callers).
    fn ecs_held_given(&mut self, player: u32, stack: myriade_engine::items::ItemStack) {
        self.with_player(player, |g| {
            g.add_stack_to_inventory(stack);
        });
    }

    /// `HostEvent::HeldSwap` (`ItemUtils.createFilledResult`): one `from` leaves the hand that holds it and `to` takes its place, else
    /// goes to the inventory; creative keeps `from` and `to` is added only when no slot holds the same item and components
    /// (`createFilledResult`). What does not fit is lost (as `ecs_held_given`).
    fn ecs_held_swap(&mut self, player: u32, from: u32, to: myriade_engine::items::ItemStack) {
        let Some(name) = horizon_ecs_proto::items::item_name(from) else { return };
        self.with_player(player, |g| {
            if g.creative {
                let held = g.inventory.iter().chain(g.armor.iter()).chain(std::iter::once(&g.offhand)).flatten().any(|s| s.same_item_same_components(&to));
                if !held {
                    g.add_stack_to_inventory(to);
                }
                return;
            }
            for hand in [myriade_types::Hand::Main, myriade_types::Hand::Off] {
                if g.with_hand(hand, |g| g.held().is_some_and(|s| &*s.item == name)) {
                    g.with_hand(hand, Game::consume_held);
                    g.with_hand(hand, |g| g.add_stack_to_inventory(to));
                    return;
                }
            }
        });
    }

    /// `HostEvent::HeldUsed`: one `item` leaves the hand of the player that holds it, the main hand first (nothing in creative).
    fn ecs_held_used(&mut self, player: u32, item: u32) {
        let Some(name) = horizon_ecs_proto::items::item_name(item) else { return };
        self.with_player(player, |g| {
            for hand in [myriade_types::Hand::Main, myriade_types::Hand::Off] {
                let held = g.with_hand(hand, |g| g.held().is_some_and(|s| &*s.item == name));
                if held {
                    g.with_hand(hand, Game::consume_held);
                    return;
                }
            }
        });
    }

    fn push_own_player(&mut self, v: Vec3) {
        if let Some(p) = self.player.as_mut() {
            p.velocity = p.velocity.add_vector(v);
        }
    }

    /// A velocity added to a creature (the `MobEvent::Push` arm, as `Game.mobs` still holds it).
    fn add_mob_velocity(&mut self, id: u32, v: Vec3) {
        self.ecs.push(id, [v.x, v.y, v.z]);
    }

    /// `CuredZombieVillagerTrigger` for the player `by` (a guest, else the acting player): the zombie, read as the villager's position
    /// and kind `zombie_villager` (the zombie left at the cure), and the villager it became.
    fn ecs_cured_zombie_villager(&mut self, villager: Entity, by: u32) {
        let id = self.entity_id(villager);
        let Some(v) = self.ecs_host_view(id) else { return };
        let zombie = myriade_engine::advancements::EntityView { kind: "minecraft:zombie_villager".into(), position: v.position, ..Default::default() };
        let guest = self.lan.as_ref().and_then(|h| h.hub.guests.iter().find(|g| g.entity_id as u32 == by).map(|g| g.conn));
        match guest {
            Some(conn) => {
                self.as_guest(conn, |g| g.trigger_cured_zombie(&zombie, &v));
            }
            None if self.player.as_ref().is_some_and(|me| me.entity_id as u32 == by) => self.trigger_cured_zombie(&zombie, &v),
            None => {}
        }
    }
    /// `ThrownEnderpearl`: 5% endermite where the player stood (when monsters spawn), the teleport, 5 damage. The ECS
    /// has already rolled the endermite, so `endermite` is that roll.
    fn pearl_lands(&mut self, to: [f64; 3], damage: f32, endermite: bool) {
        let Some(from) = self.player.as_ref().map(|pl| pl.position) else { return };
        if endermite && self.session.rules.bool_at(myriade_engine::gamerules::vanilla::SPAWN_MONSTERS) && self.session.difficulty != 0 {
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
    use myriade_engine::chunk::Chunk;
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
