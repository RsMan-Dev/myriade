use crate::health::DamageLevel;
use horizon_core::{
    geometry::Vec3,
    random::{LegacyRandom, RandomSource},
};

/// `MemorySlot.NEVER_EXPIRE`.
const NEVER: i64 = i64::MAX;

/// `MemorySlot`: a value and its time to live.
#[derive(Clone, Debug)]
pub struct Slot<T> {
    v: Option<T>,
    ttl: i64,
}
impl<T> Default for Slot<T> {
    fn default() -> Self {
        Self {
            v: None,
            ttl: NEVER,
        }
    }
}
impl<T> Slot<T> {
    fn tick(&mut self) {
        if self.v.is_some() && self.ttl != NEVER {
            if self.ttl <= 0 {
                self.clear();
            } else {
                self.ttl -= 1;
            }
        }
    }
    pub fn set(&mut self, v: T) {
        self.v = Some(v);
        self.ttl = NEVER;
    }
    pub fn set_ttl(&mut self, v: T, ttl: i64) {
        self.v = Some(v);
        self.ttl = ttl;
    }
    pub fn set_opt(&mut self, v: Option<T>) {
        match v {
            Some(v) => self.set(v),
            None => self.clear(),
        }
    }
    pub fn clear(&mut self) {
        self.v = None;
        self.ttl = NEVER;
    }
    pub fn get(&self) -> Option<&T> {
        self.v.as_ref()
    }
    pub fn present(&self) -> bool {
        self.v.is_some()
    }
    /// `MemorySlot.timeToLive`: `i64::MAX` denotes a permanent memory.
    pub fn ttl(&self) -> i64 {
        self.ttl
    }
}
impl<T: Clone> Slot<T> {
    pub fn value(&self) -> Option<T> {
        self.v.clone()
    }
}
impl<T> Slot<Vec<T>> {
    /// `Brain.setMemory` of a collection: an empty one erases the memory.
    fn set_list(&mut self, v: Vec<T>) {
        if v.is_empty() {
            self.clear();
        } else {
            self.set(v);
        }
    }
}

fn block_of(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}




/// `Activity`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Core,
    Fight,
    PlayDead,
    Idle,
    Sniff,
    Dig,
    Panic,
}
impl Act {
    pub fn name(self) -> &'static str {
        match self {
            Act::Core => "core",
            Act::Fight => "fight",
            Act::PlayDead => "play_dead",
            Act::Idle => "idle",
            Act::Sniff => "sniff",
            Act::Dig => "dig",
            Act::Panic => "panic",
        }
    }
}

/// Which brain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Axolotl,
    Sniffer,
    /// A happy ghast (the brain ticks while it is a ghastling).
    Ghastling,
    /// `CopperGolemAi`.
    CopperGolem,
    /// `TadpoleAi`.
    Tadpole,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sensor {
    Living,
    AxolotlAttackables,
    AdultSameType,
    HurtBy,
    Players,
    Tempting,
    AdultAnyType,
}
impl Kind {
    /// `BRAIN_PROVIDER`'s sensors, in order (all scan every 20 ticks).
    fn sensors(self) -> &'static [Sensor] {
        match self {
            Kind::Axolotl => &[
                Sensor::Living,
                Sensor::AdultSameType,
                Sensor::HurtBy,
                Sensor::AxolotlAttackables,
                Sensor::Tempting,
            ],
            Kind::Sniffer => &[
                Sensor::Living,
                Sensor::HurtBy,
                Sensor::Players,
                Sensor::Tempting,
            ],
            Kind::Ghastling => &[
                Sensor::Living,
                Sensor::HurtBy,
                Sensor::Tempting,
                Sensor::AdultAnyType,
                Sensor::Players,
            ],
            Kind::CopperGolem => &[Sensor::Living, Sensor::HurtBy],
            // `Tadpole.SENSOR_TYPES`: the living entities, the players, what hurt it and
            // the frog food players hold.
            Kind::Tadpole => &[
                Sensor::Living,
                Sensor::Players,
                Sensor::HurtBy,
                Sensor::Tempting,
            ],
        }
    }
}












