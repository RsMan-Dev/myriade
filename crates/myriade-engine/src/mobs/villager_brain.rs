//! The villager's `Brain` (`Villager.BRAIN_PROVIDER` and
//! `VillagerGoalPackages`), ported behaviour for behaviour: memories that
//! expire, the nine sensors and their scan rates, activities from the
//! `villager_activity` timeline, and the behaviours of each activity, started
//! and ticked in vanilla's order (priority, then the activity's slot in
//! `Brain.availableBehaviorsByPriority`'s hash map, then insertion order).
//!
//! The level random (`ServerLevel.getRandom`) times behaviours and POI scans;
//! the mob's own random drives positions, sounds and jumps. `RunOne` and the
//! other `GateBehavior`s shuffle with a random of their own, seeded with
//! `Brain::shuffle_seed` when the brain is built.
mod reputation;
mod restock;

use crate::health::DamageLevel;
use horizon_core::{
    geometry::Vec3,
    random::{LegacyRandom, LevelRandom, RandomSource},
};

/// `MemorySlot.NEVER_EXPIRE`.
pub(super) const NEVER: i64 = i64::MAX;

/// `MemorySlot`: a value and its time to live.
#[derive(Clone, Debug)]
pub struct Slot<T> {
    v: Option<T>,
    pub(super) ttl: i64,
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
    /// `MemorySlot.tick` (`Brain.forgetOutdatedMemories`).
    pub(super) fn tick(&mut self) {
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
    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.v.as_mut()
    }
    pub fn present(&self) -> bool {
        self.v.is_some()
    }
    /// The time to live (`i64::MAX`: never expires).
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
    pub(super) fn set_list(&mut self, v: Vec<T>) {
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
    Idle,
    Work,
    Play,
    Rest,
    Meet,
    Panic,
    Raid,
    PreRaid,
    Hide,
}
impl Act {
    pub fn name(self) -> &'static str {
        match self {
            Act::Core => "core",
            Act::Idle => "idle",
            Act::Work => "work",
            Act::Play => "play",
            Act::Rest => "rest",
            Act::Meet => "meet",
            Act::Panic => "panic",
            Act::Raid => "raid",
            Act::PreRaid => "pre_raid",
            Act::Hide => "hide",
        }
    }
    /// The bucket order of `HashMap<Activity, ...>` (16 buckets, the hash
    /// of the name; equal buckets keep the villager's insertion order).
    fn slot(self) -> u8 {
        match self {
            Act::Play => 0,
            Act::Core => 1,
            Act::Rest => 2,
            Act::Hide => 3,
            Act::Meet => 4,
            Act::Idle => 5,
            Act::PreRaid => 6,
            Act::Work => 7,
            Act::Panic => 8,
            Act::Raid => 9,
        }
    }
}

/// The scan rate of each sensor, in `BRAIN_PROVIDER` order.
const SENSOR_RATES: [i64; 9] = [20, 20, 20, 20, 20, 20, 20, 40, 200];



/// The `villager_activity` and `baby_villager_activity` timelines.
fn schedule(baby: bool, day_time: i64) -> Act {
    match super::villager::activity_at(baby, day_time) {
        super::villager::Activity::Idle => Act::Idle,
        super::villager::Activity::Work => Act::Work,
        super::villager::Activity::Play => Act::Play,
        super::villager::Activity::Meet => Act::Meet,
        super::villager::Activity::Rest => Act::Rest,
    }
}








// ---- Sensors ------------------------------------------------------------

const FOLLOW_RANGE: f64 = 16.0;
include!("villager_brain/hostile_sensor.rs");
#[path = "villager_secondary.rs"]
mod secondary;



// ---- Behaviours ---------------------------------------------------------
use horizon_world::poi::{PoiIdentity as Poi, PoiType};

#[path = "villager_jobs.rs"]
mod profession_jobs;

use profession_jobs::*;

