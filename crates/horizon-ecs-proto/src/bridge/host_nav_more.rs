//! The host's navigation writes of a mod on a creature, by entity: a path it supplies (followed as given, no search), its flags and
//! its path maluses. The reads are by entity too; the state lives in `nav_supplied`.
use crate::bridge::host::EcsWorld;
use crate::nav_supplied;
use bevy_ecs::entity::Entity;
use horizon_client_lib::mobs::MobWorld;

impl EcsWorld {
    /// `PathNavigation.moveTo(Path, speed)` for a path a mod supplies; false for a creature that walks no path or no node is left.
    pub fn nav_move_to_supplied(&mut self, mw: &MobWorld<'_>, e: Entity, nodes: &[[f64; 3]], speed: f64) -> bool {
        nav_supplied::follow(self.world_mut(), mw, e, nodes, speed)
    }

    /// The flag `name` of `e` (`can_float`, `can_open_doors`, `can_pass_doors`, `avoid_sun`); None for an unknown name.
    pub fn nav_flag(&self, e: Entity, name: &str) -> Option<bool> {
        nav_supplied::flag(self.world(), e, name)
    }

    pub fn set_nav_flag(&mut self, e: Entity, name: &str, on: bool) -> bool {
        nav_supplied::set_flag(self.world_mut(), e, name, on)
    }

    /// The path malus of `kind` (a `PathType` name such as `WATER`) for `e`; None for an unknown type.
    pub fn nav_malus(&self, e: Entity, kind: &str) -> Option<f32> {
        nav_supplied::malus(self.world(), e, kind)
    }

    pub fn set_nav_malus(&mut self, e: Entity, kind: &str, value: f32) -> bool {
        nav_supplied::set_malus(self.world_mut(), e, kind, value)
    }
}