/// The explored positions are the sniffer's `SNIFFER_EXPLORED_POSITIONS`.
const EXPLORED_MAX: usize = 20;

// ---- Sensors ------------------------------------------------------------


// ---- Behaviours ------------------------------------------------------------




fn dist_sqr(a: [i32; 3], b: [i32; 3]) -> i64 {
    let d = |i: usize| i64::from(a[i] - b[i]);
    d(0) * d(0) + d(1) * d(1) + d(2) * d(2)
}
fn manhattan(a: [i32; 3], b: [i32; 3]) -> i32 {
    (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs()
}

// ---- The sniffer --------------------------------------------------------




// ---- AxolotlAi -----------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum AxolotlAction {
    ValidateDead,
    StartAttack,
    StopAttack,
    Chase,
    Bite,
    EraseAttackBreeding,
    EraseDeadBreeding,
    FindLiquid,
    SwimStroll,
    WalkFromLook,
    /// `SetWalkTargetFromLookTarget.create(0.5, 3)` of the tadpole: no condition on the
    /// water, a fixed speed.
    TadpoleWalkFromLook,
    StayWater,
    StayGround,
}


// ---- Debug output (the mob AI harness) ------------------------------------

fn pos(p: [i32; 3]) -> String {
    format!("{},{},{}", p[0], p[1], p[2])
}


#[cfg(test)]
mod axolotl_parity_tests {
    use super::*;
    fn empty_brain() -> Brain {
        Brain {
            kind: Kind::Axolotl,
            mem: Memories::default(),
            sensors: Vec::new(),
            activity: Act::Idle,
            list: Vec::new(),
        }
    }
    #[test]
    fn fight_exit_sets_hunting_cooldown_and_play_dead_has_priority() {
        let mut b = empty_brain();
        b.mem.attack_target.set(Who::Mob(1));
        update_axolotl_activity(&mut b);
        assert_eq!(b.activity, Act::Fight);
        b.mem.play_dead_ticks.set(200);
        update_axolotl_activity(&mut b);
        assert_eq!(b.activity, Act::PlayDead);
        assert_eq!(b.mem.hunting_cooldown.ttl, 2400);
        assert!(!b.mem.attack_target.present());
        for _ in 0..2400 {
            b.mem.hunting_cooldown.tick();
        }
        assert!(b.mem.hunting_cooldown.present());
        b.mem.hunting_cooldown.tick();
        assert!(!b.mem.hunting_cooldown.present());
    }
    #[test]
    fn play_dead_flag_ends_at_zero_before_memory_is_erased() {
        let mut b = empty_brain();
        b.activity = Act::PlayDead;
        b.mem.play_dead_ticks.set(200);
        b.mem.hurt_by_entity.set(Who::Mob(7));
        for _ in 0..200 {
            assert!(validate_play_dead(&mut b));
        }
        assert_eq!(b.mem.play_dead_ticks.value(), Some(0));
        assert_eq!(b.activity, Act::PlayDead);
        assert!(validate_play_dead(&mut b));
        assert!(!b.mem.play_dead_ticks.present());
        assert!(!b.mem.hurt_by_entity.present());
        assert_eq!(b.activity, Act::Idle);
    }
    #[test]
    fn axolotl_has_native_activity_brain_not_fallback_goals() {
        assert!(kind_for_species(Species::Axolotl).is_some());
    }
}

#[cfg(test)]
mod breeding_duration_tests {
    use super::*;
    #[test]
    fn axolotl_breeding_uses_native_110_tick_timeout() {
        let package = axolotl_package(1);
        let timed = package
            .iter()
            .find_map(|e| {
                if let B::MakeLove { t, .. } = &e.b {
                    Some(t)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!((timed.min, timed.max), (110, 110));
    }
}

