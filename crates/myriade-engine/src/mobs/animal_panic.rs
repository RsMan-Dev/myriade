//! `AnimalPanic`, the panic of the mobs whose vanilla AI is a brain (the
//! frog, goat, allay, camel, nautilus and armadillo), and the Manhattan
//! water search it shares with `PanicGoal`.
use super::*;
use crate::mobs::armadillo;

/// `AnimalPanic`: a panic that runs `PANIC_MIN_DURATION` to
/// `PANIC_MAX_DURATION` ticks and picks a new spot to flee to every time the
/// navigation is done, where `PanicGoal` gives up as soon as it gets there.
#[derive(Clone, Debug)]
pub struct AnimalPanic {
    pub speed: f64,
    /// `ArmadilloAi.ArmadilloPanic`: only `#panic_environmental_causes`.
    pub environmental: bool,
    /// `Behavior`'s `endTimestamp`, on the mob's age.
    end: i64,
}
impl AnimalPanic {
    pub fn new(speed: f64) -> Self {
        Self {
            speed,
            environmental: false,
            end: 0,
        }
    }
}
impl Goal for AnimalPanic {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
    fn name(&self) -> &'static str {
        "Panic"
    }
    /// A brain behaviour ticks every tick; a goal only every other one.
    fn every_tick(&self) -> bool {
        true
    }
}




