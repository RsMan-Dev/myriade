//! The AI's reads by network id (`u32`): the id is turned into its entity by the level's table, then read from the ECS world
//! (`host_read_ai`). An absent entity, and an inert level, read as None.
use super::level::EcsLevel;
use bevy_ecs::entity::Entity;
use horizon_ecs_proto::{
    bridge::{host::EcsWorld, ids::id_of},
    targeting::Who,
};

/// The ECS's `Who` as the server's: a player is `Player`, an entity is `Mob` by its id (None while the entity has no id).
fn mob_who(w: &EcsWorld, who: Who) -> Option<crate::mobs::Who> {
    match who {
        Who::Player(_) => Some(crate::mobs::Who::Player),
        Who::Entity(e) => id_of(w.world(), e).map(crate::mobs::Who::Mob),
    }
}

impl EcsLevel {
    /// Runs `f` on the world and on the entity of `id`; None when the level is inert or the id is unknown.
    fn by_id<R>(&self, id: u32, f: impl FnOnce(&EcsWorld, Entity) -> R) -> Option<R> {
        let w = self.ecs()?;
        Some(f(w, self.entity_of(id)?))
    }

    pub fn ai_target(&self, id: u32) -> Option<crate::mobs::Who> {
        self.by_id(id, |w, e| w.ai_target(e).and_then(|who| mob_who(w, who))).flatten()
    }
    /// The id of the player `id` targets (`Target.who` = `Who::Player`); None for another target or none.
    pub fn ai_target_player(&self, id: u32) -> Option<u32> {
        self.by_id(id, |w, e| match w.ai_target(e) {
            Some(horizon_ecs_proto::targeting::Who::Player(p)) => Some(p),
            _ => None,
        })
        .flatten()
    }
    /// Whether the entity `id` is driven by a brain rather than goals.
    pub fn has_brain(&self, id: u32) -> bool {
        self.by_id(id, |w, e| w.has_brain(e)).unwrap_or(false)
    }
    /// `NeutralMob.isAngry` of the entity `id`.
    pub fn is_angry(&self, id: u32) -> bool {
        self.by_id(id, |w, e| w.is_angry(e)).unwrap_or(false)
    }
    pub fn last_hurt_by(&self, id: u32) -> Option<crate::mobs::Who> {
        self.by_id(id, |w, e| w.last_hurt_by(e).and_then(|who| mob_who(w, who))).flatten()
    }
    pub fn vex_owner(&self, id: u32) -> Option<u32> {
        self.by_id(id, |w, e| w.vex_owner(e).and_then(|o| id_of(w.world(), o))).flatten()
    }
    /// `Raider.getCurrentRaid`: None for a non-raider, `Some(None)` for a raider in no raid.
    pub fn raider_raid(&self, id: u32) -> Option<Option<i32>> {
        self.by_id(id, |w, e| w.raider_raid(e)).flatten()
    }
    /// The living raiders of a raid: id, wave, health.
    pub fn raid_members(&self, raid: i32) -> Vec<(u32, i32, f32)> {
        let Some(w) = self.ecs() else { return Vec::new() };
        w.raid_members(raid).into_iter().filter_map(|(e, wave, h)| id_of(w.world(), e).map(|i| (i, wave, h))).collect()
    }
    /// The sections holding a claimed village point (`EcsWorld::village_sections`).
    pub fn village_sections(&self) -> Vec<[i32; 3]> {
        self.ecs().map_or_else(Vec::new, |w| w.village_sections())
    }
    /// The claimed village points (`EcsWorld::village_points`).
    pub fn village_points(&self) -> Vec<[i32; 3]> {
        self.ecs().map_or_else(Vec::new, |w| w.village_points())
    }
    /// The coat of the creature `id` (`Variant`), 0 when unset.
    pub fn variant_of(&self, id: u32) -> Option<u8> {
        self.by_id(id, |w, e| w.variant_of(e))
    }
    pub fn aggressive(&self, id: u32) -> Option<bool> {
        self.by_id(id, |w, e| w.aggressive(e))
    }
    pub fn ai_paused(&self, id: u32) -> Option<bool> {
        self.by_id(id, |w, e| w.ai_paused(e))
    }
    pub fn wanted(&self, id: u32) -> Option<[f64; 3]> {
        self.by_id(id, |w, e| w.wanted(e)).flatten()
    }
    pub fn move_speed_modifier(&self, id: u32) -> Option<f64> {
        self.by_id(id, |w, e| w.move_speed_modifier(e)).flatten()
    }
    pub fn nav_is_done(&self, id: u32) -> Option<bool> {
        self.by_id(id, |w, e| w.nav_is_done(e))
    }
    pub fn nav_speed_modifier(&self, id: u32) -> Option<f64> {
        self.by_id(id, |w, e| w.nav_speed_modifier(e)).flatten()
    }
    pub fn has_restriction(&self, id: u32) -> Option<bool> {
        self.by_id(id, |w, e| w.has_restriction(e))
    }
    pub fn restrict_center(&self, id: u32) -> Option<[i32; 3]> {
        self.by_id(id, |w, e| w.restrict_center(e)).flatten()
    }
    pub fn restrict_radius(&self, id: u32) -> Option<i32> {
        self.by_id(id, |w, e| w.restrict_radius(e)).flatten()
    }
    pub fn is_within_restriction(&self, id: u32, at: [f64; 3]) -> Option<bool> {
        self.by_id(id, |w, e| w.is_within_restriction(e, at))
    }
    pub fn sleeping_pos(&self, id: u32) -> Option<[i32; 3]> {
        self.by_id(id, |w, e| w.sleeping_pos(e)).flatten()
    }
    pub fn can_pick_up_loot(&self, id: u32) -> Option<bool> {
        self.by_id(id, |w, e| w.can_pick_up_loot(e)).flatten()
    }
    pub fn requires_custom_persistence(&self, id: u32) -> Option<bool> {
        self.by_id(id, |w, e| w.requires_custom_persistence(e))
    }
}
