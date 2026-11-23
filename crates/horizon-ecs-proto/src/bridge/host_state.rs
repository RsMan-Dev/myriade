//! The states a creature keeps beside its physics, through the bridge (`custom_name`, `no_gravity`, `left_handed`, `stuck`, `gliding`,
//! `pose`): what the host reads and writes. Each is a component of its own, absent when neutral.
use crate::bridge::host::EcsWorld;
use crate::custom_name::{self, NameVisible};
use crate::despawn;
use crate::gliding;
use crate::left_handed;
use crate::no_gravity;
use crate::pose_state;
use crate::stuck;
use bevy_ecs::{entity::Entity, world::World};

impl EcsWorld {
    /// `EntityType.canSerialize` of the entity's type (`NameTagItem` names no other: the leash knot, the lightning bolt, the bobber).
    pub fn can_serialize(&self, e: Entity) -> bool {
        let w = self.world();
        w.get::<crate::cow::SpeciesId>(e).is_some_and(|s| w.resource::<crate::cow::SpeciesTable>().0[s.0 as usize].saved)
    }
    pub fn custom_name(&self, e: Entity) -> Option<String> {
        custom_name::get(self.world(), e)
    }
    pub fn set_custom_name(&mut self, e: Entity, name: Option<String>) {
        custom_name::set(self.world_mut(), e, name);
    }
    /// `Entity.setCustomName(null)`: the name goes.
    pub fn clear_name(&mut self, e: Entity) {
        custom_name::set(self.world_mut(), e, None);
    }
    pub fn name_visible(&self, e: Entity) -> bool {
        name_visible(self.world(), e)
    }
    pub fn set_name_visible(&mut self, e: Entity, on: bool) {
        custom_name::set_visible(self.world_mut(), e, on);
    }
    pub fn no_gravity(&self, e: Entity) -> bool {
        no_gravity::get(self.world(), e)
    }
    pub fn set_no_gravity(&mut self, e: Entity, on: bool) {
        no_gravity::set(self.world_mut(), e, on);
    }
    pub fn left_handed(&self, e: Entity) -> bool {
        left_handed::get(self.world(), e)
    }
    pub fn set_left_handed(&mut self, e: Entity, on: bool) {
        left_handed::set(self.world_mut(), e, on);
    }
    /// The arrows and stingers stuck in the creature, with their times.
    pub fn stuck(&self, e: Entity) -> stuck::Stuck {
        stuck::get(self.world(), e)
    }
    pub fn put_stuck(&mut self, e: Entity, s: stuck::Stuck) {
        stuck::put(self.world_mut(), e, s);
    }
    pub fn set_stuck(&mut self, e: Entity, arrows: i32, stingers: i32) {
        stuck::set(self.world_mut(), e, arrows, stingers);
    }
    /// `LivingEntity.getFallFlyingTicks`: the ticks of the glide so far.
    pub fn fall_fly_ticks(&self, e: Entity) -> i32 {
        gliding::ticks(self.world(), e)
    }
    pub fn set_gliding(&mut self, e: Entity, on: bool) {
        gliding::set(self.world_mut(), e, on);
    }
    /// The pose id an event imposed (`Pose.id`); None while the creature stands.
    pub fn pose(&self, e: Entity) -> Option<u8> {
        pose_state::get(self.world(), e)
    }
    pub fn set_pose(&mut self, e: Entity, pose: Option<u8>) {
        pose_state::set(self.world_mut(), e, pose);
    }
    /// `LivingEntity.getNoActionTime`: the ticks since the count started (`despawn::idle_for`); 0 for a creature without a rule.
    pub fn no_action_time(&self, e: Entity) -> i32 {
        no_action_time(self.world(), e)
    }
    /// `LivingEntity.setNoActionTime`: the count is set to `ticks`. False, nothing done, for a creature without a rule.
    pub fn set_no_action_time(&mut self, e: Entity, ticks: i32) -> bool {
        if despawn::idle_for(self.world(), e).is_none() {
            return false;
        }
        despawn::set_idle(self.world_mut(), e, ticks);
        true
    }
}

// The reads of the step view, over the world itself: `EcsWorld`'s methods above delegate to these.

/// Whether `e` shows its name tag (`NameVisible`).
pub fn name_visible(w: &World, e: Entity) -> bool {
    w.get::<NameVisible>(e).is_some()
}

/// `LivingEntity.getNoActionTime`: see `EcsWorld::no_action_time`.
pub fn no_action_time(w: &World, e: Entity) -> i32 {
    despawn::idle_for(w, e).unwrap_or(0)
}
