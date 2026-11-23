//! Writes on a living entity, by network id or by entity: health, turn, motion, fire, effects and the hand. Each one forwards to the
//! world's bridge, which marks the entity for the view. An inert level answers false, or does nothing.
use super::level::EcsLevel;
use crate::effects::EffectInstance;
use crate::items::ItemStack;
use crate::mobs::MobWorld;
use bevy_ecs::entity::Entity;
use myriade_data::potions;
use horizon_ecs_proto::{
    bridge::host_read_living::effect_kind,
    effects::{AMBIENT, HIDDEN, Kind, NO_ICON},
};

/// The kind of an effect named as the game names it (`poison` or `minecraft:poison`).
fn kind_named(name: &str) -> Option<Kind> {
    if name.contains(':') { effect_kind(name) } else { effect_kind(&format!("minecraft:{name}")) }
}

/// The kind of the effect at registry index `index`.
fn kind_at(index: i32) -> Option<Kind> {
    usize::try_from(index).ok().and_then(potions::effect_by_index).and_then(|def| kind_named(def.id))
}

impl EcsLevel {
    pub fn heal(&mut self, _mw: &MobWorld<'_>, e: Entity, amount: f32) {
        if let Some(w) = self.ecs_mut() {
            w.heal(e, amount);
        }
    }
    pub fn heal_id(&mut self, id: u32, amount: f32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.heal(e, amount))
    }
    pub fn set_health(&mut self, id: u32, value: f32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_health(e, value))
    }
    pub fn set_head_yaw(&mut self, id: u32, yaw: f32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_head_yaw(e, yaw))
    }
    pub fn set_rotation(&mut self, e: Entity, yaw: f32, pitch: f32) {
        if let Some(w) = self.ecs_mut() {
            w.set_rotation(e, yaw, pitch);
        }
    }
    /// `relative` names the axes that keep their motion through a teleport.
    pub fn teleport_motion(&mut self, e: Entity, relative: [bool; 3]) {
        if let Some(w) = self.ecs_mut() {
            w.teleport_motion(e, relative);
        }
    }
    pub fn push(&mut self, id: u32, v: [f64; 3]) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(w) = self.ecs_mut() {
            w.push(e, v);
        }
    }
    pub fn knockback(&mut self, id: u32, strength: f64, dx: f64, dz: f64) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(w) = self.ecs_mut() {
            w.knockback(e, strength, dx, dz);
        }
    }
    pub fn jump(&mut self, id: u32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.jump(e))
    }
    pub fn set_on_fire(&mut self, id: u32, ticks: u64) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(w) = self.ecs_mut() {
            w.set_on_fire(e, ticks);
        }
    }
    pub fn set_fall_distance(&mut self, e: Entity, distance: f64) {
        if let Some(w) = self.ecs_mut() {
            w.set_fall_distance(e, distance);
        }
    }
    pub fn set_main_hand(&mut self, e: Entity, stack: Option<ItemStack>) {
        if let Some(w) = self.ecs_mut() {
            w.set_main_hand(e, stack);
        }
    }
    /// The effect of `instance` (its name, amplifier and duration) on the entity `id`; true when the list changed.
    pub fn add_effect(&mut self, id: u32, instance: EffectInstance) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(kind) = kind_named(instance.id) else { return false };
        let flags = (if instance.ambient { AMBIENT } else { 0 }) | (if instance.visible { 0 } else { HIDDEN }) | (if instance.show_icon { 0 } else { NO_ICON });
        self.ecs_mut().is_some_and(|w| w.add_effect(e, kind, instance.amplifier, instance.duration, flags))
    }
    /// An effect by its name, for the entity `e`; true when the list changed.
    pub fn add_effect_named(&mut self, e: Entity, name: &str, duration: i32, amp: i32) -> bool {
        let Some(kind) = kind_named(name) else { return false };
        self.ecs_mut().is_some_and(|w| w.add_effect(e, kind, amp, duration, 0))
    }
    /// Ends every warden's minute of calm; how many were reached (0 when inert).
    pub fn expire_dig_cooldowns(&mut self) -> usize {
        self.ecs_mut().map_or(0, |w| w.expire_dig_cooldowns())
    }
    /// `/raid glow`: Glowing on every raider of raid `raid`; how many were reached (0 when inert).
    pub fn glow_raiders(&mut self, raid: i32, ticks: i32, amp: i32) -> usize {
        self.ecs_mut().map_or(0, |w| w.glow_raiders(raid, ticks, amp))
    }
    /// Removes the effect at registry index `index` from the entity `id`; true when it was there.
    pub fn remove_effect(&mut self, id: u32, index: i32) -> bool {
        let (Some(e), Some(kind)) = (self.entity_of(id), kind_at(index)) else { return false };
        self.ecs_mut().is_some_and(|w| w.remove_effect(e, kind))
    }
    /// Keeps only the effects at the registry indices `keep`; true when something was removed.
    pub fn retain_effects(&mut self, id: u32, keep: &[i32]) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let kinds: Vec<Kind> = keep.iter().filter_map(|&i| kind_at(i)).collect();
        self.ecs_mut().is_some_and(|w| w.retain_effects(e, &kinds))
    }
}
