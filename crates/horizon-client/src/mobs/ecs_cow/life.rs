//! A cow's health: `LivingEntity.hurtServer` (`Mob::damage_with`), knockback, burning, death, the
//! voice. Ported from `mobs.rs` for a cow: no armor, effects, absorption, shield or mod listener.
use super::view::CowItem;
use crate::{
    health::{DamageContainer, DamageLevel, Reduction},
    mobs::{Damage, DamageKind, Who, combat},
};
use horizon_core::{geometry::Vec3, random::RandomSource};

impl CowItem<'_, '_> {
    pub fn ambient_interval(&self) -> i32 {
        self.id.kind.ambient_interval
    }
    /// `LivingEntity.getVoicePitch`.
    pub fn voice_pitch(&mut self) -> f32 {
        let pitch = (self.random.next_float() - self.random.next_float()) * 0.2
            + if self.is_baby() { 1.5 } else { 1.0 };
        pitch
    }
    /// `Mob.playAmbientSound` / a hurt voice: the pitch is drawn, nothing listens to the sound.
    pub fn voice(&mut self) {
        let _ = self.voice_pitch();
    }
    pub fn is_on_fire(&self) -> bool {
        !self.id.kind.flags.fire_immune && self.env.fire_ticks > 0
    }
    /// A cow has no `burning_time` modifier: the attribute's base, 1.
    pub fn burning_time(&self) -> f64 {
        1.0
    }
    pub fn ignite_for_ticks(&mut self, ticks: i32) {
        self.env.fire_ticks = self.env.fire_ticks.max((f64::from(ticks) * self.burning_time()).ceil() as i32);
        self.env.ticks_frozen = 0;
    }
    /// `Entity.extinguishFire`: a burning mob draws the pitch of the hiss first.
    pub fn extinguish_fire(&mut self) {
        if self.is_on_fire() {
            let _pitch = 1.6 + (self.random.next_float() - self.random.next_float()) * 0.4;
        }
        self.env.fire_ticks = self.env.fire_ticks.min(0);
    }
    pub fn knockback_resistance(&self) -> f64 {
        self.vitals.gear_knockback_resistance.unwrap_or(self.attrs.knockback_resistance)
    }
    pub fn record_damage(&mut self, source: Damage, damage: f32) {
        let entry = combat::Entry {
            source,
            damage,
            in_water: self.fluids.in_water,
            fall_distance: self.motion.fall_distance as f32,
        };
        let age = self.clk.age;
        self.vitals.combat.record(entry, age);
    }
    /// `LivingEntity.knockback` (no listener).
    pub fn knockback(&mut self, power: f64, xd: f64, zd: f64) {
        let power = power * (1.0 - self.knockback_resistance());
        if power <= 0.0 {
            return;
        }
        self.motion.needs_sync = true;
        let (mut xd, mut zd) = (xd, zd);
        while xd * xd + zd * zd < f64::from(1.0e-5_f32) {
            xd = (self.random.next_double() - self.random.next_double()) * 0.01;
            zd = (self.random.next_double() - self.random.next_double()) * 0.01;
        }
        let len = (xd * xd + zd * zd).sqrt();
        let (kx, kz) = (xd / len * power, zd / len * power);
        let v = self.motion.velocity;
        self.motion.velocity = Vec3::new(
            v.x / 2.0 - kx,
            if self.motion.on_ground { (v.y / 2.0 + power).min(0.4) } else { v.y },
            v.z / 2.0 - kz,
        );
    }
    /// `Entity.isInvulnerableToBase`'s term and the species' own: `LivingEntity.isInvulnerableTo`.
    fn is_invulnerable_to(&self, source: &Damage) -> bool {
        self.is_invulnerable() && !self.vitals.creative_attacker && !source.kind.bypasses_invulnerability()
    }
    pub fn damage(&mut self, level: &mut DamageLevel<'_>, amount: f32, source: Damage) -> bool {
        let mut passed = false;
        self.hurt_server(level, amount, source, &mut passed)
    }
    fn hurt_server(&mut self, level: &mut DamageLevel<'_>, amount: f32, source: Damage, passed: &mut bool) -> bool {
        let _ = level;
        let invulnerable = self.vitals.removed
            || self.is_invulnerable_to(&source)
            || (source.is_fire() && self.id.kind.flags.fire_immune);
        if invulnerable {
            return false;
        }
        *passed = true;
        if !self.alive() {
            return false;
        }
        let mut container = DamageContainer::new(amount);
        let mut amount = container.new_damage();
        if amount < 0.0 {
            amount = 0.0;
        }
        let blocked = 0.0_f32;
        amount -= blocked;
        container.record_block(blocked);
        if !amount.is_finite() {
            amount = f32::MAX;
        }
        let incoming = amount;
        let cooldown = self.vitals.invulnerable as f32 > 10.0;
        if cooldown {
            if incoming <= self.vitals.last_hurt {
                return false;
            }
            amount = incoming - self.vitals.last_hurt;
        }
        let entered = amount;
        let armor_ran = !source.bypasses_armor();
        amount = container.reduce(Reduction::Armor, entered, entered - amount, amount, armor_ran);
        let after_armor = amount;
        amount = container.reduce(Reduction::MobEffects, after_armor, after_armor - amount, amount, false);
        let after_effects = amount;
        amount = container.reduce(Reduction::Enchantments, after_effects, after_effects - amount, amount, false);
        let after_enchantments = amount;
        amount = container.reduce(
            Reduction::InnateResistance,
            after_enchantments,
            after_enchantments - amount,
            amount,
            false,
        );
        let mut knock = true;
        let window = cooldown;
        container.record_should_cause_side_effects(!window);
        if window {
            container.record_reduction(Reduction::Invulnerability, self.vitals.last_hurt);
        }
        let dealt = amount;
        if window {
            container.set_post_attack_invulnerability_ticks(self.vitals.invulnerable);
        }
        container.enter_pre(dealt);
        let dealt = container.capture_inflicted();
        // `soak_absorption` of a cow with no absorption: all of it goes through.
        let through = (dealt - self.vitals.absorption).max(0.0);
        let left = self.vitals.absorption - (dealt - through);
        self.vitals.absorption = if left < 0.0 { 0.0 } else { left.min(0.0) };
        container.record_reduction(Reduction::Absorption, dealt - through);
        self.vitals.health -= through;
        self.vitals.last_hurt = incoming + container.pre_change();
        self.vitals.invulnerable = container.post_attack_invulnerability_ticks();
        if window {
            knock = false;
        } else {
            self.vitals.hurt_time = 10;
        }
        if dealt != 0.0 {
            self.record_damage(source, dealt);
        }
        container.record_health_damage(through);
        // `Animal.actuallyHurt`: damage ends love mode.
        self.extra.in_love = 0;
        let success = blocked <= 0.0 || amount > 0.0;
        if success {
            let age = i64::from(self.clk.age);
            self.vitals.last_damage = Some((source, age));
        }
        self.vitals.last_direct = None;
        // `hurt_by::resolve`
        if let Some(who) = source.attacker {
            if !matches!(source.kind, DamageKind::MobAttackNoAggro) {
                let age = self.clk.age as i32;
                self.ai.hurt_by(who, age);
            }
            if who == Who::Player {
                self.ai.last_hurt_by_player = 100;
            }
        }
        if knock && !source.no_knockback() && !(blocked > 0.0 && amount <= 0.0) {
            let (dx, dz) = match self.vitals.projectile_motion {
                Some([x, z]) => (-x, -z),
                None => source
                    .from
                    .map_or((0.0, 0.0), |f| (f[0] - self.motion.position.x, f[2] - self.motion.position.z)),
            };
            self.knockback(f64::from(0.4f32), dx, dz);
        }
        // `ai::after_hurt`
        self.ai.no_action_time = 0;
        if self.vitals.health <= 0.0 {
            self.vitals.health = 0.0;
        }
        if success {
            self.vitals.death_canceled = false;
            self.vitals.accepted_hit = None;
        }
        if knock {
            let dead = !self.alive();
            if !dead {
                self.clk.ambient_timer = -self.ambient_interval();
            }
            let _pitch = self.voice_pitch();
        }
        success
    }
}
