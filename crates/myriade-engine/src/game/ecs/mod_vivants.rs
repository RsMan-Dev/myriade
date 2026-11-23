//! The living entities of native mods on the ECS: a mob is read from its components by its network id (`EcsLevel::entity_of`).
//! The branches of the `native_living_*` files answer from the world, which holds every mob.
use super::super::Game;
use bevy_ecs::{entity::Entity, world::World};
use horizon_ecs_proto::cow::{Health, MaxHealth};

impl Game<'_> {
    /// The world and the entity of the living `id`, when the level runs and the mob is awake (`EcsLevel::exists`).
    pub(in crate::game) fn ecs_living(&self, id: u32) -> Option<(&World, Entity)> {
        let w = self.ecs.world()?;
        let e = self.ecs.entity_of(id)?;
        self.ecs.exists(id).then_some((w, e))
    }

    /// `LivingEntity.health` of the mob `id` on the ECS: the `Health` component.
    pub(in crate::game) fn ecs_living_health(&self, id: u32) -> Option<f32> {
        let (w, e) = self.ecs_living(id)?;
        w.get::<Health>(e).map(|h| h.0)
    }

    /// `LivingEntity.getMaxHealth` of the mob `id` on the ECS: the `MaxHealth` component.
    pub(in crate::game) fn ecs_living_max_health(&self, id: u32) -> Option<f32> {
        let (w, e) = self.ecs_living(id)?;
        w.get::<MaxHealth>(e).map(|h| h.0).or_else(|| w.get::<horizon_ecs_proto::cow::SpeciesId>(e).is_some().then(|| horizon_ecs_proto::cow::max_health(w, e)))
    }
}
