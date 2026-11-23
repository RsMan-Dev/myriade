//! The hurt state of a creature by id (`P bridge/host_combat.rs`): damage window, `invulnerableTime`, attackers, combat tracker,
//! fire, fuse, glowing, enderman teleports, one random seed; and the teams of the board (`P bridge/teams.rs`).
use super::level::EcsLevel;
use crate::mobs::MobWorld;
use horizon_ecs_proto::{
    bridge::{host::EcsWorld, ids::id_of, teams},
    targeting::Who,
};

fn mob_who(w: &EcsWorld, who: Who) -> Option<crate::mobs::Who> {
    match who {
        Who::Player(_) => Some(crate::mobs::Who::Player),
        Who::Entity(e) => id_of(w.world(), e).map(crate::mobs::Who::Mob),
    }
}

/// A hit waiting in the ECS's damage queue, by network id (the target is linked on the way out).
#[derive(Clone, Copy, Debug)]
pub struct PendingHit {
    pub target: u32,
    pub amount: f32,
    pub damage: &'static str,
}

impl EcsLevel {
    /// The hits queued since the last tick (before `apply_damage` takes them), emptied: what the host's attacks and blasts wrote. A target
    /// that has no network id is left out.
    pub fn drain_damage_events(&mut self) -> Vec<PendingHit> {
        let Some(w) = self.ecs_mut() else { return Vec::new() };
        let hits = std::mem::take(&mut w.world_mut().resource_mut::<horizon_ecs_proto::cow::DamageQueue>().0);
        hits.into_iter()
            .filter_map(|h| Some(PendingHit { target: id_of(w.world(), h.target)?, amount: h.amount, damage: h.damage }))
            .collect()
    }
    pub fn damage_cooldown(&self, id: u32) -> Option<i32> {
        self.ecs()?.damage_cooldown(self.entity_of(id)?)
    }
    pub fn set_damage_cooldown(&mut self, id: u32, ticks: i32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_damage_cooldown(e, ticks))
    }
    pub fn invulnerable_time(&self, id: u32) -> Option<i32> {
        self.ecs()?.invulnerable_time(self.entity_of(id)?)
    }
    pub fn set_invulnerable_time(&mut self, id: u32, ticks: i32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_invulnerable_time(e, ticks))
    }
    /// `getLastHurtByMob` while remembered; None when there is none or the attacker has no id.
    pub fn recent_attacker(&self, id: u32) -> Option<crate::mobs::Who> {
        let w = self.ecs()?;
        mob_who(w, w.recent_attacker(self.entity_of(id)?)?)
    }
    /// The luck and main-hand stack of the player `player` that hurts `e` (what its death reads of the killer); nothing when inert.
    pub fn note_killer(&mut self, e: bevy_ecs::entity::Entity, player: u32, luck: f32, weapon: Option<crate::items::ItemStack>) {
        if let Some(w) = self.ecs_mut() {
            w.note_killer(e, player, luck, weapon);
        }
    }
    pub fn last_hurt_by_player(&self, id: u32) -> i32 {
        let Some(e) = self.entity_of(id) else { return 0 };
        self.ecs().map_or(0, |w| w.last_hurt_by_player(e))
    }
    /// `LivingEntity.setLastHurtByPlayer` on the entity `id` (`credit::record`); nothing when inert.
    pub fn record_credit(&mut self, id: u32, by: Option<horizon_ecs_proto::targeting::Who>) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(w) = self.ecs_mut() {
            w.record_credit(e, by);
        }
    }
    /// The uuid of the player the entity `id` remembers as its last hurter, while remembered.
    pub fn credited_uuid(&self, id: u32) -> Option<u128> {
        self.ecs()?.credited_uuid(self.entity_of(id)?).map(crate::lan::playerdata::ints_uuid)
    }
    /// `Animal.setInLove(player)` on the entity `id`: the player's ECS id, None for no player.
    pub fn set_love_cause(&mut self, id: u32, player: Option<u32>) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(w) = self.ecs_mut() {
            w.set_love_cause(e, player);
        }
    }
    pub fn set_ignited(&mut self, id: u32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_ignited(e))
    }
    pub fn extinguish(&mut self, id: u32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.extinguish(e))
    }
    pub fn set_glowing(&mut self, id: u32, on: bool) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_glowing(e, on))
    }
    pub fn repeated_teleport(&mut self, mw: &MobWorld<'_>, id: u32, tries: u32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.repeated_teleport(mw, e, tries))
    }
    pub fn draw_seed(&mut self, id: u32) -> Option<i64> {
        let e = self.entity_of(id)?;
        self.ecs_mut()?.draw_seed(e)
    }
    pub fn add_spawn_tag(&mut self, id: u32, name: &str) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.add_spawn_tag(e, name))
    }
    pub fn set_team(&mut self, name: &str, members: &[Who]) {
        if let Some(w) = self.ecs_mut() {
            teams::set_team(w.world_mut(), name, members);
        }
    }
    pub fn set_team_friendly_fire(&mut self, name: &str, on: bool) {
        if let Some(w) = self.ecs_mut() {
            teams::set_friendly_fire(w.world_mut(), name, on);
        }
    }
    pub fn clear_team(&mut self, name: &str) {
        if let Some(w) = self.ecs_mut() {
            teams::clear_team(w.world_mut(), name);
        }
    }
}
