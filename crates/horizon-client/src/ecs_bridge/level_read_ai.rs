//! The AI's reads by network id (`u32`): the id is turned into its entity by the level's table, then read from the ECS world
//! (`host_read_ai`). An absent entity, and an inert level, read as None.
use super::level::EcsLevel;
use crate::mobs::path::PathType;
use bevy_ecs::entity::Entity;
use horizon_ecs_proto::{
    bridge::{host::EcsWorld, ids::id_of},
    targeting::Who,
};

/// The ECS's `Who` as the server's: a player is `Player`, an entity is `Mob` by its id (None while the entity has no id).
fn mob_who(w: &EcsWorld, who: Who) -> Option<crate::mobs::Who> {
    match who {
        Who::Player(_) => Some(crate::mobs::Who::Player),
        Who::Entity(e) => id_of(w.world(), e).map(crate::mobs::Who::Mob),
    }
}

impl EcsLevel {
    /// Runs `f` on the world and on the entity of `id`; None when the level is inert or the id is unknown.
    fn by_id<R>(&self, id: u32, f: impl FnOnce(&EcsWorld, Entity) -> R) -> Option<R> {
        let w = self.ecs()?;
        Some(f(w, self.entity_of(id)?))
    }

    pub fn ai_target(&self, id: u32) -> Option<crate::mobs::Who> {
        self.by_id(id, |w, e| w.ai_target(e).and_then(|who| mob_who(w, who))).flatten()
    }
    pub fn last_hurt_by(&self, id: u32) -> Option<crate::mobs::Who> {
        self.by_id(id, |w, e| w.last_hurt_by(e).and_then(|who| mob_who(w, who))).flatten()
    }
    pub fn vex_owner(&self, id: u32) -> Option<u32> {
        self.by_id(id, |w, e| w.vex_owner(e).and_then(|o| id_of(w.world(), o))).flatten()
    }
    pub fn aggressive(&self, id: u32) -> Option<bool> {
        self.by_id(id, |w, e| w.aggressive(e))
    }
    pub fn ai_paused(&self, id: u32) -> Option<bool> {
        self.by_id(id, |w, e| w.ai_paused(e))
    }
    pub fn wanted(&self, id: u32) -> Option<[f64; 3]> {
        self.by_id(id, |w, e| w.wanted(e)).flatten()
    }
    pub fn move_speed_modifier(&self, id: u32) -> Option<f64> {
        self.by_id(id, |w, e| w.move_speed_modifier(e)).flatten()
    }
    pub fn nav_is_done(&self, id: u32) -> Option<bool> {
        self.by_id(id, |w, e| w.nav_is_done(e))
    }
    pub fn nav_speed_modifier(&self, id: u32) -> Option<f64> {
        self.by_id(id, |w, e| w.nav_speed_modifier(e)).flatten()
    }
    pub fn malus(&self, id: u32, kind: PathType) -> Option<f32> {
        self.by_id(id, |w, e| w.malus(e, kind as u8)).flatten()
    }
    pub fn has_restriction(&self, id: u32) -> Option<bool> {
        self.by_id(id, |w, e| w.has_restriction(e))
    }
    pub fn restrict_center(&self, id: u32) -> Option<[i32; 3]> {
        self.by_id(id, |w, e| w.restrict_center(e)).flatten()
    }
    pub fn restrict_radius(&self, id: u32) -> Option<i32> {
        self.by_id(id, |w, e| w.restrict_radius(e)).flatten()
    }
    pub fn is_within_restriction(&self, id: u32, at: [f64; 3]) -> Option<bool> {
        self.by_id(id, |w, e| w.is_within_restriction(e, at))
    }
    pub fn sleeping_pos(&self, id: u32) -> Option<[i32; 3]> {
        self.by_id(id, |w, e| w.sleeping_pos(e)).flatten()
    }
    pub fn can_pick_up_loot(&self, id: u32) -> Option<bool> {
        self.by_id(id, |w, e| w.can_pick_up_loot(e)).flatten()
    }
    pub fn requires_custom_persistence(&self, id: u32) -> Option<bool> {
        self.by_id(id, |w, e| w.requires_custom_persistence(e))
    }
}
