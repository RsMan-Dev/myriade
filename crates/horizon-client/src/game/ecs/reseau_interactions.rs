//! Lot 13 (colle) : les frontières réseau d'interaction (attaque, usage, marchand, monture, véhicule) lisent et agissent sur l'ECS
//! par id. Ce qui est dans `Game.ecs` passe par `EcsLevel` ; le reste (`remote_entities`, rôle Client) garde le code d'origine.
//! Imports à ajouter au fichier d'accueil : `horizon_core::geometry::{Aabb, Vec3}`, `horizon_client::items::ItemStack`, `horizon_client::mobs::equine`, `horizon_ecs_proto::bridge::host::HurtSource`, `horizon_protocol::packets::play::{self, Clientbound as C}`.
use super::super::Game;

impl Game<'_> {
    /// The box of followed entity `id` (`EcsLevel::bounds`), or the client's `remote_entities` box.
    pub(in crate::game) fn mob_box(&self, id: u32) -> Option<Aabb> {
        if self.ecs.is_active() {
            return self.ecs.bounds(id);
        }
        self.remote_entities.iter().find(|m| m.id == id).map(|m| m.bounds())
    }

    /// The alive mob `id` (`EcsLevel::alive`), or the client's `remote_entities` mob.
    pub(in crate::game) fn mob_alive(&self, id: u32) -> bool {
        if self.ecs.is_active() {
            return self.ecs.alive(id);
        }
        self.remote_entities.iter().any(|m| m.id == id && m.alive() && !m.removed)
    }

    /// Whether mob `id` is in the level at all (alive or not).
    pub(in crate::game) fn mob_exists(&self, id: u32) -> bool {
        if self.ecs.is_active() {
            return self.ecs.exists(id);
        }
        self.remote_entities.iter().any(|m| m.id == id)
    }

    /// A villager without a profession: its trade screen is opaque to the guest (`Villager` and `profession.index()` None).
    pub(in crate::game) fn trader_opaque(&self, id: u32) -> bool {
        if self.ecs.is_active() {
            return self.ecs.trader_opaque(id);
        }
        self.remote_entities.iter().find(|m| m.id == id).is_some_and(|m| {
            m.species() == horizon_client::mobs::Species::Villager && m.extra.profession.index().is_none()
        })
    }

    /// Columns of the mount `id`'s inventory (`mount_menu::mount_columns`).
    pub(in crate::game) fn mount_columns_of(&self, id: u32) -> Option<usize> {
        if self.ecs.is_active() {
            return self.ecs.mount_columns(id);
        }
        let m = self.remote_entities.iter().find(|m| m.id == id)?;
        Some(crate::game::network::mount_menu::mount_columns(m))
    }

    /// The kind id of the mount `id`, as its menu names it (`minecraft:` ids as the mob has them).
    pub(in crate::game) fn mount_kind_of(&self, id: u32) -> Option<String> {
        if self.ecs.is_active() {
            return self.ecs.kind(id).map(str::to_owned);
        }
        self.remote_entities.iter().find(|m| m.id == id).map(|m| m.kind.id.to_string())
    }

    /// The slots of mount `id` as its menu shows them (`mount_contents`).
    pub(in crate::game) fn mount_contents_of(
        &self,
        id: u32,
    ) -> Option<(Option<ItemStack>, Option<Option<ItemStack>>, Vec<Option<ItemStack>>)> {
        if self.ecs.is_active() {
            return self.ecs.mount_contents(id);
        }
        let index = self.remote_entities.iter().position(|m| m.id == id)?;
        Some(self.mount_contents(index))
    }

    /// Writes the slots of mount `id` back (`mount_store`).
    pub(in crate::game) fn mount_store_of(
        &mut self,
        id: u32,
        saddle: Option<ItemStack>,
        body: Option<Option<ItemStack>>,
        items: Vec<Option<ItemStack>>,
    ) {
        if self.ecs.is_active() {
            self.ecs.mount_store(id, saddle, body, items);
        } else if let Some(index) = self.remote_entities.iter().position(|m| m.id == id) {
            self.mount_store(index, saddle, body, items);
        }
    }

    /// The mount `id` is steered by its rider's client (`equine::moved_by_client`).
    pub(in crate::game) fn mount_moved_by_client(&self, id: u32) -> bool {
        if self.ecs.is_active() {
            return self.ecs.moved_by_client(id);
        }
        self.remote_entities.iter().any(|m| m.id == id && equine::moved_by_client(m))
    }

    /// The point a steered mount was last seen at (`vehicle_good` of a rider's tick).
    pub(in crate::game) fn mount_steered_at(&self, id: u32) -> Option<Vec3> {
        if self.ecs.is_active() {
            return self.ecs.position(id).filter(|_| self.ecs.moved_by_client(id)).map(|p| Vec3::new(p[0], p[1], p[2]));
        }
        self.remote_entities
            .iter()
            .find(|m| m.id == id && equine::moved_by_client(m))
            .map(|m| m.position)
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
        if !self.ecs.alive(mob) || self.dragon.as_ref().is_some_and(|d| d.id as i32 == id) {
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
                    g.ecs.hurt(e, amount, HurtSource::Player(source));
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
        let item = self.held().map(|s| s.item.to_string()).unwrap_or_default();
        if self.interact_leash_ecs(uid, &item).is_some()
            || self.interact_mount_ecs(uid, &item)
            || self.shear_saddle_ecs(uid, &item) == Some(true)
        {
            return Some(true);
        }
        Some(self.ecs.interact(e, conn, hand))
    }

    /// Too-fast check and move of a steered mount the ECS holds (`handleMoveVehicle`): a refused move sends the mount back;
    /// an accepted one is a teleport request. Its collision, fall and the motion write are not bridged yet (listed in the report).
    pub(in crate::game) fn ecs_mount_move(&mut self, conn: u32, v: u32, target: Vec3, yaw: f32, owner: bool) {
        let (Some(e), Some(m)) = (self.ecs.entity_of(v), self.ecs.motion(v)) else {
            return;
        };
        let old = Vec3::new(m.position[0], m.position[1], m.position[2]);
        let velocity = Vec3::new(m.velocity[0], m.velocity[1], m.velocity[2]);
        let moved = target.distance_squared(old) - velocity.length_squared();
        if moved > 100.0 && !owner {
            log::warn!("vehicle {v} moved too quickly! {moved}");
            self.send_vehicle_back(conn, old, m.yaw);
            return;
        }
        self.ecs.teleport(e, [target.x, target.y, target.z]);
        let _ = yaw;
    }

    /// `MoveVehicle` sent back to the guest's client at `at`.
    pub(in crate::game) fn send_vehicle_back(&mut self, conn: u32, at: Vec3, yaw: f32) {
        let packet = play::MoveVehicle { position: [at.x, at.y, at.z], yaw, pitch: 0.0 };
        if let Some(host) = self.lan.as_mut() {
            host.hub.send_to(conn, &C::MoveVehicle(packet));
        }
    }
}

// Tests (écrits, non lancés). Le fixture de jeu est celui des tests `lan_vehicles.rs` (`game.run_command`, `game.ecs`).
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attack_of_an_unknown_id_is_refused_without_panic() {
        let mut game = Game::test_fixture();
        let conn = 1;
        let refused = game.ecs_guest_hit(conn, 0, 987_654);
        assert!(refused.is_none(), "an id the level does not hold is not a hit");
    }

    #[test]
    fn attack_of_a_linked_id_writes_a_damage_event() {
        let mut game = Game::test_fixture();
        let conn = 1;
        assert!(game.run_command("summon minecraft:zombie 0.5 200 0.5 {NoAI:1b}"));
        let mob = game.ecs.exists_ids().first().copied().expect("a linked mob");
        let landed = game.ecs_guest_hit(conn, 0, mob as i32);
        assert_eq!(landed, Some(true));
        assert!(game.ecs.drain_damage_events().iter().any(|d| d.target == mob));
    }
}
