//! A player's melee attack (`Player.attack`): the attack strength, critical and sweep attacks, and
//! what a hit on a target that is not a mob does.
use super::*;

#[cfg(test)]
#[path = "tests/spawn_protection_tests.rs"]
mod spawn_protection_tests;

impl Game<'_> {
    /// `Player.getAttackStrengthScale(0.5)`.
    pub(super) fn attack_strength_scale(&self) -> f32 {
        self.attack_strength_scale_at(0.5)
            .expect("finite partial tick")
    }

    /// `Player.getAttackStrengthScale(partial)` for the player in place.
    /// Returns `None` for a non-finite partial tick; finite partials are not restricted.
    pub(super) fn attack_strength_scale_at(&self, partial: f32) -> Option<f32> {
        if !partial.is_finite() {
            return None;
        }
        // 1.8.9 had no attack cooldown.
        if self.legacy_rules() {
            return Some(1.0);
        }
        let delay = myriade_engine::attack_strength::delay(self.main_hand_attack_speed());
        Some(((self.attack_ticks as f32 + partial) / delay).clamp(0.0, 1.0))
    }

    /// `Player.isSweepAttack`: a charged, grounded sword hit below the movement
    /// limit, which `native_sweep_attack` may force or stop for the hit on `target`.
    pub(in crate::game) fn sweep_attack_position(
        &mut self,
        scale: f32,
        crit: bool,
        target: &str,
    ) -> Option<[f64; 3]> {
        // The sweep came with 1.9.
        if self.legacy_rules() {
            return None;
        }
        // Player.getSpeed: the movement speed attribute as it is now (the
        // field is only refreshed by the local movement step, never for a
        // server's player).
        let p = self.player.as_mut()?;
        if let Ok(speed) = p.attribute("minecraft:movement_speed") {
            p.speed = speed as f32;
        }
        if !self.sweep_decision(scale, crit, target) {
            return None;
        }
        let p = self.player.as_ref()?;
        // Mth.sin / Mth.cos of yRot * (PI / 180), from its sine table.
        use horizon_core::geometry::trig;
        let yaw = f64::from(self.yaw * trig::DEG_TO_RAD);
        Some([
            p.position.x - f64::from(trig::sin(yaw)),
            p.position.y + f64::from(p.dimensions.height) * 0.5,
            p.position.z + f64::from(trig::cos(yaw)),
        ])
    }

    /// `Player.doSweepAttack` after a successful hit, in vanilla's order: the
    /// sweep sound, the victims around `target` (its box, and the mob that
    /// was hit, when there is one) and then the arc.
    pub(in crate::game) fn sweep_attack(
        &mut self,
        at: [f64; 3],
        from: Option<[f64; 3]>,
        target: Option<(Aabb, mace::Spared)>,
        scale: f32,
    ) {
        self.audio
            .play("minecraft:entity.player.attack.sweep", from, 1.0, 1.0);
        if let Some((area, mob)) = target {
            self.sweep_damage(area, mob, scale);
        }
        let arc = myriade_engine::particles::ParticleOptions::simple(
            myriade_engine::particles::ParticleType::SweepAttack,
        );
        // Player.sweepAttack's `sendParticles(SWEEP_ATTACK, ..., 0, dx, 0, dz, 0)`:
        // the offset is the one from the player to the particle.
        let origin = from.unwrap_or(at);
        let spread = [at[0] - origin[0], 0.0, at[2] - origin[2]];
        self.lan_send_particles(lan_particles::QueuedParticle {
            dimension: self.dim.id(),
            options: arc,
            state: None,
            at,
            spread,
            speed: [0.0; 3],
            count: 0,
            randomization: 0,
        });
    }

    /// `Player.attack` after a target that is not a mob took the hit:
    /// the sprint knockback sound, a living target's critical hit (`living`:
    /// its entity id; `crit`: whether `melee_damage` decided the hit critical,
    /// mods' `critical_hit` included), the `sweep` decided before the hit
    /// (`sweep_before_hit`) around the target's box `area` and the strong or weak sound.
    pub(super) fn attack_effects(
        &mut self,
        area: Aabb,
        sweep: Option<[f64; 3]>,
        living: Option<i32>,
        crit: bool,
        spared: mace::Spared,
        magic: Option<i32>,
    ) {
        let scale = self.attack_strength_scale();
        let full = scale > 0.9;
        let player = self.player.as_ref();
        let from = player.map(|p| [p.position.x, p.position.y, p.position.z]);
        if full && player.is_some_and(|p| p.sprinting) {
            self.audio
                .play("minecraft:entity.player.attack.knockback", from, 1.0, 1.0);
        }
        let crit = living.is_some() && crit;
        if let Some(at) = sweep {
            self.sweep_attack(at, from, Some((area, spared)), scale);
        }
        if crit {
            self.audio
                .play("minecraft:entity.player.attack.crit", from, 1.0, 1.0);
            if let Some(id) = living {
                self.area_critical_hit(id, area, false);
            }
        } else if sweep.is_none() {
            let sound = if full {
                "minecraft:entity.player.attack.strong"
            } else {
                "minecraft:entity.player.attack.weak"
            };
            self.audio.play(sound, from, 1.0, 1.0);
        }
        // Player.magicCrit: the enchantments added damage.
        if let Some(id) = magic {
            self.area_critical_hit(id, area, true);
        }
    }

    /// `critical_hit` around the box `area` of entity `id` (`magic`: the
    /// enchanted hit instead of the crit).
    pub(in crate::game) fn area_critical_hit(&mut self, id: i32, area: Aabb, magic: bool) {
        let (feet, size) = (area.bottom_center(), area.size());
        self.critical_hit(
            self.lan_acting,
            id,
            [feet.x, feet.y, feet.z],
            size.x,
            size.y,
            magic,
        );
    }

    /// `Player.canCriticalAttack` for a living target at attack strength
    /// `scale`: falling, off the ground, not in water, not on a ladder, not blind (mobility
    /// restricted), not riding, not sprinting.
    pub(super) fn critical_attack(&self, scale: f32) -> bool {
        let player = self.player.as_ref();
        let falling = player.is_some_and(|p| {
            !p.ground.on_ground
                && p.fall_distance > 0.0
                && !p.fluids.was_touching_water
                && !p.on_climbable
                && !p.sprinting
        });
        let restricted = self.potions.effects.get("blindness").is_some() || self.riding.is_some();
        scale > 0.9 && falling && !restricted
    }

    /// Player.attack with the attack-strength charge, crits and knockback.
    pub(super) fn attack(&mut self) -> bool {
        if self.spectator() {
            return false;
        }
        // A painting nearer than the targeted block and mob breaks instead.
        // The creatures are found by `ecs_mob_hit` (their real boxes), so they keep their place before the paintings.
        let mob_distance = self.ecs_mob_hit().map(|m| m.1);
        if self.aimed_painting().is_some_and(|(_, d)| mob_distance.is_none_or(|md| d < md) && self.before_block(d)) && self.attack_painting() {
            return true;
        }
        // A ghast's fireball or a wind charge in front: turned back.
        if self.attack_redirectable() {
            return true;
        }
        if self.attack_frame()
            || self.attack_armor_stand()
            || self.attack_cushion()
            || self.attack_boat()
            || self.attack_cart()
            || self.attack_dragon()
        {
            return true;
        }
        let Some((id, distance)) = self.ecs_mob_hit() else {
            return false;
        };
        if !self.before_block(distance) || !self.held_range_allows(distance) {
            return false;
        }
        self.attack_mob(id);
        // Minecraft.startAttack swings after the attack.
        self.swing_arm();
        true
    }
}

