//! The game side of piglins (`PiglinAi`): the player, the LAN guests, the scoreboard
//! and the mods' hooks (anger, safe armour, currency).
use super::Game;

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
            || !self.ecs.exists(mob)
            || self.ecs.ai_target(mob).is_some()
        {
            return;
        }
        self.set_event_ai_target(mob, Some(target), player);
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
    /// `PiglinAi.angerNearbyPiglins`: the ECS's guards take the alert
    /// (`alert_guards`), with their own sight and idle checks.
    pub(in crate::game) fn anger_piglins(&mut self, only_if_seen: bool) {
        // Fake players never become a mob's target.
        if self.lan_acting_fake || self.creative || self.spectator() || self.player.is_none() {
            return;
        }
        if let Some(id) = self.acting_player_uuid_id() {
            self.ecs.alert_guards(id, only_if_seen);
        }
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
