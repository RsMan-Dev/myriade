//! The dragons and the fight on the host side: each call takes the network id of its entity and answers through the ECS (`EcsWorld`).
use super::level::EcsLevel;
use crate::mobs::MobWorld;
use horizon_core::geometry::Aabb;
use horizon_ecs_proto::bridge::dragon::{CrystalView, DragonView};
use horizon_ecs_proto::dragon::{
    fight::{FightSave, FightView},
    hurt::{Blow, Hurt},
};
use horizon_ecs_proto::spawn_sheet::SpawnSheet;
use horizon_ecs_proto::targeting::Who;

impl EcsLevel {
    /// `EcsWorld::summon_dragon`: a dragon at `at`; its network id (0 when the level is inert).
    pub fn summon_dragon(&mut self, mw: &MobWorld<'_>, at: [f64; 3], yaw: f32, seed: i64, phase: u8, sheet: &SpawnSheet, alloc: &mut dyn FnMut() -> u32) -> u32 {
        let Some(w) = self.ecs_mut() else { return 0 };
        let e = w.summon_dragon(mw, at, yaw, seed, phase, sheet);
        w.id_for(e, alloc)
    }

    /// `EcsWorld::dragon_views`: every dragon of the level with its network id.
    pub fn dragon_views(&self) -> Vec<(u32, DragonView)> {
        let Some(w) = self.ecs() else { return Vec::new() };
        w.dragon_ids().into_iter().filter_map(|e| Some((self.id_of(e)?, w.dragon_view(e)?))).collect()
    }

    /// `EcsWorld::crystal_views`: every spike crystal of the level with its network id.
    pub fn crystal_views(&self) -> Vec<(u32, CrystalView)> {
        let Some(w) = self.ecs() else { return Vec::new() };
        w.crystal_views().into_iter().filter_map(|(e, v)| Some((self.id_of(e)?, v))).collect()
    }

    /// `EcsWorld::dragon_boxes` of dragon `id`; empty when it is not a dragon.
    pub fn dragon_boxes(&self, id: u32) -> Vec<(usize, Aabb)> {
        let (Some(e), Some(w)) = (self.entity_of(id), self.ecs()) else { return Vec::new() };
        w.dragon_boxes(e)
    }

    /// `EcsWorld::set_dragon_phase` of dragon `id`; false when it is not a dragon.
    pub fn set_dragon_phase(&mut self, mw: &MobWorld<'_>, id: u32, phase: i32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(w) = self.ecs_mut() else { return false };
        w.set_dragon_phase(mw, e, phase)
    }

