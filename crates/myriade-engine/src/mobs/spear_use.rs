//! Mobs using a spear (`KineticWeapon`): `startUsingItem` of the main hand,
//! the stab of every tick of the use (`KineticWeapon.damageEntities`) and
//! what makes a mob use it: `SpearUseGoal` of zombies and zombified piglins,
//! and the fight activity's `SpearApproach`, `SpearAttack` and `SpearRetreat`
//! of piglins.
//! A mob's `using_item` counts the ticks of the use up from 1, which the
//! living entity flags tell the clients (`mob_meta`), so the arm and the
//! spear are drawn charging for them as well.
use super::{
    Damage, DamageKind, MobEvent, MobWorld, Species, Who,
    brain::Timed,
    gear,
    goals::{Cx, Goal, LOOK, MOVE, goal_clone, reduced},
};
use crate::spear::{Kinetic, REACH, hits_in_reach};
use horizon_core::geometry::{Aabb, Vec3, trig::Mth};








/// What a stab can meet: who, where and how fast it moves per second.
struct Candidate {
    who: Who,
    bounds: Aabb,
    speed: Vec3,
}










/// `SpearUseGoal.SpearUseState`: where a charge is.
#[derive(Clone, Debug)]
struct SpearUseState {
    engage_time: i32,
    fleeing_time: i32,
    away_pos: Option<[f64; 3]>,
    done: bool,
}
impl SpearUseState {
    /// `SpearUseState.<init>`: not engaged and not fleeing yet.
    fn new() -> Self {
        Self {
            engage_time: -1,
            fleeing_time: -1,
            away_pos: None,
            done: false,
        }
    }
    /// `SpearUseState.notEngagedYet`.
    fn not_engaged_yet(&self) -> bool {
        self.engage_time < 0
    }
    /// `SpearUseState.startEngagement`: the spear stays down for `spear_down_time` goal ticks.
    fn start_engagement(&mut self, spear_down_time: i32) {
        self.engage_time = spear_down_time;
    }
    /// `SpearUseState.tickAndCheckEngagement`: whether the time ran out.
    fn tick_and_check_engagement(&mut self) -> bool {
        if self.engage_time > 0 {
            self.engage_time -= 1;
            if self.engage_time == 0 {
                return true;
            }
        }
        false
    }
    /// `SpearUseState.tickAndCheckFleeing`: whether it fled for too long.
    fn tick_and_check_fleeing(&mut self) -> bool {
        if self.fleeing_time > 0 {
            self.fleeing_time += 1;
            if self.fleeing_time > SpearUse::MAX_FLEEING_TIME {
                self.done = true;
                return true;
            }
        }
        false
    }
}

/// `SpearUseGoal`: with a spear in hand and a target, walk up to the
/// approach distance, lower the spear and charge until its use time ends,
/// back off 9 to 11 blocks and start over.
#[derive(Clone, Debug)]
pub struct SpearUse {
    speed_modifier_when_charging: f64,
    speed_modifier_when_repositioning: f64,
    approach_distance_sq: f32,
    target_in_range_radius_sq: f32,
    state: Option<SpearUseState>,
}
impl SpearUse {
    /// `SpearUseGoal.MAX_FLEEING_TIME`: `reducedTickDelay(100)`.
    const MAX_FLEEING_TIME: i32 = 50;

    pub fn new(
        speed_modifier_when_charging: f64,
        speed_modifier_when_repositioning: f64,
        approach_distance: f32,
        target_in_range_radius: f32,
    ) -> Self {
        Self {
            speed_modifier_when_charging,
            speed_modifier_when_repositioning,
            approach_distance_sq: approach_distance * approach_distance,
            target_in_range_radius_sq: target_in_range_radius * target_in_range_radius,
            state: None,
        }
    }


}
impl Goal for SpearUse {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
}

/// `SpearAttack.SpearStatus`: where a piglin is in its charge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpearStatus {
    Approach,
    Charging,
    Retreat,
}

