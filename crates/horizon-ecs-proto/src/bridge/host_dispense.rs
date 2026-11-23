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
    /// `behaviours::shear::shear_entity` with no player (a dispenser): state, loot and sound of the species. False when not shorn now.
    pub fn shear(&mut self, e: Entity) -> bool {
        crate::behaviours::shear::shear_entity(self.world_mut(), e, None)
    }

    /// `readyForShearing` of the species' definition: false for a creature with none.
    pub fn can_shear(&self, e: Entity) -> bool {
        let w = self.world();
        w.get::<crate::behaviours::shear::Shearing>(e).is_some_and(|s| (s.0.ready)(w, e))
    }

    /// `TntBlock.prime` on an entity: its fuse is the TNT's (`FUSE` ticks, `POWER`), credited to `owner`; false when `e` is gone or already lit.
    pub fn prime(&mut self, e: Entity, owner: Option<Who>) -> bool {
        let w = self.world_mut();
        if w.get_entity(e).is_err() || w.get::<Primed>(e).is_some() {
            return false;
        }
        let fuse_at = now_of(w, e) + tnt::FUSE as u64;
        w.entity_mut(e).insert(Primed { fuse_at, power: tnt::POWER, owner, fell: 0.0, used_portal: false });
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

    /// `Bucketable.setFromBucket(true)` after a bucket emptied a creature: `FromBucket` is saved and the creature is kept
    /// (`requiresCustomPersistence` reads it). False when `e` is gone or its species is not bucketable (the tadpole's is always true).
    pub fn set_from_bucket(&mut self, e: Entity) -> bool {
        let w = self.world_mut();
        let bits = w.get::<crate::cow::SpeciesId>(e).map_or(0, |id| crate::cow::tags_of_id(*id));
        if bits & (crate::cow::tag::BUCKETABLE | crate::cow::tag::TADPOLE) == 0 {
            return false;
        }
        if bits & crate::cow::tag::BUCKETABLE != 0 {
            let mut c = Compound::new();
            c.insert("FromBucket".into(), horizon_core::nbt::Tag::Byte(1));
            if crate::nbt::load(w, e, &c).is_none() {
                return false;
            }
        }
        w.entity_mut(e).insert(crate::despawn::FromBucket);
        true
    }

    /// One brush stroke on `e`: every stack it rolls from `BRUSH_TABLE` is returned. Empty when `e` is not
    /// brushable.
    pub fn brush(&mut self, e: Entity) -> Vec<ItemStack> {
        let w = self.world_mut();
        dispense::brush_drop(w, e)
    }

}
