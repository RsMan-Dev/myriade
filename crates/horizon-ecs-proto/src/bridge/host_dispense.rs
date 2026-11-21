//! The dispenser's effects through the bridge: a creature shorn, brushed or primed, and an egg's name. Each writes P's state and leaves the sounds and the
//! loot to the host's dispenser blocks (blocks/dispenser.rs).
use crate::bridge::host::EcsWorld;
use crate::cow::{after, now_of};
use crate::custom_name;
use crate::dispense;
use crate::targeting::Who;
use crate::tnt::{self, Primed};
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::nbt::Compound;

impl EcsWorld {
    /// `Shearable.shear` / unshear: sets `Sheared` on `e`. False when `e` is not shearable or the state
    /// already holds.
    pub fn set_sheared(&mut self, e: Entity, on: bool) -> bool {
        let w = self.world_mut();
        if !dispense::set_sheared(w, e, on) {
            return false;
        }
        true
    }

    pub fn is_sheared(&self, e: Entity) -> bool {
        dispense::is_sheared(self.world(), e)
    }

    /// `TntBlock.prime` on an entity: its fuse is the TNT's (`FUSE` ticks, `POWER`), credited to `owner`; false when `e` is gone or already lit.
    pub fn prime(&mut self, e: Entity, owner: Option<Who>) -> bool {
        let w = self.world_mut();
        if w.get_entity(e).is_err() || w.get::<Primed>(e).is_some() {
            return false;
        }
        let fuse_at = now_of(w, e) + tnt::FUSE as u64;
        w.entity_mut(e).insert(Primed { fuse_at, power: tnt::POWER, owner, fell: 0.0 });
        after(w, e, tnt::FUSE, tnt::primed_due);
        true
    }

    /// `Item.useOn` of an egg: the given `entity_data` and `bucket_entity_data` are loaded onto `e` (`merge_nbt`), then the stack's custom
    /// name goes on it when it has one.
    pub fn apply_egg_components(&mut self, e: Entity, stack: &ItemStack, entity_data: Option<&Compound>, bucket_entity_data: Option<&Compound>) {
        if self.world().get_entity(e).is_err() {
            return;
        }
        for patch in [entity_data, bucket_entity_data].into_iter().flatten() {
            self.merge_nbt(e, patch);
        }
        if let Some(name) = stack.custom_name() {
            custom_name::set(self.world_mut(), e, Some(name));
        }
    }

    /// One brush stroke on `e`: every stack it rolls from `BRUSH_TABLE` is returned. Empty when `e` is not
    /// brushable.
    pub fn brush(&mut self, e: Entity) -> Vec<ItemStack> {
        let w = self.world_mut();
        dispense::brush_drop(w, e)
    }

}
