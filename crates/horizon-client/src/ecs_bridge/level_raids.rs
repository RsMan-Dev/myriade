//! The raids by network id: the host's raid state is written to the ECS as a view, its raiders join by id and are read back by id.
use super::level::EcsLevel;
use horizon_ecs_proto::{bridge::ids::id_of, raid_buffs::{RaidBuff, RaidBuffRules}, raids::RaidView};

/// A living raider of a raid, by network id.
#[derive(Clone, Copy, Debug)]
pub struct RaiderNow {
    pub id: u32,
    pub wave: i32,
    pub leader: bool,
    pub health: f32,
    pub position: [i32; 3],
    pub tick_count: i32,
    pub no_action_time: i32,
    pub ticks_outside: i32,
}

impl EcsLevel {
    /// `EcsWorld::set_raid_view`: what the goals read of raid `id` (None: gone). Nothing when inert.
    pub fn set_raid_view(&mut self, id: i32, view: Option<RaidView>) {
        if let Some(w) = self.ecs_mut() {
            w.set_raid_view(id, view);
        }
    }
    /// `EcsWorld::enlist_raider`: the creature `id` joins raid `raid` in `wave`. False when the id is unknown or the level is inert.
    pub fn enlist_raider(&mut self, id: u32, raid: i32, wave: i32, leader: bool) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(w) = self.ecs_mut() else { return false };
        w.enlist_raider(e, raid, wave, leader);
        true
    }
    /// The living raiders of `raid` (`EcsWorld::raiders_of`).
    pub fn raiders_of(&self, raid: i32) -> Vec<RaiderNow> {
        let Some(w) = self.ecs() else { return Vec::new() };
        w.raiders_of(raid)
            .into_iter()
            .filter_map(|r| id_of(w.world(), r.entity).map(|id| RaiderNow { id, wave: r.wave, leader: r.leader, health: r.health, position: r.position, tick_count: r.tick_count, no_action_time: r.no_action_time, ticks_outside: r.ticks_outside }))
            .collect()
    }
    /// `Raider.setTicksOutsideRaid` on creature `id`.
    pub fn set_ticks_outside_raid(&mut self, id: u32, ticks: i32) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(w) = self.ecs_mut() {
            w.set_ticks_outside_raid(e, ticks);
        }
    }
    /// `Raider.setCurrentRaid(null)` on creature `id`.
    pub fn leave_raid(&mut self, id: u32) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(w) = self.ecs_mut() {
            w.leave_raid(e);
        }
    }
    /// `Raider.applyRaidBuffs` on creature `id`.
    pub fn apply_raid_buffs(&mut self, id: u32, kind: RaidBuff, wave: i32, rules: RaidBuffRules) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(w) = self.ecs_mut() else { return false };
        w.apply_raid_buffs(e, kind, wave, rules)
    }
}
