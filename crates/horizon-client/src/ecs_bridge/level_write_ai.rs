//! The AI's writes on `EcsLevel`: the path, the wish, the look, the restriction, the sleep, the age, the pause, the aggression and the
//! no-action count of a creature, by its network id; the stop of a navigation by entity. An inert level takes nothing: false.
use super::level::EcsLevel;
use crate::mobs::MobWorld;
use bevy_ecs::entity::Entity;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{bridge::host::EcsWorld, targeting::Who};

impl EcsLevel {
    /// `EcsWorld::stop_navigation`.
    pub fn stop_navigation(&mut self, e: Entity) {
        if let Some(w) = self.ecs_mut() {
            w.stop_navigation(e);
        }
    }

    /// Runs `f` on the world and the entity of `id`; false when the level is inert or the id has no entity.
    fn write_id(&mut self, id: u32, f: impl FnOnce(&mut EcsWorld, Entity)) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(w) = self.ecs_mut() else { return false };
        f(w, e);
        true
    }

    /// `EcsWorld::nav_move_to` for the creature `id`; false when it cannot walk. The mob world is not read by the walk.
    pub fn nav_move_to(&mut self, _mw: &MobWorld<'_>, id: u32, at: [f64; 3], speed: f64) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.nav_move_to(e, at, speed))
    }

    pub fn nav_stop(&mut self, id: u32) -> bool {
        self.write_id(id, |w, e| w.stop_navigation(e))
    }

    pub fn set_wanted(&mut self, id: u32, at: [f64; 3], speed: f64) -> bool {
        self.write_id(id, |w, e| w.set_wanted(e, at, speed))
    }

    pub fn look_at(&mut self, id: u32, at: [f64; 3]) -> bool {
        self.write_id(id, |w, e| w.look_at(e, at))
    }

    pub fn set_aggressive(&mut self, id: u32, on: bool) -> bool {
        self.write_id(id, |w, e| w.set_aggressive(e, on))
    }

    pub fn set_ai_paused(&mut self, id: u32, on: bool) {
        self.write_id(id, |w, e| w.set_ai_paused(e, on));
    }

    pub fn set_baby(&mut self, id: u32, baby: bool) -> bool {
        self.write_id(id, |w, e| w.set_baby(e, baby))
    }

    pub fn restrict_to(&mut self, id: u32, center: [i32; 3], radius: i32) -> bool {
        self.write_id(id, |w, e| w.restrict_to(e, center, radius))
    }

    pub fn clear_restriction(&mut self, id: u32) -> bool {
        self.write_id(id, |w, e| w.clear_restriction(e))
    }

    /// `EcsWorld::set_no_action_time`: the count is an `i32` on both sides.
    pub fn set_no_action_time(&mut self, id: u32, ticks: i32) -> bool {
        self.write_id(id, |w, e| {
            w.set_no_action_time(e, ticks);
        })
    }

    pub fn start_sleeping(&mut self, id: u32, pos: [i32; 3], at: Vec3) -> bool {
        self.write_id(id, |w, e| w.start_sleeping(e, pos, at))
    }

    pub fn stop_sleeping(&mut self, id: u32) -> bool {
        self.write_id(id, |w, e| w.stop_sleeping(e))
    }

    /// `EcsWorld::set_target` for the creature `id`; `None` takes its target away.
    pub fn set_target(&mut self, id: u32, who: Option<Who>) -> bool {
        self.write_id(id, |w, e| w.set_target(e, who))
    }
}
