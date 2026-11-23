//! Accepted melee hits on special targets and their shared attacker tail.
use super::*;
use myriade_engine::mobs::DamageKind;

impl Game<'_> {
    /// Player.attack's cost after an accepted special-entity hurt. Hanging
    /// decorations consume their hit in skipAttackInteraction, before this tail.
    pub(super) fn special_attack_exhaustion(&mut self) {
        if let Some(player) = self.player.as_mut() {
            player.cause_food_exhaustion(0.1);
        }
    }
    /// The attacker's side of Player.causeExtraKnockback, including when the
    /// accepted target was removed by its hurt implementation.
    pub(super) fn special_attack_slowdown(&mut self, knockback: f32) {
        if knockback > 0.0 {
            if let Some(player) = self.player.as_mut() {
                player.velocity.x *= myriade_engine::attack_strength::ATTACKER_SLOWDOWN;
                player.velocity.z *= myriade_engine::attack_strength::ATTACKER_SLOWDOWN;
            }
            self.stop_sprint_after_extra_knockback();
        }
    }

    pub(super) fn attack_dragon(&mut self) -> bool {
        let Some((id, d)) = self.dragon_hit() else {
            return false;
        };
        if !self.before_block(d) || self.mob_hit().is_some_and(|m| m.1 < d) {
            return false;
        }
        // A dragon part is no LivingEntity: `canCriticalAttack` is false.
        // Its dragon: the one whose eight parts take the ids after it.
        let dragon = self.dragon_views().into_iter().map(|(d, _)| d).find(|d| (d + 1..=d + 8).contains(&id));
        let (amount, scale, _, _) = self.melee_damage("minecraft:ender_dragon", false, dragon);
        self.swing_arm();
        // The part's box.
        let area = self
            .special_targets()
            .into_iter()
            .find(|(part, _)| *part == id)
            .map(|(_, b)| b);
        // The sweep and the sounds read the charge, which the hit then spends.
        let sweep = self.sweep_before_hit("minecraft:ender_dragon", false);
        let hurt = self.melee_special(id, amount, DamageKind::PlayerAttack);
        if let (Some(area), true) = (area, hurt) {
            let (knockback, _) = self.extra_knockback("minecraft:ender_dragon", scale);
            self.attack_effects(area, sweep, None, false, super::mace::Spared::Nothing, None);
            self.post_attack_dragon_burst();
            self.special_attack_slowdown(knockback);
            self.special_attack_exhaustion();
        }
        self.attack_ticks = 0;
        true
    }
    /// `VehicleEntity.hurtServer` from the player's attack.
    pub(super) fn attack_cart(&mut self) -> bool {
        let Some((cart, d)) = self.cart_hit() else {
            return false;
        };
        if !self.before_block(d) || self.mob_hit().is_some_and(|m| m.1 < d) {
            return false;
        }
        let burning = self
            .held()
            .is_some_and(|s| s.enchantment_level("minecraft:fire_aspect") > 0);
        let (Some(area), Some(kind)) = (self.ecs.bounds(cart), self.ecs.cart_kind(cart)) else {
            return false;
        };
        // Player.attack's full damage against the cart's entity type.
        let id = cart as i32;
        let (damage, scale, _, magic) = self.melee_damage(kind, false, Some(id as u32));
        let sweep = self.sweep_before_hit(kind, false);
        let attacker = self.own_entity_id();
        let source = myriade_engine::health::HurtSource {
            damage_type: "minecraft:player_attack",
            causing: attacker,
            direct: attacker,
            position: None,
        };
        let vanilla =
            self.saved_refuses_damage(id as u32, "minecraft:player_attack", self.creative);
        if !crate::game::entity::damage_level_of!(self, scoped).invulnerability_check(
            myriade_engine::health::Hurting::Entity(id, kind, source),
            vanilla,
        ) && self.cart_player_hurt(cart, attacker, damage, burning)
            && damage > 0.0
        {
            let (knockback, _) = self.extra_knockback(kind, scale);
            self.special_attack_slowdown(knockback);
            self.special_attack_exhaustion();
        }
        self.vehicle_attack_sounds(area, sweep, magic.then_some(id));
        // Player.onAttack: the hit starts the attack strength again.
        self.attack_ticks = 0;
        self.swing_arm();
        true
    }
}
