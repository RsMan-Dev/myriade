//! The boats of a dimension's ECS level: the apparition by wood name, with the network id it gets, and the wood a boat has. Inert on a
//! client, where each answers as an empty level does.
use super::level::EcsLevel;
use crate::mobs::MobWorld;
use bevy_ecs::entity::Entity;
use horizon_core::geometry::Vec3;

impl EcsLevel {
    /// A boat of `wood` (`oak`, `bamboo`...) at `at`, facing `yaw`, with a chest when `chest`. None when inert or when `wood` is unknown.
    pub fn spawn_boat(&mut self, mw: &MobWorld<'_>, wood: &str, chest: bool, at: [f64; 3], yaw: f32) -> Option<Entity> {
        self.ecs_mut()?.spawn_boat(mw, wood, chest, at, yaw)
    }

    /// The same boat, with the network id it is linked to (`alloc` gives a new id when it has none). None when no boat was made.
    pub fn spawn_boat_id(&mut self, mw: &MobWorld<'_>, wood: &str, chest: bool, at: [f64; 3], yaw: f32, alloc: &mut dyn FnMut() -> u32) -> Option<u32> {
        let w = self.ecs_mut()?;
        let e = w.spawn_boat(mw, wood, chest, at, yaw)?;
        Some(w.id_for(e, alloc))
    }

    /// The wood name and chest flag of boat `e`, None when inert or when `e` is no boat.
    pub fn boat_wood(&self, e: Entity) -> Option<(&'static str, bool)> {
        self.ecs()?.boat_wood(e)
    }

    /// The id of the boat the ray from `eye` along `dir` enters first within `reach`, with the distance; `exclude` (the boat the aimer rides)
    /// is left out. None when inert, when no boat is hit, or when that boat has no id yet.
    pub fn boat_pick(&self, eye: [f64; 3], dir: [f64; 3], reach: f64, exclude: Option<u32>) -> Option<(u32, f64)> {
        let ecs = self.ecs()?;
        let skip = exclude.and_then(|id| self.entity_of(id));
        let (e, t) = ecs.boat_pick(eye, dir, reach, skip)?;
        Some((ecs.id_of(e)?, t))
    }

    /// Whether the boat `id` is out of control (under water for `TIME_TO_EJECT` ticks in a row); false when inert or when it is no boat.
    pub fn boat_out_of_control(&self, id: u32) -> bool {
        self.entity_of(id).and_then(|e| self.ecs().map(|w| w.boat_out_of_control(e))).unwrap_or(false)
    }

    /// The ids of the boats whose position lies in `min..=max`; a boat without an id is left out. Empty when inert.
    pub fn boats_in(&self, min: Vec3, max: Vec3) -> Vec<u32> {
        self.ecs().map_or_else(Vec::new, |ecs| ecs.boats_in(min, max).into_iter().filter_map(|e| ecs.id_of(e)).collect())
    }

    /// The position of the seat `slot` of the boat `id` among `count` riders; None when inert, when `id` is no boat or when there is no such seat.
    pub fn boat_seat(&self, id: u32, slot: usize, count: usize) -> Option<Vec3> {
        let e = self.entity_of(id)?;
        self.ecs()?.boat_seat(e, slot, count)
    }

    /// The damage of the boat `id` (the shake's wear, 0 at rest); None when inert or when `id` is no boat.
    pub fn boat_damage(&self, id: u32) -> Option<f32> {
        let e = self.entity_of(id)?;
        self.ecs()?.boat_damage(e)
    }

    /// The `hurtTime` of the boat `id` (its shake's ticks left, 0 at rest); None when inert or when `id` is no boat.
    pub fn boat_hurt_time(&self, id: u32) -> Option<i32> {
        let e = self.entity_of(id)?;
        self.ecs()?.boat_hurt_time(e)
    }

    /// The riders the boat `id` takes; None when inert or when `id` is no boat.
    pub fn boat_max_passengers(&self, id: u32) -> Option<usize> {
        let e = self.entity_of(id)?;
        self.ecs()?.boat_max_passengers(e)
    }
    /// The network ids of every boat of the level, by the boat marker (`EcsWorld::boat_entities`), in id order.
    pub fn boat_ids(&self) -> Vec<u32> {
        let Some(ecs) = self.ecs() else { return Vec::new() };
        let mut ids: Vec<u32> = ecs.boat_entities().into_iter().filter_map(|e| self.id_of(e)).collect();
        ids.sort_unstable();
        ids
    }
}