impl Game<'_> {}

impl Game<'_> {
    /// `Player.attack` on the mob `id`: the ECS hit (`attack_entity`).
    pub(super) fn attack_mob(&mut self, id: u32) {
        self.attack_entity(id);
    }
    /// `Player.attack`'s damage against `target_kind` (with the attack
    /// strength and whether it is a critical hit): the weapon and effects
    /// scaled by the attack strength, a mace's smash, the critical hit, then
    /// enchantments.
    pub(super) fn melee_damage(
        &mut self,
        target_kind: &str,
        living: bool,
        victim: Option<u32>,
    ) -> (f32, f32, bool, bool) {
        self.melee_damage_with_admission(target_kind, living, victim)
            .0
    }
    /// Player.attack admits its hurt/tail before the item's damage bonus. Keep
    /// that predicate with the one damage evaluation, including its RNG draws.
    pub(in crate::game) fn melee_damage_with_admission(
        &mut self,
        target_kind: &str,
        living: bool,
        victim: Option<u32>,
    ) -> ((f32, f32, bool, bool), bool) {
        if self.legacy_rules() {
            let hit = self.legacy_melee_damage(target_kind, living);
            return (hit, hit.0 > 0.0);
        }
        let hand_damage = self.attack_damage();
        // Riptide's spin replaces the attack damage attribute.
        let spin = self.player.as_ref().map_or(0.0, |p| p.auto_spin_attack_dmg);
        let damage = if spin > 0.0 { spin } else { hand_damage };
        let scale = self.attack_strength_scale();
        // getEnchantedDamage(target, damage, source) - damage, taken before the
        // attack strength scales the base damage.
        let enchant_bonus = self.enchanted_damage(target_kind, damage, victim) - damage;
        let mut amount = damage * (0.2 + scale * scale * 0.8);
        // Player.attack decides the critical hit only for a hit with base or
        // enchantment damage: checked before the mace's smash bonus is added.
        let damaging = amount > 0.0 || enchant_bonus * scale > 0.0;
        // Item.getAttackDamageBonus (a mace's smash), before the critical hit.
        amount += self.smash_damage_bonus(target_kind);
        let vanilla_crit = living && self.critical_attack(scale);
        let mut crit = vanilla_crit;
        let mut multiplier = if vanilla_crit { 1.5 } else { 1.0 };
        if living && damaging {
            self.native_critical_hit(target_kind, vanilla_crit, &mut crit, &mut multiplier);
        }
        if crit {
            amount *= multiplier;
        }
        // Enchantment damage (sharpness, smite, bane) scaled by the attack strength.
        amount += enchant_bonus * scale;
        ((amount, scale, crit, enchant_bonus * scale > 0.0), damaging)
    }
}
