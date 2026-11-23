//! The creature states of the bridge (`P bridge/host_state.rs`) by id: reads answer the neutral value and writes say whether the id was
//! in the level. An inert level (a client) holds none of them.
use super::level::EcsLevel;
use crate::mobs::stuck::Stuck;
use bevy_ecs::entity::Entity;
use horizon_ecs_proto::{bridge::host::EcsWorld, stuck::Stuck as EcsStuck};

impl EcsLevel {
    fn read_state<R>(&self, id: u32, neutral: R, f: impl FnOnce(&EcsWorld, Entity) -> R) -> R {
        match (self.entity_of(id), self.ecs()) {
            (Some(e), Some(w)) => f(w, e),
            _ => neutral,
        }
    }
    fn write_state(&mut self, id: u32, f: impl FnOnce(&mut EcsWorld, Entity)) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| {
            f(w, e);
            true
        })
    }
    /// `EntityType.canSerialize` (a name tag names nothing else).
    pub fn can_serialize(&self, id: u32) -> bool {
        self.read_state(id, false, |w, e| w.can_serialize(e))
    }
    pub fn custom_name(&self, id: u32) -> Option<String> {
        self.read_state(id, None, |w, e| w.custom_name(e))
    }
    pub fn set_custom_name(&mut self, id: u32, name: Option<String>) -> bool {
        self.write_state(id, |w, e| w.set_custom_name(e, name))
    }
    pub fn clear_name(&mut self, id: u32) -> bool {
        self.write_state(id, |w, e| w.clear_name(e))
    }
    pub fn name_visible(&self, id: u32) -> bool {
        self.read_state(id, false, |w, e| w.name_visible(e))
    }
    pub fn set_name_visible(&mut self, id: u32, on: bool) -> bool {
        self.write_state(id, |w, e| w.set_name_visible(e, on))
    }
    pub fn no_gravity(&self, id: u32) -> bool {
        self.read_state(id, false, |w, e| w.no_gravity(e))
    }
    pub fn set_no_gravity(&mut self, id: u32, on: bool) -> bool {
        self.write_state(id, |w, e| w.set_no_gravity(e, on))
    }
    pub fn left_handed(&self, id: u32) -> bool {
        self.read_state(id, false, |w, e| w.left_handed(e))
    }
    pub fn set_left_handed(&mut self, id: u32, on: bool) -> bool {
        self.write_state(id, |w, e| w.set_left_handed(e, on))
    }
    pub fn stuck(&self, id: u32) -> (i32, i32) {
        self.read_state(id, (0, 0), |w, e| {
            let s = w.stuck(e);
            (s.arrows, s.stingers)
        })
    }
    /// `Mob.noActionTime` of `id`; None when it is absent.
    pub fn no_action_time(&self, id: u32) -> Option<i32> {
        self.read_state(id, None, |w, e| Some(w.no_action_time(e)))
    }
    pub fn set_stuck(&mut self, id: u32, arrows: i32, stingers: i32) -> bool {
        self.write_state(id, |w, e| w.set_stuck(e, arrows, stingers))
    }
    /// `f` on the creature's `Stuck` (the counts and their times), written back after; None when `id` is absent or the level inert.
    pub fn stuck_with<T>(&mut self, id: u32, f: impl FnOnce(&mut Stuck) -> T) -> Option<T> {
        let e = self.entity_of(id)?;
        let w = self.ecs_mut()?;
        let s = w.stuck(e);
        let mut g = Stuck { arrows: s.arrows, stingers: s.stingers, remove_arrow_time: s.remove_arrow_time, remove_stinger_time: s.remove_stinger_time };
        let r = f(&mut g);
        w.put_stuck(e, EcsStuck { arrows: g.arrows, stingers: g.stingers, remove_arrow_time: g.remove_arrow_time, remove_stinger_time: g.remove_stinger_time });
        Some(r)
    }
    pub fn fall_fly_ticks(&self, id: u32) -> i32 {
        self.read_state(id, 0, |w, e| w.fall_fly_ticks(e))
    }
    pub fn set_gliding(&mut self, id: u32, on: bool) -> bool {
        self.write_state(id, |w, e| w.set_gliding(e, on))
    }
    /// The pose an event imposed; None while the creature stands. The id is `Pose.id`.
    pub fn pose(&self, id: u32) -> Option<myriade_types::Pose> {
        self.read_state(id, None, |w, e| w.pose(e).map(|p| myriade_types::Pose::from_id(i32::from(p))))
    }
    /// Sets the pose; standing (None) removes it. The variants are declared in `Pose.id` order, so the cast is the id.
    pub fn set_pose(&mut self, id: u32, pose: Option<myriade_types::Pose>) -> bool {
        self.write_state(id, |w, e| w.set_pose(e, pose.map(|p| p as u8)))
    }
}