    /// `EcsWorld::set_dragon_target` of dragon `id`: target `target` (a network id: an entity, else a player in the level, else refused,
    /// nothing written). None clears it. False when nothing was written.
    pub fn set_dragon_target(&mut self, id: u32, target: Option<u32>) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let who = match target {
            None => None,
            Some(t) => match self.entity_of(t) {
                Some(te) => Some(Who::Entity(te)),
                None if self.ecs().is_some_and(|w| w.player_online(t)) => Some(Who::Player(t)),
                None => return false,
            },
        };
        let Some(w) = self.ecs_mut() else { return false };
        w.set_dragon_target(e, who)
    }

    /// `EcsWorld::hurt_dragon` of dragon `id`; refused when it is not there.
    pub fn hurt_dragon(&mut self, mw: &MobWorld<'_>, id: u32, b: Blow) -> Hurt {
        let Some(e) = self.entity_of(id) else { return Hurt::Refused };
        let Some(w) = self.ecs_mut() else { return Hurt::Refused };
        w.hurt_dragon(mw, e, b)
    }

    /// `EnderDragon.knockBack` of dragon `id` on the targets `who`; nothing when it is not there.
    pub fn dragon_wing(&mut self, id: u32, who: &[Who]) {
        let Some(e) = self.entity_of(id) else { return };
        let Some(w) = self.ecs_mut() else { return };
        horizon_ecs_proto::dragon::hurt::wing_rule(w.world_mut(), e, who);
    }

    /// `EnderDragon.hurt(ServerLevel, List)` of dragon `id` on the targets `who`; nothing when it is not there.
    pub fn dragon_head(&mut self, id: u32, who: &[Who]) {
        let Some(e) = self.entity_of(id) else { return };
        let Some(w) = self.ecs_mut() else { return };
        horizon_ecs_proto::dragon::hurt::head_rule(w.world_mut(), e, who);
    }

    /// `LastHurtBy` of entity `id`: its last attacker `who` at tick `at` (`setLastHurtByMob`); nothing when it is not there.
    pub fn set_last_hurt_by(&mut self, id: u32, who: Who, at: u64) {
        let Some(e) = self.entity_of(id) else { return };
        let Some(w) = self.ecs_mut() else { return };
        w.world_mut().entity_mut(e).insert(horizon_ecs_proto::cow::LastHurtBy { who, at });
    }

    /// `EcsWorld::blast_dragon` of dragon `id`: the blast's hits, then its push.
    pub fn blast_dragon(&mut self, mw: &MobWorld<'_>, id: u32, center: [f64; 3], radius: f32, exposure: &dyn Fn(Aabb) -> f32, by: Option<u32>) -> Option<[f64; 3]> {
        let e = self.entity_of(id)?;
        self.ecs_mut()?.blast_dragon(mw, e, center, radius, exposure, by)
    }

    /// `EcsWorld::dragon_push` of dragon `id`: the push alone, no damage.
    pub fn dragon_push(&mut self, mw: &MobWorld<'_>, id: u32, center: [f64; 3], radius: f32, exposure: &dyn Fn(Aabb) -> f32) -> Option<[f64; 3]> {
        let e = self.entity_of(id)?;
        self.ecs_mut()?.dragon_push(mw, e, center, radius, exposure)
    }

    /// `EcsWorld::fight_load`: the fight of the saved world; `mw` is not needed to load it.
    pub fn fight_load(&mut self, _mw: &MobWorld<'_>, seed: i64, saved: FightSave) {
        let Some(w) = self.ecs_mut() else { return };
        w.fight_load(seed, saved);
    }

    /// `EcsWorld::fight_view`: the fight as the host reads it (its dragon is an entity, not an id).
    pub fn fight_view(&self) -> FightView {
        self.ecs().map_or_else(FightView::default, |w| w.fight_view())
    }

    /// `EcsWorld::place_crystal`: a spike crystal at `at`; its network id (0 when the level is inert).
    pub fn place_crystal(&mut self, mw: &MobWorld<'_>, at: [f64; 3], show_bottom: bool, alloc: &mut dyn FnMut() -> u32) -> u32 {
        let Some(w) = self.ecs_mut() else { return 0 };
        let e = w.place_crystal(mw, at, show_bottom);
        w.id_for(e, alloc)
    }

    /// `EcsWorld::hit_crystal` of crystal `id`; false when it is not there or refuses the blow.
    pub fn hit_crystal(&mut self, mw: &MobWorld<'_>, id: u32, by: Option<u32>, fire: bool, creative: bool) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(w) = self.ecs_mut() else { return false };
        w.hit_crystal(mw, e, by, fire, creative)
    }

    /// `EcsWorld::destroy_crystal` of crystal `id`.
    pub fn destroy_crystal(&mut self, mw: &MobWorld<'_>, id: u32, by: Option<u32>) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(w) = self.ecs_mut() {
            w.destroy_crystal(mw, e, by);
        }
    }

    /// `EcsWorld::set_exit_portal`: the exit portal's origin.
    pub fn set_exit_portal(&mut self, origin: [i32; 3]) {
        if let Some(w) = self.ecs_mut() {
            w.set_exit_portal(origin);
        }
    }
}
