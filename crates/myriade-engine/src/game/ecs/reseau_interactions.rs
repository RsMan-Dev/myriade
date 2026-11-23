//! Lot 13 (colle) : les frontières réseau d'interaction (attaque, usage, marchand, monture, véhicule) lisent et agissent sur l'ECS
//! par id. Ce qui est dans `Game.ecs` passe par `EcsLevel`.
//! Imports à ajouter au fichier d'accueil : `horizon_core::geometry::{Aabb, Vec3}`, `myriade_engine::items::ItemStack`, `myriade_engine::mobs::equine`, `horizon_ecs_proto::bridge::host::HurtSource`, `horizon_protocol::packets::play::{self, Clientbound as C}`.
use super::super::Game;
use myriade_engine::{items::ItemStack, mobs::Who};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::bridge::host::HurtSource;
use horizon_protocol::packets::play::{self, Clientbound as C};

impl Game<'_> {
    /// The box of followed entity `id` (`EcsLevel::bounds`).
    pub(in crate::game) fn mob_box(&self, id: u32) -> Option<Aabb> {
        self.ecs.bounds(id)
    }

    /// The alive mob `id` (`EcsLevel::alive`).
    pub(in crate::game) fn mob_alive(&self, id: u32) -> bool {
        self.ecs.alive(id)
    }

    /// Whether mob `id` is in the level at all (alive or not).
    pub(in crate::game) fn mob_exists(&self, id: u32) -> bool {
        self.ecs.exists(id)
    }

    /// A villager without a profession: its trade screen is opaque to the guest (`Villager` and `profession.index()` None).
    pub(in crate::game) fn trader_opaque(&self, id: u32) -> bool {
        self.ecs.trader_opaque(id)
    }

    /// Columns of the mount `id`'s inventory (`mobs::mount_inventory::mount_columns`).
    pub(in crate::game) fn mount_columns_of(&self, id: u32) -> Option<usize> {
        self.ecs.mount_columns(id)
    }

    /// The kind id of the mount `id`, as its menu names it (`minecraft:` ids as the mob has them).
    pub(in crate::game) fn mount_kind_of(&self, id: u32) -> Option<String> {
        self.ecs.kind(id).map(str::to_owned)
    }

    /// The slots of mount `id` as its menu shows them (`mount_contents`).
    pub(in crate::game) fn mount_contents_of(
        &self,
        id: u32,
    ) -> Option<(Option<ItemStack>, Option<Option<ItemStack>>, Vec<Option<ItemStack>>)> {
        self.ecs.mount_contents(id)
    }

    /// Writes the slots of mount `id` back (`mount_store`).
    pub(in crate::game) fn mount_store_of(
        &mut self,
        id: u32,
        saddle: Option<ItemStack>,
        body: Option<Option<ItemStack>>,
        items: Vec<Option<ItemStack>>,
    ) {
        self.ecs.mount_store(id, saddle, body, items);
    }

    /// The mount `id` is steered by its rider's client (`equine::moved_by_client`).
    pub(in crate::game) fn mount_moved_by_client(&self, id: u32) -> bool {
        self.ecs.moved_by_client(id)
    }

    /// The point a steered mount was last seen at (`vehicle_good` of a rider's tick).
    pub(in crate::game) fn mount_steered_at(&self, id: u32) -> Option<Vec3> {
        self.ecs.position(id).filter(|_| self.ecs.moved_by_client(id)).map(|p| Vec3::new(p[0], p[1], p[2]))
    }

    /// The corners of the box of `id` as `look_at_entity` takes them.
    pub(in crate::game) fn ecs_corners(&self, id: i32) -> Option<([f64; 3], [f64; 3])> {
        let b = self.mob_box(u32::try_from(id).ok()?)?;
        let (mn, mx) = (b.min_position(), b.max_position());
        Some(([mn.x, mn.y, mn.z], [mx.x, mx.y, mx.z]))
    }

    /// Attack of the guest `gi` on the followed mob `id`: the melee damage of `attack_mob` is the host's, the blow is an
    /// `EcsLevel::hurt`. None when `id` is no alive mob of the level (the caller goes on with the other targets).
    pub(in crate::game) fn ecs_guest_hit(&mut self, conn: u32, gi: usize, id: i32) -> Option<bool> {
        let mob = u32::try_from(id).ok()?;
        let e = self.ecs.entity_of(mob)?;
        if !self.ecs.alive(mob) || self.ecs.kind(mob) == Some("minecraft:ender_dragon") {
            return None;
        }
        if self.lan.as_ref()?.hub.guests[gi].vehicle == Some(mob) {
            return Some(false);
        }
        let kind = self.ecs.kind(mob)?;
        let sent = self
            .as_guest_at(gi, conn, |g| {
                let marked = g.player.as_mut().is_some_and(|p| std::mem::take(&mut p.health.sync_velocity));
                let ((amount, _, _, _), admitted) = g.melee_damage_with_admission(kind, true, Some(mob));
                g.attack_ticks = 0;
                if admitted {
                    let source = g.player.as_ref().map_or(0, |p| p.entity_id as u32);
                    g.ecs_note_killer(e, source);
                    g.ecs_with(|ecs, mw| ecs.hurt(mw, e, amount, HurtSource::Player(source)));
                }
                g.swing_arm();
                let p = g.player.as_mut()?;
                let sent = std::mem::replace(&mut p.health.sync_velocity, marked);
                sent.then_some(p.velocity)
            })
            .ok()
            .flatten();
        self.send_guest_motion(conn, sent);
        if let Some(host) = self.lan.as_mut()
            && host.hub.guests[gi].participates_in_player_simulation()
        {
            host.mob_players.insert(mob, conn);
        }
        Some(true)
    }

    /// Usage of the followed entity `id` by the guest's hand (`EcsLevel::interact`); None for an id the level does not hold.
    pub(in crate::game) fn ecs_interact(&mut self, id: i32, hand: u8, conn: u32) -> Option<bool> {
        let uid = u32::try_from(id).ok()?;
        let e = self.ecs.entity_of(uid)?;
        // The host's leads, mount and saddle rules come before the bridge's generic interaction.
        let held = self.held().cloned();
        let item = held.as_ref().map(|s| s.item.to_string()).unwrap_or_default();
        // `Mob.checkAndHandleImportantInteractions`: a named name tag, then a spawn egg of the creature's own type, before anything else.
        // `NameTagItem.interactLivingEntity`: a name tag with a name names a living entity whose type is saved (not the leash knot, the
        // lightning bolt, the bobber), makes it persistent and is used up; anything else passes on to the entity's own interaction.
        if item == "minecraft:name_tag" {
            if let Some(name) = held.as_ref().and_then(|s| s.custom_name()).filter(|_| self.ecs.can_serialize(uid)) {
                if self.mob_alive(uid) {
                    self.ecs.set_custom_name(uid, Some(name.clone()));
                    self.ecs.set_persistent(e);
                    self.consume_held();
                    if let (Some(player), Some(player_name)) = (self.lan_acting_uuid, self.lan_acting_name.clone()) {
                        self.native_player_name_entity(player, &player_name, uid, &name);
                    }
                }
                return Some(true);
            }
        }
        if item.ends_with("_spawn_egg") {
            return Some(self.ecs_use_stack(e, conn, hand, held));
        }
        self.ecs_interact_host_first(uid, e, &item, conn, hand, held)
    }

    /// P's interaction with the whole stack in the rules' reach (components, wear), and the player's creative state.
    fn ecs_use_stack(&mut self, e: bevy_ecs::entity::Entity, conn: u32, hand: u8, held: Option<myriade_engine::items::ItemStack>) -> bool {
        let creative = self.creative;
        self.ecs_with(|ecs, mw| match held {
            Some(stack) => ecs.interact_with(mw, e, conn, hand, stack, creative),
            None => ecs.interact(mw, e, conn, hand),
        })
    }

    /// The host's leads, mount and saddle rules and the merchant's screen, then P's modules and rules.
    fn ecs_interact_host_first(&mut self, uid: u32, e: bevy_ecs::entity::Entity, item: &str, conn: u32, hand: u8, held: Option<myriade_engine::items::ItemStack>) -> Option<bool> {
        if self.interact_leash_ecs(uid, item).is_some()
            || self.interact_mount_ecs(uid, item)
            || self.shear_saddle_ecs(uid, item) == Some(true)
        {
            return Some(true);
        }
        // A merchant: the host's screen, stat and refusals (`interact_trader_ecs`) decide. P's `trade_session` interact opens the
        // session but no screen, so a merchant never reaches it from here.
        if self.ecs.is_villager(uid).is_some() {
            return Some(self.interact_trader_ecs(uid, item).unwrap_or(false));
        }
        // `player_shear_entity`: shears on a creature that is ready for them. A mod may cancel (the interaction is used, nothing changes);
        // the hook has `&mut self`, so the creature and the shears are found again before P shears.
        if item == "minecraft:shears"
            && let Some(done) = self.player_shear_ecs(uid, e, hand != 0, held.as_ref())
        {
            return Some(done);
        }
        Some(self.ecs_use_stack(e, conn, hand, held))
    }

    /// `interact_mob` on the ECS level: the creature a guest's packet named, or the one under the crosshair before any block,
    /// then `ecs_interact` (host leads, mount and saddle rules, then P's modules and rules). The acting hand is `self.hand`.
    pub(in crate::game) fn interact_mob_ecs(&mut self, target: Option<super::super::InteractTarget>) -> bool {
        let id = match target {
            Some(t) => match u32::try_from(t.id) {
                Ok(id) if self.creature_species(id).is_some() && self.mob_alive(id) && Some(id) != self.riding => id,
                _ => return false,
            },
            None => match self.ecs_mob_hit() {
                Some((id, d)) if self.before_block(d) && Some(id) != self.riding => id,
                _ => return false,
            },
        };
        let Some(conn) = self.acting_player_uuid_id() else { return false };
        let hand = u8::from(self.hand != mod_api::Hand::Main);
        let used = self.ecs_interact(id as i32, hand, conn) == Some(true);
        if used {
            // `Mob.interact`: a use that consumed the action posts `ENTITY_INTERACT` at the entity, for the player.
            if let Some(at) = self.ecs.position(id) {
                let ctx = self.who_context(Some(Who::Player), None);
                self.post_game_event("minecraft:entity_interact", at, ctx);
            }
            self.swing_arm();
        }
        used
    }

    /// Too-fast check and move of a steered mount the ECS holds (`handleMoveVehicle`): a refused move sends the mount back;
    /// an accepted one is `absSnapTo` (position, yaw, pitch) and the move as the motion. True when accepted. The collision test
    /// ("moved wrongly") and the fall of `vehicle.move` are not bridged yet (listed in the report).
    pub(in crate::game) fn ecs_mount_move(&mut self, conn: u32, v: u32, target: Vec3, yaw: f32, pitch: f32, owner: bool) -> bool {
        let (Some(e), Some(m)) = (self.ecs.entity_of(v), self.ecs.motion(v)) else {
            return false;
        };
        let old = Vec3::new(m.position[0], m.position[1], m.position[2]);
        let velocity = Vec3::new(m.velocity[0], m.velocity[1], m.velocity[2]);
        let moved = target.distance_squared(old) - velocity.length_squared();
        if moved > 100.0 && !owner {
            log::warn!("vehicle {v} moved too quickly! {moved}");
            self.send_vehicle_back(conn, old, m.yaw);
            return false;
        }
        self.ecs_with(|ecs, mw| {
            ecs.teleport(mw, e, [target.x, target.y, target.z]);
            ecs.set_rotation(e, yaw, pitch);
            ecs.set_velocity(e, target.subtract(old));
        });
        true
    }

    /// `MoveVehicle` sent back to the guest's client at `at`.
    pub(in crate::game) fn send_vehicle_back(&mut self, conn: u32, at: Vec3, yaw: f32) {
        let packet = play::MoveVehicle { position: [at.x, at.y, at.z], yaw, pitch: 0.0 };
        if let Some(host) = self.lan.as_mut() {
            host.hub.send_to(conn, &C::MoveVehicle(packet));
        }
    }
}
