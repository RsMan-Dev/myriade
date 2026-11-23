//! The creaking heart's reads and writes on a creaking by network id (see `host_creaking` in the ECS bridge). Inert on a client level.
use super::level::EcsLevel;
use bevy_ecs::entity::Entity;
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::bridge::{host_creaking::HeartLink, uuids};

impl EcsLevel {
    /// `EcsWorld::spawn_creaking`: the new creaking's network id, linked with `alloc`.
    pub fn spawn_creaking(&mut self, mw: &MobWorld<'_>, at: [f64; 3], yaw: f32, bonus: f64, heart: [i32; 3], uuid: [i32; 4], alloc: &mut dyn FnMut() -> u32) -> Option<u32> {
        let e = self.ecs_mut()?.spawn_creaking(mw, at, yaw, bonus, heart, uuid)?;
        Some(self.id_for(e, alloc))
    }

    /// The loaded creaking that holds `uuid`, its network id linked with `alloc` when it has none.
    pub fn creaking_for_uuid(&mut self, uuid: [i32; 4], alloc: &mut dyn FnMut() -> u32) -> Option<u32> {
        let e = uuids::entity_by_uuid(self.world()?, uuid)?;
        self.ecs()?.heart_link(e)?;
        Some(self.id_for(e, alloc))
    }

    fn entity(&self, id: u32) -> Option<Entity> {
        self.entity_of(id)
    }

    /// The heart a creaking is linked to.
    pub fn heart_link(&self, id: u32) -> Option<HeartLink> {
        self.ecs()?.heart_link(self.entity(id)?)
    }

    /// The creaking is alive, awake and linked to a heart.
    pub fn creaking_present(&self, id: u32) -> bool {
        self.entity(id).is_some_and(|e| self.ecs().is_some_and(|w| w.heart_present(e)))
    }

    /// Gives the entity `id` the uuid `uuid`.
    pub fn give_uuid(&mut self, id: u32, uuid: [i32; 4]) {
        if let (Some(e), Some(w)) = (self.entity(id), self.ecs_mut()) {
            uuids::set_uuid(w.world_mut(), e, uuid);
        }
    }

    pub fn creaking_persistent(&self, id: u32) -> bool {
        self.entity(id).is_some_and(|e| self.ecs().is_some_and(|w| w.is_persistent(e)))
    }

    pub fn creaking_feet(&self, id: u32) -> Option<Vec3> {
        self.ecs()?.feet(self.entity(id)?)
    }

    /// The box of the linked creaking, asleep or not.
    pub fn creaking_box(&self, id: u32) -> Option<Aabb> {
        self.ecs()?.heart_bounds(self.entity(id)?)
    }

    pub fn set_tearing_down(&mut self, id: u32) -> bool {
        let Some(e) = self.entity(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_tearing_down(e))
    }

    pub fn zero_health(&mut self, id: u32) {
        if let (Some(e), Some(w)) = (self.entity(id), self.ecs_mut()) {
            w.kill_linked(e);
        }
    }

    /// `Creaking.tick` with no heart of its own: `setHealth(0)`.
    pub fn kill_orphan(&mut self, mw: &MobWorld<'_>, id: u32) {
        if let (Some(e), Some(w)) = (self.entity(id), self.ecs_mut()) {
            w.kill_orphan(mw, e);
        }
    }

    pub fn creaking_voice(&mut self, id: u32, name: &'static str) {
        if let (Some(e), Some(w)) = (self.entity(id), self.ecs_mut()) {
            w.creaking_voice(e, name);
        }
    }

    /// `Creaking.tearDown`: centre and spread of the crumbling box; the creaking is gone.
    pub fn tear_down_creaking(&mut self, id: u32) -> Option<([f64; 3], [f64; 3])> {
        let e = self.entity(id)?;
        self.ecs_mut()?.tear_down_creaking(e)
    }

    pub fn creaking_player_stuck(&mut self, id: u32) -> bool {
        let Some(e) = self.entity(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.creaking_player_stuck(e))
    }

    /// `Level.getNearestPlayer(at, range, false)`.
    pub fn nearest_player_within(&self, at: [f64; 3], range: f64) -> Option<u32> {
        self.ecs()?.nearest_player_within(at, range)
    }
}
