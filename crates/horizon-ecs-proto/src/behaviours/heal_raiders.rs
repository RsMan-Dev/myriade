//! `Witch.NearestHealableRaiderTargetGoal`: not a module, a mode of `nearest_target` (`Params::HEAL_RAIDERS` with `types` set by the witch's file: the raiders but the witch, only in
//! an active raid, one draw in two per ask) and the wait it starts: 200 ticks (`HealWait`, on the witch only) during which neither this goal nor the
//! nearest player (`Params::WITCH_PLAYER`) asks. The wait is a deadline stamp read when asked, never ticked.
use super::nearest_target::Params;
use bevy_ecs::prelude::*;

#[derive(Component, Clone, Copy, Debug)]
pub struct HealWait(pub u64);
const WAIT: u64 = 200;
impl Params {
    /// `HEAL_RAIDERS` with the raiders it heals (the witch's file names them).
    pub const HEAL_RAIDERS: Self = Self { interval: 0, heal: true, ..Self::DEFAULT };
    pub const WITCH_PLAYER: Self = Self { players: true, after_heal: true, ..Self::DEFAULT };
}
pub fn waiting(w: &World, e: Entity) -> bool {
    w.get::<HealWait>(e).is_some_and(|h| h.0 > crate::cow::now_of(w, e))
}
pub fn wait(w: &mut World, e: Entity) {
    let until = crate::cow::now_of(w, e) + WAIT;
    w.entity_mut(e).insert(HealWait(until));
}
