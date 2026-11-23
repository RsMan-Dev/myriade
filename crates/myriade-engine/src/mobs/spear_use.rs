use crate::spear::{Kinetic, REACH, hits_in_reach};
use horizon_core::geometry::{Aabb, Vec3, trig::Mth};


















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



















/// `MeleeAttack.isHoldingUsableNonMeleeWeapon`: a piglin holding a kinetic
/// weapon (`canUseNonMeleeWeapon`) never melees; its spear behaviours attack.
#[derive(Clone, Debug)]
pub struct NoMeleeWithSpear<G>(pub G);

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
