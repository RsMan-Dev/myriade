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

    pub fn set_patrol_leader(&mut self, id: u32, on: bool) -> bool {
        self.write_id(id, |w, e| w.set_patrol_leader(e, on))
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
    pub fn set_target(&mut self, mw: &MobWorld<'_>, id: u32, who: Option<Who>) -> bool {
        self.write_id(id, |w, e| w.set_target(mw, e, who))
    }

    /// `EcsWorld::bee_forget_hive` for the bee `e` (the hive it went to is full or gone).
    pub fn bee_forget_hive(&mut self, e: Entity) {
        if let Some(w) = self.ecs_mut() {
            w.bee_forget_hive(e);
        }
    }

    /// `EcsWorld::dolphin_treasure` for the dolphin `e`: the structure the host located for it, or none.
    pub fn dolphin_treasure(&mut self, e: Entity, at: Option<[i32; 3]>) {
        if let Some(w) = self.ecs_mut() {
            w.dolphin_treasure(e, at);
        }
    }

    /// `EcsWorld::bee_stay_out` for the creature `id`: it keeps out of a hive for `ticks`.
    pub fn bee_stay_out(&mut self, id: u32, ticks: u64) -> bool {
        self.write_id(id, |w, e| w.bee_stay_out(e, ticks))
    }

    /// `EcsWorld::bee_exited` for the bee `e` made from the data of a hive.
    pub fn bee_exited(&mut self, mw: &MobWorld<'_>, e: Entity, hive: [i32; 3], facing: [i32; 2], blocked: bool, delivered: bool) {
        if let Some(w) = self.ecs_mut() {
            w.bee_exited(mw, e, hive, facing, blocked, delivered);
        }
    }

    /// `EcsWorld::hive_is_full`: whether the bees know the hive at `hive` as full.
    pub fn hive_is_full(&self, hive: [i32; 3]) -> bool {
        self.ecs().is_some_and(|w| w.hive_is_full(hive))
    }

    /// `EcsWorld::set_hive_full`: the hive at `hive` holds three occupants or has room.
    pub fn set_hive_full(&mut self, hive: [i32; 3], full: bool) {
        if let Some(w) = self.ecs_mut() {
            w.set_hive_full(hive, full);
        }
    }

    /// `EcsWorld::untargeted_bees_in`: the bees in `area` with no target.
    pub fn untargeted_bees_in(&self, area: horizon_core::geometry::Aabb) -> Vec<Entity> {
        self.ecs().map_or_else(Vec::new, |w| w.untargeted_bees_in(area))
    }
}
