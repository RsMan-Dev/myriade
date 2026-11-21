//! The navigation writes and reads of a mod on `EcsLevel`, by network id: a supplied path, the flags and the path maluses. An inert
//! level takes nothing: false or None.
use super::level::EcsLevel;
use crate::mobs::MobWorld;

impl EcsLevel {
    /// `EcsWorld::nav_move_to_supplied` for the creature `id`; false when it walks no path or the level is inert.
    pub fn nav_move_to_supplied(&mut self, mw: &MobWorld<'_>, id: u32, nodes: &[[f64; 3]], speed: f64) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.nav_move_to_supplied(mw, e, nodes, speed))
    }

    pub fn nav_flag(&self, id: u32, name: &str) -> Option<bool> {
        self.ecs()?.nav_flag(self.entity_of(id)?, name)
    }

    pub fn set_nav_flag(&mut self, id: u32, name: &str, on: bool) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_nav_flag(e, name, on))
    }

    pub fn nav_malus(&self, id: u32, kind: &str) -> Option<f32> {
        self.ecs()?.nav_malus(self.entity_of(id)?, kind)
    }

    pub fn set_nav_malus(&mut self, id: u32, kind: &str, value: f32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_nav_malus(e, kind, value))
    }
}
