//! The host's writes on a living entity: health, turn, motion, fire, effects and the hand. Each one that changes what the view reads
//! marks the entity (`view::mark`); none adds a component of its own.
use crate::area;
use crate::bridge::{host::EcsWorld, view};
use crate::cow::{Contact, Health, MaxHealth, Resist, Vel, Yaw, set_on_fire};
use crate::effects::{self, Effects, Kind};
use crate::goals::{self, Head};
use crate::level;
use crate::rules::regen;
use bevy_ecs::entity::Entity;
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::Vec3;

/// The bound of a write on health: the maximum the entity has (its component, else its species'), none without a species.
fn cap(w: &bevy_ecs::world::World, e: Entity) -> f32 {
    w.get::<MaxHealth>(e).map(|m| m.0).or_else(|| w.get::<crate::cow::SpeciesId>(e).is_some().then(|| crate::cow::max_health(w, e))).unwrap_or(f32::MAX)
}

impl EcsWorld {
    /// `LivingEntity.heal`: `amount` more health, capped at the max; false when the entity is dead or has no health.
    pub fn heal(&mut self, e: Entity, amount: f32) -> bool {
        let w = self.world_mut();
        let Some(h) = w.get::<Health>(e).map(|h| h.0).filter(|h| *h > 0.0) else { return false };
        let max = cap(w, e);
        if let Some(mut hp) = w.get_mut::<Health>(e) {
            hp.0 = regen::healed(h, amount, max);
        }
        view::mark(w, e);
        true
    }

    /// Writes `Health` bounded to `[0, max]`; it does not kill (`hurt` does). False when the entity has no health.
    pub fn set_health(&mut self, e: Entity, value: f32) -> bool {
        let w = self.world_mut();
        let max = cap(w, e);
        let Some(mut hp) = w.get_mut::<Health>(e) else { return false };
        hp.0 = value.max(0.0).min(max);
        drop(hp);
        view::mark(w, e);
        true
    }

    /// Turns the entity: `Yaw`, and the head when it has one.
    pub fn set_rotation(&mut self, e: Entity, yaw: f32, pitch: f32) {
        let w = self.world_mut();
        if let Some(mut y) = w.get_mut::<Yaw>(e) {
            y.0 = yaw;
        }
        if let Some(mut h) = w.get_mut::<Head>(e) {
            (h.yaw, h.pitch) = (yaw, pitch);
        }
        view::mark(w, e);
    }

    /// `Entity.setYHeadRot`: the head's yaw, the body and pitch untouched. False for an entity without a head.
    pub fn set_head_yaw(&mut self, e: Entity, yaw: f32) -> bool {
        let w = self.world_mut();
        let Some(mut h) = w.get_mut::<Head>(e) else { return false };
        h.yaw = yaw;
        view::mark(w, e);
        true
    }

    /// `Entity.teleportTo` motion: the axes of `Vel` that `keep` does not name are set to zero.
    pub fn teleport_motion(&mut self, e: Entity, keep: [bool; 3]) {
        let w = self.world_mut();
        let Some(v) = w.get::<Vel>(e).map(|v| v.0) else { return };
        let kept = Vec3::new(
            if keep[0] { v.x } else { 0.0 },
            if keep[1] { v.y } else { 0.0 },
            if keep[2] { v.z } else { 0.0 },
        );
        goals::set_velocity(w, e, kept);
        view::mark(w, e);
    }

    /// `Entity.push`: the velocity grows by `v`, and the body wakes.
    pub fn push(&mut self, e: Entity, v: [f64; 3]) {
        let w = self.world_mut();
        area::push_creature(w, e, v);
        view::mark(w, e);
    }

    /// `LivingEntity.knockback`: the strength left after `Resist`, the horizontal speed halved and pushed along `(dx, dz)`, the
    /// lift on the ground capped at 0.4. A null direction gives no horizontal push.
    pub fn knockback(&mut self, e: Entity, strength: f64, dx: f64, dz: f64) {
        let w = self.world_mut();
        let power = strength * (1.0 - w.get::<Resist>(e).map_or(0.0, |r| r.0));
        let on_ground = w.get::<Contact>(e).is_some_and(|c| c.on_ground);
        let Some(v) = w.get::<Vel>(e).map(|v| v.0) else { return };
        if power <= 0.0 {
            return;
        }
        let len = (dx * dx + dz * dz).sqrt();
        let (kx, kz) = if len < 1.0e-4 { (0.0, 0.0) } else { (dx / len * power, dz / len * power) };
        let y = if on_ground { (v.y / 2.0 + power).min(0.4) } else { v.y };
        goals::set_velocity(w, e, Vec3::new(v.x / 2.0 - kx, y, v.z / 2.0 - kz));
        view::mark(w, e);
    }