/// `BlockPos.closerToCenterThan(position, d)`.
pub(super) fn closer_to_center(p: [i32; 3], at: Vec3, d: f64) -> bool {
    let c = Vec3::new(
        f64::from(p[0]) + 0.5,
        f64::from(p[1]) + 0.5,
        f64::from(p[2]) + 0.5,
    );
    c.distance_squared(at) < d * d
}
fn manhattan(a: [i32; 3], b: [i32; 3]) -> i32 {
    (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs()
}
/// `PoiManager.sectionsToVillage`: the Chebyshev distance in sections to a
/// village section, at most 7.
pub(super) fn sections_to_village(villages: &[[i32; 3]], s: [i32; 3]) -> i32 {
    villages
        .iter()
        .map(|v| {
            (v[0] - s[0])
                .abs()
                .max((v[1] - s[1]).abs())
                .max((v[2] - s[2]).abs())
        })
        .min()
        .unwrap_or(7)
        .min(7)
}

/// A memory a gate or behaviour conditions on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mem {
    VisibleBabies,
    InteractionTarget,
    BreedTarget,
    Home,
}


/// `InteractWith`'s targets.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Interact {
    Villager,
    BreedVillager,
    Cat,
}
/// `AcquirePoi.JitteredLinearRetry`.
#[derive(Clone, Copy, Debug)]
struct Retry {
    previous: i64,
    next: i64,
    delay: i32,
}
impl Retry {
}

fn one(f: impl FnOnce() -> bool, running: &mut bool) -> bool {
    let r = f();
    if r {
        *running = true;
    }
    r
}


fn manhattan_sq(a: [i32; 3], b: [i32; 3]) -> i64 {
    let d = [
        i64::from(a[0] - b[0]),
        i64::from(a[1] - b[1]),
        i64::from(a[2] - b[2]),
    ];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

/// The remembered POI positions a behaviour reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PoiMem {
    Job,
    Potential,
    Home,
    Meeting,
}
impl PoiMem {
}
fn dist_sqr(a: [i32; 3], b: [i32; 3]) -> i64 {
    manhattan_sq(a, b)
}
/// `BehaviorUtils.findSectionClosestToVillage`.
pub(super) fn closest_section(villages: &[[i32; 3]], center: [i32; 3], radius: i32) -> [i32; 3] {
    let here = sections_to_village(villages, center);
    let mut best: Option<([i32; 3], i32)> = None;
    for z in center[2] - radius..=center[2] + radius {
        for y in center[1] - radius..=center[1] + radius {
            for x in center[0] - radius..=center[0] + radius {
                let s = [x, y, z];
                let d = sections_to_village(villages, s);
                if d < here && best.is_none_or(|(_, b)| d < b) {
                    best = Some((s, d));
                }
            }
        }
    }
    best.map_or(center, |(s, _)| s)
}

// ---- Packages -----------------------------------------------------------


// ---- Mod behaviors ------------------------------------------------------


// ---- The probe's view ---------------------------------------------------

fn pos(p: [i32; 3]) -> String {
    format!("{},{},{}", p[0], p[1], p[2])
}

#[cfg(test)]
mod poi_classification_tests {
    use super::*;
    use horizon_world::chunk::BlockState;



    #[test]
    #[ignore = "requires the vanilla bed POI oracle output"]
    fn vanilla_bed_poi_states() {
        let path = std::env::var("HORIZON_BED_POI_ORACLE").expect("oracle output path");
        let rows = std::fs::read_to_string(path).unwrap();
        assert_eq!(rows.lines().count(), 272);
        for row in rows.lines() {
            let fields: Vec<_> = row.split_whitespace().collect();
            let [name, part, expected] = fields[..] else {
                panic!("invalid oracle row")
            };
            let expected = expected == "true";
            let state = BlockState {
                name: name.into(),
                properties: [("part".into(), part.into())].into_iter().collect(),
            };
            assert_eq!(
                runtime_bed_poi(name, Some(part)).is_some(),
                expected,
                "{row}"
            );
            assert_eq!(
                horizon_world::poi::saved_poi(&state) == Some(PoiType::Home),
                expected,
                "{row}"
            );
        }
    }


    #[test]
    fn bed_heads_agree_between_runtime_and_saved_pois() {
        for (name, part, runtime, saved) in [
            ("minecraft:straw_bed", "head", false, false),
            ("minecraft:straw_bed", "foot", false, false),
            ("minecraft:red_bed", "head", true, true),
            ("minecraft:red_bed", "foot", false, false),
        ] {
            let state = BlockState {
                name: name.into(),
                properties: [("part".into(), part.into())].into_iter().collect(),
            };
            assert_eq!(runtime_bed_poi(name, Some(part)).is_some(), runtime);
            assert_eq!(horizon_world::poi::poi_type(&state).is_some(), saved);
        }
    }
}


