//! The dispenser's writes on `EcsLevel`, by the creature's id: shearing, priming, brushing and an egg's name. An inert level takes none of
//! them.
use super::level::EcsLevel;
use horizon_client_lib::items::ItemStack;
use horizon_core::nbt::Compound;
use horizon_ecs_proto::targeting::Who;

impl EcsLevel {
    /// `EcsWorld::set_sheared` on the creature `id`; false when the level is inert, `id` is unknown or the state already holds.
    pub fn set_sheared(&mut self, id: u32, on: bool) -> bool {
        self.ecs_mut().is_some_and(|w| w.entity_of(id).is_some_and(|e| w.set_sheared(e, on)))
    }

    /// `EcsWorld::prime` on the entity `id`, credited to `owner`; false when inert, unknown or already lit.
    pub fn prime(&mut self, id: u32, owner: Option<Who>) -> bool {
        self.ecs_mut().is_some_and(|w| w.entity_of(id).is_some_and(|e| w.prime(e, owner)))
    }

    /// `EcsWorld::apply_egg_components` on the entity `id`.
    pub fn apply_egg_components(&mut self, id: u32, stack: &ItemStack, entity_data: Option<&Compound>, bucket_entity_data: Option<&Compound>) {
        let Some(w) = self.ecs_mut() else { return };
        if let Some(e) = w.entity_of(id) {
            w.apply_egg_components(e, stack, entity_data, bucket_entity_data);
        }
    }

    /// One brush stroke on the entity `id`: the stacks it drops, each also queued as a `Brushed` event. Empty when inert or unknown.
    pub fn brush(&mut self, id: u32) -> Vec<ItemStack> {
        let Some(w) = self.ecs_mut() else { return Vec::new() };
        w.entity_of(id).map(|e| w.brush(e)).unwrap_or_default()
    }
}
