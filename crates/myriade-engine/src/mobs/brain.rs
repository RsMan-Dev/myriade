use horizon_core::{
    geometry::Vec3,
    random::{LevelRandom, RandomSource},
};

/// A memory with an expiry (`Brain.setMemoryWithExpiry`): present for the
/// tick it was set and `ttl` more ticks. `Ttl::default()` is absent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ttl(i32);
impl Ttl {
    pub fn set(&mut self, ttl: i32) {
        self.0 = ttl + 1;
    }
    pub fn erase(&mut self) {
        self.0 = 0;
    }
    pub fn present(self) -> bool {
        self.0 > 0
    }
    /// `Brain.forgetOutdatedMemories`, once at the start of each tick.
    pub fn tick(&mut self) {
        if self.0 > 0 {
            self.0 -= 1;
        }
    }
}
/// `Behavior`'s bookkeeping in the brains ported behaviour for behaviour
/// (villager, animal, monster, warden): running, and until when.
#[derive(Clone, Copy, Debug)]
pub struct Timed {
    pub running: bool,
    pub end: i64,
    pub min: i32,
    pub max: i32,
}
impl Timed {
    pub fn new(min: i32, max: i32) -> Self {
        Self {
            running: false,
            end: 0,
            min,
            max,
        }
    }
    /// `Behavior.tryStart`'s duration draw from the level random, in Java's
    /// int arithmetic (made even when it cannot vary).
    pub fn begin(&mut self, lr: &mut LevelRandom, now: i64) {
        self.running = true;
        let bound = self.max.wrapping_add(1).wrapping_sub(self.min);
        let d = self.min.wrapping_add(lr.next_int_bound(bound).unwrap_or(0));
        self.end = now + i64::from(d);
    }
    /// `Behavior.timedOut`.
    pub fn timed_out(&self, now: i64) -> bool {
        now > self.end
    }
}
/// `WalkTarget`: where to go, how fast and how close counts as there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkTarget {
    pub pos: [i32; 3],
    pub speed: f64,
    pub close: i32,
}
impl WalkTarget {
    pub fn new(pos: [i32; 3], speed: f64, close: i32) -> Self {
        Self { pos, speed, close }
    }
}
/// `MoveToTargetSink(min, max)`: walks to the walk target and gives up on it
/// after a random time.
#[derive(Clone, Debug)]
pub struct Sink {
    min: i32,
    max: i32,
    until: Option<i32>,
}
impl Sink {
    pub fn new(min: i32, max: i32) -> Self {
        Self {
            min,
            max,
            until: None,
        }
    }
    pub fn running(&self) -> bool {
        self.until.is_some()
    }
}
use horizon_core::nbt::{Compound, Tag};




#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_memory_lasts_its_expiry_and_the_tick_it_was_set() {
        let mut t = Ttl::default();
        assert!(!t.present());
        t.set(2);
        let mut ticks = 0;
        while t.present() {
            ticks += 1;
            t.tick();
        }
        assert_eq!(ticks, 3);
    }
}
