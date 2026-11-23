//! Reads by network id, for the frontiers that hold an id and not an entity (tracker, packets, commands): the id is turned into
//! its entity by the level's table, then read from the view. An entity that sleeps (`Disabled`) is absent from every read.
use super::level::EcsLevel;
use bevy_ecs::entity::Entity;
use horizon_ecs_proto::{behaviours::inventory, bridge::{host::Select, ids::id_of, view}, item, mount, targeting::Who};

impl EcsLevel {
    /// `inventory::count`: how many of `item` the villager `e` holds.
    pub fn count(&self, e: Entity, item: &str) -> i32 {
        self.world().map_or(0, |w| inventory::count(w, e, item))
    }
    /// `item::kind_of`: the kind of the item entity `e`.
    pub fn kind_of(&self, e: Entity) -> Option<&'static str> {
        item::kind_of(self.world()?, e)
    }
    /// `mount::rider_of`: who rides `vehicle`.
    pub fn rider_of(&self, vehicle: Entity) -> Option<Who> {
        mount::rider_of(self.world()?, vehicle)
    }
    pub fn kind(&self, id: u32) -> Option<&'static str> {
        Some(view::view(self.world()?, self.entity_of(id)?)?.kind)
    }
    pub fn position(&self, id: u32) -> Option<[f64; 3]> {
        view::position(self.world()?, self.entity_of(id)?)
    }
    pub fn exists(&self, id: u32) -> bool {
        self.world().zip(self.entity_of(id)).is_some_and(|(w, e)| view::view(w, e).is_some())
    }
    /// The ids of the followed entities within `r` of `at`, from the spatial index.
    pub fn ids_near(&self, at: [f64; 3], r: f64) -> Vec<u32> {
        let Some(w) = self.world() else { return Vec::new() };
        let q = Select { kind: None, tag: None, near: Some((at, r)), limit: usize::MAX };
        self.select(&q).into_iter().filter_map(|e| id_of(w, e)).collect()
    }
}
