//! The mob that last hurt a mob (`LivingEntity.lastHurtByMob`): its record in
//! `Ai`, its end in `baseTick`, and what a hit or a stop being angry does to it.
use super::ai::Ai;
use super::{Damage, DamageKind, MobWorld, Species, Who};

// `LivingEntity.lastHurtByMob` is a lazy `EntityReference`: a record read back
// from `last_hurt_by_mob` stays the bare UUID (`Ai::last_hurt_by_ref`) until the
// game resolves it into `last_hurt_by`. `getLastHurtByMob` answers nothing for
// it and `baseTick` does not end it; its timestamp keeps aging.
impl Ai {
    /// `LivingEntity.setLastHurtByMob(who)`: the new record replaces a
    /// reference read back that was not resolved.
    pub fn hurt_by(&mut self, who: Who, tick: i32) {
        self.last_hurt_by = Some(who);
        self.last_hurt_by_ref = None;
        self.last_hurt_by_time = tick;
    }
    /// `LivingEntity.setLastHurtByMob(null)`, which also stamps the time.
    pub fn forget_hurt_by(&mut self, tick: i32) {
        self.last_hurt_by = None;
        self.last_hurt_by_ref = None;
        self.last_hurt_by_time = tick;
    }
}
