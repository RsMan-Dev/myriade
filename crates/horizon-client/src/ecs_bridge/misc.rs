//! The lightning, end crystals, evoker fangs and area clouds on the level: each spawn returns its network id, linked on the spot (0 when the
//! level is inert). What the ECS queued for them is taken with `take_misc_events`.
use super::level::EcsLevel;
use crate::mobs::MobWorld;
use horizon_ecs_proto::{effect_cloud::CloudSpec, end_crystal::Blow, lightning::BoltLevel, targeting::Who};

impl EcsLevel {
    /// `EcsWorld::strike`: a bolt at `at`; `cosmetic` for a skeleton trap's; `level` the host's blocks for the strike.
    pub fn strike(&mut self, mw: &MobWorld<'_>, at: [f64; 3], cosmetic: bool, level: Option<Box<dyn BoltLevel>>, alloc: &mut dyn FnMut() -> u32) -> u32 {
        let Some(w) = self.ecs_mut() else { return 0 };
        let e = w.strike(mw, at, cosmetic, level);
        w.id_for(e, alloc)
    }

    /// `EcsWorld::spawn_crystal`: an end crystal at `at` (its feet).
    pub fn spawn_crystal(&mut self, mw: &MobWorld<'_>, at: [f64; 3], show_bottom: bool, alloc: &mut dyn FnMut() -> u32) -> u32 {
        let Some(w) = self.ecs_mut() else { return 0 };
        let e = w.spawn_crystal(mw, at, show_bottom);
        w.id_for(e, alloc)
    }

    /// `EcsWorld::hurt_crystal` on the crystal of network id `id`; false when the level is inert or the id is unknown.
    pub fn hurt_crystal(&mut self, mw: &MobWorld<'_>, id: u32, blow: Blow) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(w) = self.ecs_mut() else { return false };
        w.hurt_crystal(mw, e, blow)
    }

    /// `EcsWorld::spawn_fangs`: evoker fangs at `at`, biting after `warmup` ticks for `owner` (built by the host caller).
    pub fn spawn_fangs(&mut self, mw: &MobWorld<'_>, at: [f64; 3], yaw: f32, warmup: i32, owner: Option<Who>, alloc: &mut dyn FnMut() -> u32) -> u32 {
        let Some(w) = self.ecs_mut() else { return 0 };
        let e = w.spawn_fangs(mw, at, yaw, warmup, owner);
        w.id_for(e, alloc)
    }

    /// `EcsWorld::spawn_cloud`: a potion cloud at `at`, `spec` built by the caller from `CloudSpec::new` and the fields it changes.
    pub fn spawn_cloud(&mut self, mw: &MobWorld<'_>, at: [f64; 3], spec: CloudSpec, alloc: &mut dyn FnMut() -> u32) -> u32 {
        let Some(w) = self.ecs_mut() else { return 0 };
        let e = w.spawn_cloud(mw, at, spec);
        w.id_for(e, alloc)
    }

}
