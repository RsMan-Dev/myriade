//! The wandering trader (`WanderingTrader`): drinking invisibility at night
//! and milk by day, fleeing monsters, walking to the place it came to
//! visit, staying within its home, trading and leaving after its despawn
//! delay.
use super::{
    MobEvent, MobWorld, Species,
    goals::{
        Avoid, AvoidEntity, Cx, Float, Goal, JUMP, LOOK, LookAtPlayer, MOVE, Panic, Stroll,
        goal_clone,
    },
};
use horizon_core::random::RandomSource;

// Trader state: `point` the wander target, `ints[5]` the despawn delay, `flag2` whether
// a player is trading with it and `using_item` how long it has been
// drinking.


/// `WanderingTraderSpawner`: how long a spawned trader stays.
pub const DESPAWN_DELAY: i32 = 48000;
const DRINK_TICKS: i32 = 32;
const INVISIBILITY: &str = "minecraft:invisibility";

/// `UseItemGoal`: drink a potion of invisibility once it is dark outside,
/// or milk to clear it by day.
#[derive(Clone, Debug)]
struct DrinkItem {
    item: &'static str,
    finish_sound: &'static str,
    dark: bool,
}
impl Goal for DrinkItem {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
    fn every_tick(&self) -> bool {
        true
    }
}
/// `TradeWithPlayerGoal`: stand still while trading.
#[derive(Clone, Debug)]
struct TradeWithPlayer;
impl Goal for TradeWithPlayer {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | JUMP
    }
}
/// `LookAtTradingPlayerGoal`.
#[derive(Clone, Debug)]
struct LookAtTradingPlayer;
impl Goal for LookAtTradingPlayer {
    goal_clone!();
    fn flags(&self) -> u8 {
        LOOK
    }
    fn every_tick(&self) -> bool {
        true
    }
}
/// `WanderingTrader.WanderToPositionGoal`: head for the wander target, in
/// hops of ten blocks while it is far.
#[derive(Clone, Debug)]
struct WanderToPosition {
    stop: f64,
    speed: f64,
}
impl Goal for WanderToPosition {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
}
/// `MoveTowardsRestrictionGoal`, also the guardian's priority-5 goal.
#[derive(Clone, Debug)]
pub(crate) struct MoveTowardsRestriction {
    speed: f64,
    wanted: [f64; 3],
    /// `setFlags(MOVE, LOOK)`, which `Guardian.registerGoals` adds to it.
    look: bool,
}
impl MoveTowardsRestriction {
    pub(crate) fn new(speed: f64) -> Self {
        Self {
            speed,
            wanted: [0.0; 3],
            look: false,
        }
    }
    pub(crate) fn looking(mut self) -> Self {
        self.look = true;
        self
    }
}
impl Goal for MoveTowardsRestriction {
    goal_clone!();
    fn flags(&self) -> u8 {
        if self.look { MOVE | LOOK } else { MOVE }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use horizon_world::dimension_type::DimensionFacts;

    #[test]
    fn despawn_delay_is_two_days() {
        assert_eq!(DESPAWN_DELAY, 2 * 24000);
    }

}

#[cfg(test)]
mod trade_start_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;

}

#[cfg(test)]
mod use_overflow_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;

}

