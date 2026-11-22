//! The raids through the bridge: the host owns the raid state and writes its view (`set_raid_view`), makes its raiders join
//! (`enlist_raider`) and reads them back (`raiders_of`, a query of `Raider`, no scan of the world).
use super::host::EcsWorld;
use crate::raids::RaidView;
use bevy_ecs::entity::Entity;

/// A living raider as the raid reads it.
#[derive(Clone, Copy, Debug)]
pub struct RaiderRead {
    pub entity: Entity,
    pub wave: i32,
    pub leader: bool,
    pub health: f32,
    pub position: [i32; 3],
    /// `Entity.tickCount` (its own time), `LivingEntity.noActionTime` and `Raider.ticksOutsideRaid`.
    pub tick_count: i32,
    pub no_action_time: i32,
    pub ticks_outside: i32,
}

impl EcsWorld {
    /// The host's state of raid `id` as the goals read it (None: the raid is gone). The raiders of the raid are woken when it changes.
    pub fn set_raid_view(&mut self, id: i32, view: Option<RaidView>) {
        crate::raids::set_view(self.world_mut(), id, view);
    }
    /// The creature `e` joins raid `raid` in wave `wave` (`Raid.joinRaid`).
    pub fn enlist_raider(&mut self, e: Entity, raid: i32, wave: i32, leader: bool) {
        crate::raids::enlist_raider(self.world_mut(), e, raid, wave, leader);
    }
    /// The living raiders of raid `raid`.
    pub fn raiders_of(&self, raid: i32) -> Vec<RaiderRead> {
        let w = self.world();
        let Some(q) = w.try_query::<(Entity, &crate::raids::Raider, &crate::cow::Health, &crate::cow::Pos)>() else { return Vec::new() };
        q.iter_manual(w)
            .filter(|(_, r, ..)| r.raid == Some(raid))
            .map(|(entity, r, h, p)| RaiderRead {
                entity,
                wave: r.wave,
                leader: r.leader,
                health: h.0,
                position: [p.0.x.floor() as i32, p.0.y.floor() as i32, p.0.z.floor() as i32],
                tick_count: i32::try_from(crate::cow::now_of(w, entity)).unwrap_or(i32::MAX),
                no_action_time: crate::despawn::idle_for(w, entity).unwrap_or(0),
                ticks_outside: crate::raids::ticks_outside(w, entity),
            })
            .collect()
    }
    /// `Raider.setTicksOutsideRaid`.
    pub fn set_ticks_outside_raid(&mut self, e: Entity, ticks: i32) {
        crate::raids::set_ticks_outside(self.world_mut(), e, ticks);
    }
    /// `Raider.setCurrentRaid(null)`: the raid dropped the raider.
    pub fn leave_raid(&mut self, e: Entity) {
        crate::raids::leave_raid(self.world_mut(), e);
    }
    /// `Raider.applyRaidBuffs` of `kind` for `wave`, the draw taken from the entity's own `Rng`. False when `e` is gone.
    pub fn apply_raid_buffs(&mut self, e: Entity, kind: crate::raid_buffs::RaidBuff, wave: i32, rules: crate::raid_buffs::RaidBuffRules) -> bool {
        crate::raid_buffs::apply(self.world_mut(), e, kind, wave, rules)
    }
}
