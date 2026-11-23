//! `RandomStrollGoal` and its subclasses (`WaterAvoidingRandomStrollGoal`,
//! `RandomSwimmingGoal`, `WaterAvoidingRandomFlyingGoal`,
//! `Parrot.ParrotWanderGoal`) and `RandomStroll.fly` for an allay's brain
//! behaviour.
use super::*;

/// RandomStrollGoal / WaterAvoidingRandomStrollGoal.
#[derive(Clone, Debug)]
pub struct Stroll {
    pub speed: f64,
    pub interval: i32,
    pub check_no_action: bool,
    /// WaterAvoidingRandomStrollGoal probability of a plain stroll.
    pub water_avoiding: Option<f32>,
    /// RandomSwimmingGoal: BehaviorUtils.getRandomSwimmablePos.
    pub swim: bool,
    /// WaterAvoidingRandomFlyingGoal: a hover spot, else an air or water pos.
    pub flying: bool,
    /// `RandomStroll.fly` (`AllayAi`): an air or water spot within 10 by 7,
    /// 2 below, with no hover spot tried first.
    pub fly: bool,
    /// `Parrot.ParrotWanderGoal`: to a tree, else as `flying`.
    pub parrot: bool,
    /// `setFlags(MOVE, LOOK)`, which only `Guardian.registerGoals` adds.
    pub look: bool,
    /// `RandomStrollGoal.forceTrigger`: the next `canUse` skips the interval's
    /// draw (see `Goal::trigger`).
    pub force: bool,
    wanted: [f64; 3],label: Option<&'static str>,
}
impl Stroll {
    /// `RandomStrollGoal.<init>` (interval 120, checks the no-action time).
    pub fn new(speed: f64) -> Self {
        Self {
            speed,
            interval: 120,
            check_no_action: true,
            water_avoiding: None,
            swim: false,
            flying: false,
            fly: false,
            parrot: false,
            look: false,
            force: false,
            wanted: [0.0; 3],
            position: None,
            gate: None,
            label: None,
        }
    }
    /// The goal's class name, for debugging.
    pub fn named(mut self, label: &'static str) -> Self {
        self.label = Some(label);
        self
    }
    pub fn avoiding_water(speed: f64) -> Self {
        Self {
            water_avoiding: Some(0.001),
            ..Self::new(speed)
        }
    }
    pub fn interval(mut self, i: i32) -> Self {
        self.interval = i;
        self
    }
    /// WaterAvoidingRandomFlyingGoal (a nearby air position).
    pub fn flying(speed: f64) -> Self {
        Self {
            flying: true,
            ..Self::new(speed)
        }
    }
    /// `RandomStroll.fly(speed)`: `getTargetFlyPos`, an air or water spot
    /// within 10 by 7 (`AirAndWaterRandomPos.getPos(body, 10, 7, -2, ...)`).
    pub fn fly(speed: f64) -> Self {
        Self {
            fly: true,
            ..Self::new(speed)
        }
    }
    /// `setFlags(MOVE, LOOK)`: the stroll keeps the head too (the guardian's).
    pub fn looking(mut self) -> Self {
        self.look = true;
        self
    }
    /// `Parrot.ParrotWanderGoal(speed)`, a WaterAvoidingRandomFlyingGoal.
    pub fn parrot(speed: f64) -> Self {
        Self {
            water_avoiding: Some(0.001),
            parrot: true,
            ..Self::new(speed)
        }
    }
    /// RandomSwimmingGoal(speed, interval)
    pub fn swimming(speed: f64, interval: i32) -> Self {
        Self {
            swim: true,
            ..Self::new(speed).interval(interval)
        }
    }
}
impl Goal for Stroll {
    goal_clone!();
    fn name(&self) -> &'static str {
        if let Some(label) = self.label {
            label
        } else if self.parrot {
            "ParrotWander"
        } else if self.water_avoiding.is_some() {
            "Stroll"
        } else if self.swim {
            "RandomSwimming"
        } else {
            "RandomStroll"
        }
    }
    fn flags(&self) -> u8 {
        if self.look { MOVE | LOOK } else { MOVE }
    }
    /// `RandomStrollGoal.trigger`.
    fn trigger(&mut self) {
        self.force = true;
    }
}
