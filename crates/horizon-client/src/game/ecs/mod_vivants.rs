//! The living entities of native mods on the ECS: a mob is read from its components by its network id (`EcsLevel::entity_of`),
//! and the remote list (`remote_entities`, the client role's) by `remote_mob`. The server never fills that list, so the
//! branches of the `native_living_*` files answer from the world where it runs, and from the list only on a client.
use super::super::Game;
use bevy_ecs::{entity::Entity, world::World};
use horizon_client::mobs::Mob;
use horizon_ecs_proto::cow::{Health, MaxHealth};

impl Game<'_> {
    /// The mob `id` of the remote list (a client's rendering list): the old read of `remote_entities`.
    pub(in crate::game) fn remote_mob(&self, id: u32) -> Option<&Mob> {
        self.remote_entities.iter().find(|m| m.id == id && !m.removed)
    }

    /// `remote_mob`, for a write.
    pub(in crate::game) fn remote_mob_mut(&mut self, id: u32) -> Option<&mut Mob> {
        self.remote_entities.iter_mut().find(|m| m.id == id && !m.removed)
    }

    /// `remote_mob` without the removed filter (a mob being removed still answers there).
    pub(in crate::game) fn remote_mob_any(&self, id: u32) -> Option<&Mob> {
        self.remote_entities.iter().find(|m| m.id == id)
    }

    /// `remote_mob_any`, for a write.
    pub(in crate::game) fn remote_mob_any_mut(&mut self, id: u32) -> Option<&mut Mob> {
        self.remote_entities.iter_mut().find(|m| m.id == id)
    }

    /// Takes the remote list out, for a Mob-side write (`JoinList::lift`); empty on the server.
    pub(in crate::game) fn remote_lift(&mut self) -> horizon_client::join_seq::JoinList<Mob> {
        self.remote_entities.lift()
    }

    /// Puts the list taken by `remote_lift` back (`JoinList::restore`, survivors first).
    pub(in crate::game) fn remote_restore(&mut self, taken: horizon_client::join_seq::JoinList<Mob>) {
        self.remote_entities.restore(taken, horizon_client::join_seq::RestoreOrder::SurvivorsFirst);
    }

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
        w.get::<MaxHealth>(e).map(|h| h.0)
    }
}