/// The spear memories of a piglin's brain (`SPEAR_STATUS` and the like). None
/// of them is saved: they have no codec.
#[derive(Clone, Debug, Default)]
pub struct Memories {
    /// `SPEAR_STATUS`.
    pub status: Option<SpearStatus>,
    /// `SPEAR_ENGAGE_TIME`: ticks of the charge left.
    pub engage_time: Option<i32>,
    /// `SPEAR_CHARGE_POSITION`: where the charge goes to reposition.
    pub charge_position: Option<[f64; 3]>,
    /// `SPEAR_FLEEING_TIME`: ticks spent backing off.
    pub fleeing_time: Option<i32>,
    /// `SPEAR_FLEEING_POSITION`: where it backs off to.
    pub fleeing_position: Option<[f64; 3]>,
}


















/// The spear behaviours of a piglin's fight activity, `SpearApproach`,
/// `SpearAttack` and `SpearRetreat`: each starts on the `SPEAR_STATUS` the
/// one before it leaves. The goal runs them in the order of `Brain.tick`:
/// the ones not running try to start, then the running ones tick or stop.
/// Fight is the activity while the piglin has a target; a piglin with a
/// spear never melees (`NoMeleeWithSpear`).
#[derive(Clone, Debug)]
pub struct PiglinSpear {
    approach: Timed,
    attack: Timed,
    retreat: Timed,
}
impl PiglinSpear {
    /// `new SpearApproach(1.0, 10.0F)`: the squared approach distance.
    const APPROACH_DISTANCE_SQ: f32 = 10.0 * 10.0;
    /// `new SpearAttack(1.0, 1.0, 2.0F)`: the speed charging and the squared
    /// radius around the target that ends the run at it.
    const SPEED_MODIFIER_WHEN_CHARGING: f64 = 1.0;
    const TARGET_IN_RANGE_RADIUS_SQ: f32 = 2.0 * 2.0;
    /// The `speedModifierWhenRepositioning` of the three behaviours.
    const SPEED_MODIFIER_WHEN_REPOSITIONING: f64 = 1.0;

    pub fn new() -> Self {
        Self {
            // `Behavior(Map)`: a duration of 60 ticks (which `timedOut` ignores).
            approach: Timed::new(60, 60),
            attack: Timed::new(60, 60),
            // `super(Map, 100)`: a timeout of 100 ticks.
            retreat: Timed::new(100, 100),
        }
    }

}
impl Default for PiglinSpear {
    fn default() -> Self {
        Self::new()
    }
}
impl Goal for PiglinSpear {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn every_tick(&self) -> bool {
        true
    }
}

/// `MeleeAttack.isHoldingUsableNonMeleeWeapon`: a piglin holding a kinetic
/// weapon (`canUseNonMeleeWeapon`) never melees; its spear behaviours attack.
#[derive(Clone, Debug)]
pub struct NoMeleeWithSpear<G>(pub G);
impl<G: Goal + Clone + 'static> Goal for NoMeleeWithSpear<G> {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.0.flags()
    }
    fn name(&self) -> &'static str {
        self.0.name()
    }
    fn interruptable(&self) -> bool {
        self.0.interruptable()
    }
    fn every_tick(&self) -> bool {
        self.0.every_tick()
    }
}

#[cfg(test)]
mod tests {
    use super::{count_use_tick, is_using_item, start_using_item};
    use horizon_core::geometry::Vec3;


    /// The counters a mod's `living_set_item_use_remaining` leaves a 72000
    /// tick use at (remaining `i32::MIN`, `i32::MAX`, 0 and -5): the next
    /// ticks count on without overflowing, and the use keeps running.
    #[test]
    fn a_use_counter_at_any_value_a_mod_sets_survives_the_next_ticks() {
        for counter in [i32::MAX, 1, 72_001, 72_006, i32::MAX - 1] {
            let mut m = spear_zombie();
            assert!(start_using_item(&mut m, false));
            m.extra.using_item = counter;
            for tick in 1..=3 {
                count_use_tick(&mut m);
                assert!(is_using_item(&m), "{counter} tick {tick}");
                assert_eq!(
                    m.extra.using_item,
                    counter.saturating_add(tick),
                    "{counter} tick {tick}"
                );
            }
        }
    }
}
