//! The lightning, end crystals, fangs and area clouds on the bridge: each spawn runs under the lent world, and what they queue is read with
//! `EcsWorld::drain_misc`.
use crate::bridge::host::EcsWorld;
use crate::cloud_tick;
use crate::effect_cloud::CloudSpec;
use crate::colony_register::MiscOut;
use crate::end_crystal::Blow;
use crate::end_crystal_tick;
use crate::fangs_tick;
use crate::lightning::BoltLevel;
use crate::lightning_tick;
use crate::targeting::Who;
use bevy_ecs::entity::Entity;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

fn at_of(at: [f64; 3]) -> Vec3 {
    Vec3::new(at[0], at[1], at[2])
}

impl EcsWorld {
    /// A bolt at `at`; `cosmetic` for a skeleton trap's; `level` the host's blocks for the strike.
    pub fn strike(&mut self, mw: &MobWorld<'_>, at: [f64; 3], cosmetic: bool, level: Option<Box<dyn BoltLevel>>) -> Entity {
        self.lent(mw, |w| lightning_tick::strike(w, at_of(at), cosmetic, level))
    }

    /// An end crystal at `at` (its feet).
    pub fn spawn_crystal(&mut self, mw: &MobWorld<'_>, at: [f64; 3], show_bottom: bool) -> Entity {
        self.lent(mw, |w| end_crystal_tick::spawn(w, at_of(at), show_bottom))
    }

    /// A blow on a crystal; true when it goes (its blast is queued).
    pub fn hurt_crystal(&mut self, mw: &MobWorld<'_>, e: Entity, blow: Blow) -> bool {
        self.lent(mw, |w| end_crystal_tick::hurt(w, e, blow))
    }

    /// Evoker fangs at `at`, biting after `warmup` ticks for `owner`.
    pub fn spawn_fangs(&mut self, mw: &MobWorld<'_>, at: [f64; 3], yaw: f32, warmup: i32, owner: Option<Who>) -> Entity {
        self.lent(mw, |w| fangs_tick::spawn(w, at_of(at), yaw, warmup, owner))
    }

    /// A potion cloud at `at`, from its `CloudSpec` (`CloudSpec::new`, the jar's defaults, and the fields the caller changes).
    pub fn spawn_cloud(&mut self, mw: &MobWorld<'_>, at: [f64; 3], spec: CloudSpec) -> Entity {
        self.lent(mw, |w| cloud_tick::spawn(w, at_of(at), spec))
    }

}
