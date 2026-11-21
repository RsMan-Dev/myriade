//! The game side of piglins (`PiglinAi`): the player, the LAN guests, the scoreboard
//! and the mods' hooks (anger, safe armour, currency).
use super::Game;
use horizon_client::{
    items::ItemStack,
    mobs::{
        Mob, Species,
        piglin::{BARTERING_ITEM, is_idle},
    },
};
use horizon_core::geometry::Vec3;

impl Game<'_> {
    /// `ZombifiedPiglin.alertOthers`: keep the source's player binding and
    /// refuse recipients that already have a target or are allied to it.
    pub(in crate::game) fn piglin_alert_target(
        &mut self,
        alert: horizon_client::mobs::undead::PiglinAlert,
    ) {
        use horizon_client::mobs::Who;
        let horizon_client::mobs::undead::PiglinAlert {
            mob,
            source,
            target,
        } = alert;
        let player = self
            .lan
            .as_ref()
            .and_then(|host| host.mob_players.get(&source).copied())
            .or(self.lan_acting);
        let target_team = match target {
            Who::Mob(id) => self.mob_team(id),
            Who::Player => match player {
                Some(conn) => self
                    .command_guest(conn)
                    .and_then(|guest| self.score.board.players_team(&guest.name)),
                None => self.player_team(),
            },
        };
        if target == Who::Mob(mob)
            || horizon_client::scoreboard::teams_allied(self.mob_team(mob), target_team)
            || self
                .remote_entities
                .iter()
                .find(|other| other.id == mob)
                .is_none_or(|other| other.ai.target.is_some())
        {
            return;
        }
        self.set_event_ai_target(mob, Some(target), player);
    }
    /// `PiglinAi.isBarterCurrency`: vanilla's answer (`BARTERING_ITEM`), then the
    /// parts of the stack's item (`ItemPart::is_piglin_currency`). An item with no
    /// part for it costs one table lookup.
    pub(in crate::game) fn piglin_currency(&self, stack: &ItemStack) -> bool {
        let mut currency = &*stack.item == BARTERING_ITEM;
        if mod_api::__private::item_listens(mod_api::HookId::IsPiglinCurrency, &stack.item) {
            self.native_is_piglin_currency(stack, &mut currency);
        }
        currency
    }
    /// `PiglinAi.isWearingSafeArmor` for the acting player: a stack in an armour
    /// slot (feet, legs, chest, head, `EquipmentSlotGroup.ARMOR`'s order) in
    /// `#minecraft:piglin_safe_armor`, once the parts of its item answered
    /// (`ItemPart::makes_piglins_neutral`). An item with no part for it costs one
    /// table lookup.
    pub(super) fn wearing_gold(&self) -> bool {
        let tag = self
            .res
            .items()
            .item_tags
            .get("minecraft:piglin_safe_armor");
        self.armor.iter().rev().flatten().any(|s| {
            let mut neutral = tag.is_some_and(|t| t.contains(&*s.item));
            if mod_api::__private::item_listens(mod_api::HookId::MakesPiglinsNeutral, &s.item)
                && let Some(epoch) = self.native_lifecycle.epoch()
                && let Some(p) = self.player.as_ref()
            {
                let wearer =
                    mod_api::LivingRef::new(mod_api::EntityRef::new(epoch, p.entity_id as u32));
                self.native_makes_piglins_neutral(s, wearer, &mut neutral);
            }
            neutral
        })
    }
    /// `PiglinAi.setAngerTargetToNearestTargetablePlayerIfFound`: the nearest
    /// visible targetable player when UNIVERSAL_ANGER is on.
    fn nearest_targetable_player_for_piglin(&self, m: &Mob) -> Option<Option<[i32; 4]>> {
        let range = m.attrs.follow_range;
        let mut best: Option<(Option<[i32; 4]>, f64)> = None;
        if let Some(p) = self.player.as_ref()
            && !self.creative
            && !self.spectator()
            && !p.health.dead_or_dying()
        {
            let eye = [p.position.x, p.position.y + 1.62, p.position.z];
            let d2 = m.distance_sqr(p.position);
            if d2 <= range * range && self.can_see(m.center(), eye) {
                let uuid = self
                    .lan_acting_uuid
                    .map(horizon_client::lan::playerdata::uuid_ints)
                    .or(self.level.player_uuid);
                best = Some((uuid, d2));
            }
        }
        if let Some(h) = self.lan.as_ref() {
            for g in &h.hub.guests {
                if h.hub.here(g) && g.loaded && !g.dead && !g.spectator() && g.game_mode != 1 {
                    let eye = [g.position[0], g.position[1] + 1.62, g.position[2]];
                    let pos = Vec3::new(g.position[0], g.position[1], g.position[2]);
                    let d2 = m.distance_sqr(pos);
                    if d2 <= range * range
                        && self.can_see(m.center(), eye)
                        && best.as_ref().is_none_or(|(_, bd)| d2 < *bd)
                    {
                        let uuid = horizon_client::lan::playerdata::uuid_ints(g.uuid);
                        best = Some((Some(uuid), d2));
                    }
                }
            }
        }
        best.map(|(uuid, _)| uuid)
    }
    /// `PiglinAi.angerNearbyPiglins`.
    pub(in crate::game) fn anger_piglins(&mut self, only_if_seen: bool) {
        // Fake players never become a mob's target.
        if self.lan_acting_fake {
            return;
        }
        let Some(p) = self.player.as_ref() else {
            return;
        };
        if self.creative || self.spectator() {
            return;
        }
        let at = p.position;
        if let Some(id) = self.acting_player_uuid_id() {
            self.ecs.alert_guards(id, only_if_seen);
        }
        let eye = [at.x, at.y + 1.62, at.z];
        let player_uuid = self
            .lan_acting_uuid
            .map(horizon_client::lan::playerdata::uuid_ints)
            .or(self.level.player_uuid);
        let universal = self
            .session
            .rules
            .bool_at(horizon_client::gamerules::vanilla::UNIVERSAL_ANGER);
        for i in 0..self.remote_entities.len() {
            // A hook below may have shortened the list.
            if i >= self.remote_entities.len() {
                break;
            }
            let m = &self.remote_entities[i];
            if m.species() != Species::Piglin
                || m.is_baby()
                || !m.alive()
                || !is_idle(m)
                || (m.position.x - at.x).abs() > 16.0
                || (m.position.y - at.y).abs() > 16.0
                || (m.position.z - at.z).abs() > 16.0
            {
                continue;
            }
            if only_if_seen && !self.can_see(m.center(), eye) {
                continue;
            }
            let target_uuid = if universal {
                self.nearest_targetable_player_for_piglin(m)
                    .unwrap_or(player_uuid)
            } else {
                player_uuid
            };
            let id = self.remote_entities[i].id;
            let change = if mod_api::__private::listened(mod_api::HookId::LivingChangeTarget) {
                let original = self
                    .lan_acting_entity
                    .or_else(|| self.player_identity().map(|(_, id)| id))
                    .and_then(|id| self.entity(id))
                    .and_then(|entity| self.living(entity));
                let Some(original) = original else { continue };
                let Some(epoch) = self.native_lifecycle.epoch() else {
                    continue;
                };
                let mob = mod_api::MobRef::new(mod_api::EntityRef::new(epoch, id));
                self.native_mob_target_event(mob, Some(original), true)
                    .and_then(|target| self.native_mob_bind_target(id, target))
                    .map_or(
                        horizon_client::mobs::TargetChange::Keep,
                        horizon_client::mobs::TargetChange::Set,
                    )
            } else {
                horizon_client::mobs::TargetChange::Set(Some(horizon_client::mobs::Who::Player))
            };
            // The hook had `&mut self`: find the piglin again by id.
            let Some(i) = self.remote_entities.iter().position(|m| m.id == id && !m.removed) else {
                continue;
            };
            let m = &mut self.remote_entities[i];
            let previous = m.ai.target;
            horizon_client::mobs::piglin::set_guarded_anger_target(m, target_uuid);
            m.ai.target = match change {
                horizon_client::mobs::TargetChange::Keep => previous,
                horizon_client::mobs::TargetChange::Set(target) => target,
            };
        }
    }
    /// A clear line of sight between two points (blocks with collision stop it).
    fn can_see(&self, from: [f64; 3], to: [f64; 3]) -> bool {
        let steps = 32;
        (1..steps).all(|i| {
            let f = f64::from(i) / f64::from(steps);
            let p = [
                from[0] + (to[0] - from[0]) * f,
                from[1] + (to[1] - from[1]) * f,
                from[2] + (to[2] - from[2]) * f,
            ];
            !self
                .res
                .facts()
                .get(self.world.block(
                    p[0].floor() as i32,
                    p[1].floor() as i32,
                    p[2].floor() as i32,
                ))
                .full_collision
        })
    }
    /// Whether breaking or opening this block angers piglins.
    pub(super) fn guarded_by_piglins(&self, name: &str) -> bool {
        self.res
            .items()
            .block_tags
            .get("minecraft:guarded_by_piglins")
            .is_some_and(|t| t.contains(name))
    }
}
