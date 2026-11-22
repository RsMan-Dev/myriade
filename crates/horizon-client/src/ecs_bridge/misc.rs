//! The lightning, end crystals, evoker fangs and area clouds on the level: each spawn returns its network id, linked on the spot (0 when the
//! level is inert). What the ECS queued for them is taken with `take_misc_events`.
use super::level::EcsLevel;
use crate::mobs::MobWorld;
use horizon_ecs_proto::{effect_cloud::CloudSpec, end_crystal::Blow, lightning::BoltLevel, targeting::Who};

const END_CRYSTAL: &str = "minecraft:end_crystal";

impl EcsLevel {
    /// `EcsWorld::strike`: a bolt at `at`; `cosmetic` for a skeleton trap's; `level` the host's blocks for the strike.
    pub fn strike(&mut self, mw: &MobWorld<'_>, at: [f64; 3], cosmetic: bool, level: Option<Box<dyn BoltLevel>>, alloc: &mut dyn FnMut() -> u32) -> u32 {
        let Some(w) = self.ecs_mut() else { return 0 };
        let e = w.strike(mw, at, cosmetic, level);
        w.id_for(e, alloc)
    }

    /// `EcsWorld::set_next_bolt_cause`: the player whose trident summons the next bolt.
    pub fn set_next_bolt_cause(&mut self, player: Option<u32>) {
        if let Some(w) = self.ecs_mut() {
            w.set_next_bolt_cause(player);
        }
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

    /// The `minecraft:end_crystal` ids of the level, in id order (`species_ids`).
    pub fn crystal_ids(&self) -> Vec<u32> {
        self.species_ids(END_CRYSTAL)
    }

    /// The crystal `id`'s `ShowBottom`, beam and shield; None for an unknown id.
    pub fn crystal_state(&self, id: u32) -> Option<(bool, Option<[i32; 3]>, bool)> {
        let e = self.entity_of(id)?;
        self.ecs()?.crystal_state(e)
    }

    /// `EcsWorld::set_crystal_beam` on the crystal `id`.
    pub fn set_crystal_beam(&mut self, id: u32, to: Option<[i32; 3]>) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(w) = self.ecs_mut() {
            w.set_crystal_beam(e, to);
        }
    }

    /// `EcsWorld::shrink_breath_cloud` on the breath cloud `id`.
    pub fn shrink_breath_cloud(&mut self, id: u32, by: f32) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(w) = self.ecs_mut() {
            w.shrink_breath_cloud(e, by);
        }
    }

    /// `EcsWorld::set_end_fire`: the fire under the crystals, on in the End's fight.
    pub fn set_end_fire(&mut self, on: bool) {
        if let Some(w) = self.ecs_mut() {
            w.set_end_fire(on);
        }
    }

    /// `EcsWorld::set_crystal_shield` on the crystal `id`.
    pub fn set_crystal_shield(&mut self, id: u32, on: bool) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(w) = self.ecs_mut() {
            w.set_crystal_shield(e, on);
        }
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

    /// `EcsWorld::spawn_flame`: the dragon's breath cloud at `at`; its network id (0 when the level is inert).
    pub fn spawn_flame(&mut self, mw: &MobWorld<'_>, at: [f64; 3], alloc: &mut dyn FnMut() -> u32) -> u32 {
        let Some(w) = self.ecs_mut() else { return 0 };
        let e = w.spawn_flame(mw, at);
        w.id_for(e, alloc)
    }

    /// `EcsWorld::end_flames`: the dragon's flame clouds go.
    pub fn end_flames(&mut self, mw: &MobWorld<'_>) {
        if let Some(w) = self.ecs_mut() {
            w.end_flames(mw);
        }
    }

    /// `EcsWorld::place_cushion`: a cushion on `block` at `at`; its network id, linked on the spot (0 when inert or refused).
    pub fn place_cushion(&mut self, mw: &MobWorld<'_>, block: [i32; 3], at: [f64; 3], color: u8, yaw: f32, name: Option<Box<horizon_core::nbt::Tag>>, alloc: &mut dyn FnMut() -> u32) -> u32 {
        let Some(w) = self.ecs_mut() else { return 0 };
        let Some(e) = w.place_cushion(mw, block, at, color, yaw, name) else { return 0 };
        w.id_for(e, alloc)
    }

    /// `EcsWorld::hurt_cushion` on the cushion of network id `id`; false when the level is inert or the id is unknown.
    pub fn hurt_cushion(&mut self, mw: &MobWorld<'_>, id: u32, source: horizon_ecs_proto::species::cushion::Source) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(w) = self.ecs_mut() else { return false };
        w.hurt_cushion(mw, e, source)
    }

    /// `EcsWorld::spawn_ominous`: an ominous item spawner at `at`, dropping `item` after `after` ticks; its network id (0 when inert).
    pub fn spawn_ominous(&mut self, mw: &MobWorld<'_>, at: [f64; 3], item: horizon_client::items::ItemStack, after: u32, alloc: &mut dyn FnMut() -> u32) -> u32 {
        let Some(w) = self.ecs_mut() else { return 0 };
        let e = w.spawn_ominous(mw, at, item, after);
        w.id_for(e, alloc)
    }
}
