//! The leads and the uuids of the ECS entities, on `EcsLevel`: the bridge's free functions (`leash_save`, `uuids`) on the level's world.
//! An inert level answers as an empty world: nothing held, nothing attached, no uuid.
use super::level::EcsLevel;
use bevy_ecs::entity::Entity;

const LEASH_KNOT: &str = "minecraft:leash_knot";
use horizon_core::nbt::Tag;
use horizon_ecs_proto::{
    bridge::{leash_save, uuids, view},
    leash::{self, Holder},
};

impl EcsLevel {
    /// The `leash` key of `e`'s lead, written into `tag` (`leash_save::save`).
    pub fn leash_save(&self, e: Entity, tag: &mut Tag) {
        if let Some(w) = self.world() {
            leash_save::save(w, e, tag);
        }
    }
    /// Reads the `leash` key of `tag` as a pending lead on `e` (`leash_save::load`).
    pub fn leash_load(&mut self, e: Entity, tag: &Tag) {
        if let Some(w) = self.world_mut() {
            leash_save::load(w, e, tag);
        }
    }
    /// Ties the pending leads whose holder is there (`leash_save::resolve_pending`).
    pub fn leash_resolve_pending(&mut self) {
        if let Some(w) = self.world_mut() {
            leash_save::resolve_pending(w);
        }
    }
    pub fn leash_can_attach(&self, e: Entity, center: [f64; 3]) -> bool {
        self.world().is_some_and(|w| leash_save::can_attach(w, e, center))
    }
    pub fn leash_can_be_leashed(&self, e: Entity) -> bool {
        self.world().is_some_and(|w| leash_save::can_be_leashed(w, e))
    }
    /// Marks the view of the holder `e`: its lead moved.
    pub fn notify_leash_holder(&mut self, e: Entity) {
        if let Some(w) = self.world_mut() {
            view::mark(w, e);
        }
    }
    /// Puts the lead of `e` on `holder` (`leash::attach`), or takes it off when `holder` is None (`leash::detach`); false when refused.
    pub fn set_leash(&mut self, e: Entity, holder: Option<Holder>) -> bool {
        let Some(w) = self.world_mut() else { return false };
        match holder {
            Some(h) => leash::attach(w, e, h),
            None => leash::detach(w, e).is_some(),
        }
    }
    pub fn set_uuid(&mut self, e: Entity, uuid: [i32; 4]) {
        if let Some(w) = self.world_mut() {
            uuids::set_uuid(w, e, uuid);
        }
    }
    pub fn entity_by_uuid(&self, uuid: [i32; 4]) -> Option<Entity> {
        self.world().and_then(|w| uuids::entity_by_uuid(w, uuid))
    }
    /// The knot of the fence `fence`, made by the ECS and linked to `alloc`'s id; 0 when inert.
    pub fn knot_tie(&mut self, fence: [i32; 3], alloc: &mut dyn FnMut() -> u32) -> u32 {
        let Some(w) = self.ecs_mut() else { return 0 };
        let e = w.leash_tie_knot(fence);
        w.id_for(e, alloc)
    }
    /// A leashee left the knot of `fence`: the ECS discards it when no lead holds it.
    pub fn knot_released(&mut self, fence: [i32; 3]) {
        if let Some(w) = self.ecs_mut() {
            w.leash_knot_released(fence);
        }
    }
    /// The fence of the knot with the network id `id`, None when it is not a knot.
    pub fn knot_fence_of(&self, id: u32) -> Option<[i32; 3]> {
        let e = self.entity_of(id)?;
        self.world().and_then(|w| leash_save::knot_fence(w, e))
    }
    /// The network ids and fences of the knots the ECS holds.
    pub fn knots(&self) -> Vec<(u32, [i32; 3])> {
        self.species_ids(LEASH_KNOT).into_iter().filter_map(|id| self.knot_fence_of(id).map(|f| (id, f))).collect()
    }
}