    /// `LivingEntity.jump`: true when the entity stands on the ground and jumps.
    pub fn jump(&mut self, e: Entity) -> bool {
        let w = self.world_mut();
        if !w.get::<Contact>(e).is_some_and(|c| c.on_ground) {
            return false;
        }
        level::jump(w, e);
        view::mark(w, e);
        true
    }

    /// `Entity.igniteForSeconds`: on fire for `ticks` more.
    pub fn set_on_fire(&mut self, e: Entity, ticks: u64) {
        let w = self.world_mut();
        set_on_fire(w, e, ticks);
        view::mark(w, e);
    }

    /// `LivingEntity.addEffect`: true when the list changed.
    pub fn add_effect(&mut self, e: Entity, kind: Kind, amp: i32, ticks: i32, flags: u8) -> bool {
        let w = self.world_mut();
        let changed = effects::add_flagged(w, e, kind, amp, ticks, flags);
        if changed {
            view::mark(w, e);
        }
        changed
    }

    /// Ends every warden's minute of calm (`dig_cooldown_until` back to 0): the entities that carry an `ActPhase`. How many were reached.
    pub fn expire_dig_cooldowns(&mut self) -> usize {
        let w = self.world_mut();
        let mut n = 0;
        let mut q = w.query::<&mut crate::behaviours::act_phase::ActPhase>();
        for mut a in q.iter_mut(w) {
            a.dig_cooldown_until = 0;
            n += 1;
        }
        n
    }

    /// `/raid glow`: every raider of raid `raid` gets Glowing for `ticks` at amplifier `amp`; how many were reached.
    pub fn glow_raiders(&mut self, raid: i32, ticks: i32, amp: i32) -> usize {
        let w = self.world_mut();
        let raiders: Vec<Entity> = w
            .query::<(Entity, &crate::raids::Raider)>()
            .iter(w)
            .filter(|(_, r)| r.raid == Some(raid))
            .map(|(e, _)| e)
            .collect();
        for &e in &raiders {
            if effects::add_flagged(w, e, Kind::Glowing, amp, ticks, 0) {
                view::mark(w, e);
            }
        }
        raiders.len()
    }

    /// `LivingEntity.removeEffect`: true when the effect was there.
    pub fn remove_effect(&mut self, e: Entity, kind: Kind) -> bool {
        let w = self.world_mut();
        let removed = effects::remove(w, e, kind);
        if removed {
            view::mark(w, e);
        }
        removed
    }

    /// Removes every effect that is not in `keep`; true when something was removed.
    pub fn retain_effects(&mut self, e: Entity, keep: &[Kind]) -> bool {
        let w = self.world_mut();
        let gone: Vec<Kind> = w.get::<Effects>(e).map_or_else(Vec::new, |f| {
            f.list.iter().map(|x| x.kind).filter(|k| !keep.contains(k)).collect()
        });
        let mut changed = false;
        for kind in gone {
            changed |= effects::remove(w, e, kind);
        }
        if changed {
            view::mark(w, e);
        }
        changed
    }

    /// `Contact.fall_distance`, the distance the entity has fallen.
    pub fn set_fall_distance(&mut self, e: Entity, distance: f64) {
        let w = self.world_mut();
        let Some(mut c) = w.get_mut::<Contact>(e) else { return };
        c.fall_distance = distance;
        drop(c);
        view::mark(w, e);
    }

    /// `LivingEntity.setItemSlot(MAINHAND, stack)`, through `goals::set_main_hand`.
    pub fn set_main_hand(&mut self, e: Entity, stack: Option<ItemStack>) {
        let w = self.world_mut();
        goals::set_main_hand(w, e, stack);
        view::mark(w, e);
    }
}
