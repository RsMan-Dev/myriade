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
pub use reputation::murder_witnesses;
use reputation::targetable;

use super::{
    MobEvent, MobWorld, Species, Who,
    brain::{Timed, living_near},
    path::{Path, PathNode},
};
use crate::health::DamageLevel;
use horizon_core::{
    geometry::Vec3,
    random::{LegacyRandom, LevelRandom, RandomSource},
};
pub(super) use restock::restock_if_due;

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

/// `PositionTracker`: an entity (`EntityTracker`, at its eyes or feet) or a
/// block (`BlockPosTracker`, at its centre or at a given point).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tracker {
    Entity(Who, bool),
    Block([i32; 3]),
    Point([f64; 3]),
}
impl Tracker {
    /// `currentPosition`.
    pub fn position(&self, w: &MobWorld) -> Option<[f64; 3]> {
        match *self {
            Tracker::Entity(who, eye) => w.target_info(who).map(|t| {
                [
                    t.position.x,
                    if eye { t.eye_y } else { t.position.y },
                    t.position.z,
                ]
            }),
            Tracker::Block(p) => Some([
                f64::from(p[0]) + 0.5,
                f64::from(p[1]) + 0.5,
                f64::from(p[2]) + 0.5,
            ]),
            Tracker::Point(v) => Some(v),
        }
    }
    /// `currentBlockPosition`.
    pub fn block(&self, w: &MobWorld) -> Option<[i32; 3]> {
        match *self {
            Tracker::Entity(who, _) => w.target_info(who).map(|t| block_of(t.position)),
            Tracker::Block(p) => Some(p),
            Tracker::Point(v) => Some([
                v[0].floor() as i32,
                v[1].floor() as i32,
                v[2].floor() as i32,
            ]),
        }
    }
}
fn block_of(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
/// `WalkTarget`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkTarget {
    pub target: Tracker,
    pub speed: f32,
    pub close: i32,
}
impl WalkTarget {
    pub(super) fn block(p: [i32; 3], speed: f32, close: i32) -> Self {
        Self {
            target: Tracker::Block(p),
            speed,
            close,
        }
    }
    /// `new WalkTarget(Vec3, ...)`: the block containing the point.
    pub(super) fn point(p: [f64; 3], speed: f32, close: i32) -> Self {
        Self::block(
            [
                p[0].floor() as i32,
                p[1].floor() as i32,
                p[2].floor() as i32,
            ],
            speed,
            close,
        )
    }
}

/// `NearestVisibleLivingEntities`: the nearby entities and a lazily filled
/// cache of `Sensor.isEntityTargetable`.
#[derive(Clone, Debug, Default)]
pub struct Visible {
    pub(super) list: Vec<Who>,
    pub(super) cache: Vec<(Who, bool)>,
}

/// The memories a villager's brain registers.
#[derive(Clone, Debug, Default)]
pub struct Memories {
    pub home: Slot<[i32; 3]>,
    pub job_site: Slot<[i32; 3]>,
    pub potential_job_site: Slot<[i32; 3]>,
    pub meeting_point: Slot<[i32; 3]>,
    pub secondary_job_site: Slot<Vec<[i32; 3]>>,
    pub mobs: Slot<Vec<Who>>,
    pub visible_mobs: Slot<Visible>,
    pub visible_villager_babies: Slot<Vec<Who>>,
    pub nearest_players: Slot<Vec<Who>>,
    pub nearest_visible_player: Slot<Who>,
    pub nearest_visible_targetable_player: Slot<Who>,
    pub nearest_visible_targetable_players: Slot<Vec<Who>>,
    pub walk_target: Slot<WalkTarget>,
    pub look_target: Slot<Tracker>,
    pub interaction_target: Slot<Who>,
    pub breed_target: Slot<Who>,
    /// `PATH`: the navigation's path (its target).
    pub path: Slot<Path>,
    pub doors_to_close: Slot<Vec<[i32; 3]>>,
    pub nearest_bed: Slot<[i32; 3]>,
    /// `HURT_BY`: the damage (its game time).
    pub hurt_by: Slot<i64>,
    pub hurt_by_entity: Slot<Who>,
    pub nearest_hostile: Slot<Who>,
    pub hiding_place: Slot<[i32; 3]>,
    pub heard_bell_time: Slot<i64>,
    /// `NEAREST_VISIBLE_WANTED_ITEM`: where the item entity lies.
    pub nearest_wanted_item: Slot<[f64; 3]>,
    pub cant_reach_walk_target_since: Slot<i64>,
    pub golem_detected_recently: Slot<bool>,
    pub last_slept: Slot<i64>,
    pub last_woken: Slot<i64>,
    pub last_worked_at_poi: Slot<i64>,
}
impl Memories {
    fn tick(&mut self) {
        self.home.tick();
        self.job_site.tick();
        self.potential_job_site.tick();
        self.meeting_point.tick();
        self.secondary_job_site.tick();
        self.mobs.tick();
        self.visible_mobs.tick();
        self.visible_villager_babies.tick();
        self.nearest_players.tick();
        self.nearest_visible_player.tick();
        self.nearest_visible_targetable_player.tick();
        self.nearest_visible_targetable_players.tick();
        self.walk_target.tick();
        self.look_target.tick();
        self.interaction_target.tick();
        self.breed_target.tick();
        self.path.tick();
        self.doors_to_close.tick();
        self.nearest_bed.tick();
        self.hurt_by.tick();
        self.hurt_by_entity.tick();
        self.nearest_hostile.tick();
        self.hiding_place.tick();
        self.heard_bell_time.tick();
        self.nearest_wanted_item.tick();
        self.cant_reach_walk_target_since.tick();
        self.golem_detected_recently.tick();
        self.last_slept.tick();
        self.last_woken.tick();
        self.last_worked_at_poi.tick();
    }
    /// `Brain.pack`: the memories with a codec, which a rebuilt brain keeps.
    fn pack(&self) -> Memories {
        Memories {
            home: self.home.clone(),
            job_site: self.job_site.clone(),
            potential_job_site: self.potential_job_site.clone(),
            meeting_point: self.meeting_point.clone(),
            golem_detected_recently: self.golem_detected_recently.clone(),
            last_slept: self.last_slept.clone(),
            last_woken: self.last_woken.clone(),
            last_worked_at_poi: self.last_worked_at_poi.clone(),
            ..Memories::default()
        }
    }
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

/// A villager's brain.
#[derive(Clone, Debug)]
pub struct Brain {
    pub mem: Memories,
    sensors: [i64; 9],
    /// The active non-core activity (`CORE` is always active).
    pub activity: Act,
    default_activity: Act,
    last_schedule_update: i64,
    list: Vec<Entry>,
    pub shuffle_seed: i64,
    baby: bool,
    /// Whether the nearest-bed sensor's batch cache is used (`NearestBedSensor`).
    bed_cache: Vec<([i32; 3], i64)>,
    /// `NearestItemSensor` is due; the game, which knows the items, answers.
    scan_items: bool,
}
#[derive(Clone, Debug)]
struct Entry {
    prio: i32,
    act: Act,
    b: B,
}


impl Brain {
    fn new(_mob_id: &str, baby: bool, shuffle_seed: i64, farmer: bool) -> Self {
        let mut b = Brain {
            mem: Memories::default(),
            sensors: [0; 9],
            activity: Act::Idle,
            default_activity: Act::Idle,
            last_schedule_update: -9999,
            list: Vec::new(),
            shuffle_seed,
            baby,
            bed_cache: Vec::new(),
            scan_items: false,
        };
        b.list = packages(baby, shuffle_seed, farmer);
        b
    }
    fn requirements_met(&self, a: Act) -> bool {
        match a {
            Act::Work => !self.baby && self.mem.job_site.present(),
            Act::Play => self.baby,
            Act::Meet => self.mem.meeting_point.present(),
            Act::Core
            | Act::Idle
            | Act::Rest
            | Act::Panic
            | Act::Raid
            | Act::PreRaid
            | Act::Hide => true,
        }
    }
    pub fn is_active(&self, a: Act) -> bool {
        a == Act::Core || a == self.activity
    }
    /// `setActiveActivityIfPossible`.
    pub fn set_active_if_possible(&mut self, a: Act) {
        if self.requirements_met(a) {
            self.activity = a;
        } else {
            self.activity = self.default_activity;
        }
    }
    /// `updateActivityFromSchedule`.
    fn update_from_schedule(&mut self, game_time: i64, day_time: i64) {
        if game_time - self.last_schedule_update > 20 {
            self.last_schedule_update = game_time;
            let scheduled = schedule(self.baby, day_time);
            if !self.is_active(scheduled) {
                self.set_active_if_possible(scheduled);
            }
        }
    }
}
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



/// What behaviours read and emit during a brain tick.
pub struct Bx<'a, 'w, 'l> {
    pub w: &'a MobWorld<'w>,
    pub level: &'a mut DamageLevel<'l>,
    pub events: &'a mut Vec<MobEvent>,
    pub now: i64,
    /// `Villager.refreshBrain` was asked for.
    refresh: bool,
}





// ---- Sensors ------------------------------------------------------------

const FOLLOW_RANGE: f64 = 16.0;
include!("villager_brain/hostile_sensor.rs");
#[path = "villager_secondary.rs"]
mod secondary;
use secondary::sense_secondary;



// ---- Behaviours ---------------------------------------------------------
use horizon_world::poi::{PoiIdentity as Poi, PoiType};

#[path = "villager_jobs.rs"]
mod profession_jobs;

pub use profession_jobs::poi_at;
use profession_jobs::*;

/// The POIs of chunk `(qx, qz)` from `y0` to `y1` (within the world), in
/// scan order: y, then z, then x. Only the indexed POI blocks of the sections
/// are visited (`PoiManager.getInChunk`).
fn chunk_pois(
    w: &MobWorld,
    (qx, qz): (i32, i32),
    y0: i32,
    y1: i32,
    mut visit: impl FnMut(Poi, [i32; 3]),
) {
    let Some(chunk) = w.world.chunks.get(&(qx, qz)) else {
        return;
    };
    let kind = |state: u32| poi_of_state(w, horizon_world::registry::BlockStateId(state));
    let (min, mut lists) = (w.world.bottom_y(), Vec::new());
    w.world.pois.column(chunk, min, (y0, y1), kind, &mut lists);
    for (base, pois) in &lists {
        for &(i, poi) in pois.iter() {
            let i = i32::from(i);
            let y = base + (i >> 8);
            if (y0..=y1).contains(&y) {
                visit(poi, [qx * 16 + (i & 15), y, qz * 16 + (i >> 4 & 15)]);
            }
        }
    }
}
/// `PoiManager.getInRange(poiType -> true, center, radius, IS_OCCUPIED)`: the
/// POIs within `radius` of `center` that some villager holds a ticket on
/// (`taken`, from [`others_tickets`]), in scan order.
pub(super) fn occupied_pois_in_range(
    w: &MobWorld,
    taken: &crate::fast_hash::FxHashMap<[i32; 3], usize>,
    center: [i32; 3],
    radius: i32,
) -> Vec<[i32; 3]> {
    let mut found = Vec::new();
    let r2 = i64::from(radius) * i64::from(radius);
    let (cx, cz) = (center[0] >> 4, center[2] >> 4);
    let chunks = radius.div_euclid(16) + 1;
    let y0 = (center[1] - radius).max(w.world.bottom_y());
    let y1 = (center[1] + radius).min(w.world.top_y() - 1);
    for qz in cz - chunks..=cz + chunks {
        for qx in cx - chunks..=cx + chunks {
            chunk_pois(w, (qx, qz), y0, y1, |_, p| {
                if (p[0] - center[0]).abs() <= radius
                    && (p[2] - center[2]).abs() <= radius
                    && dist_sqr(p, center) <= r2
                    && taken.get(&p).is_some_and(|&n| n > 0)
                {
                    found.push(p);
                }
            });
        }
    }
    found
}
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

/// `GateBehavior`: `RunOne` (shuffled, run one) and the ordered gates.
#[derive(Clone, Debug)]
struct Gate {
    running: bool,
    /// Entry condition: this memory must be absent.
    absent: Option<Mem>,
    /// Erased when the gate stops.
    erase: Option<Mem>,
    shuffled: bool,
    random: LegacyRandom,
    kids: Vec<(B, i32, f64)>,
}
impl Gate {
    fn run_one(seed: i64, kids: Vec<(B, i32)>) -> Self {
        Self {
            running: false,
            absent: None,
            erase: None,
            shuffled: true,
            random: LegacyRandom::new(seed),
            kids: kids.into_iter().map(|(b, w)| (b, w, 0.0)).collect(),
        }
    }
    fn ordered(seed: i64, erase: Mem, kid: B) -> Self {
        Self {
            running: false,
            absent: None,
            erase: Some(erase),
            shuffled: false,
            random: LegacyRandom::new(seed),
            kids: vec![(kid, 1, 0.0)],
        }
    }
}

/// `SetEntityLookTarget`'s predicates.
#[derive(Clone, Copy, Debug, PartialEq)]
enum LookAt {
    Species(Species),
    Player,
    Category(&'static str),
}
/// `InteractWith`'s targets.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Interact {
    Villager,
    BreedVillager,
    Cat,
}
/// The fixed behaviours whose only state is their running flag (OneShots
/// stop in the same tick).
#[derive(Clone, Debug)]
enum B {
    Swim(Timed),
    InteractWithDoor {
        running: bool,
        last_node: Option<PathNode>,
        cooldown: i32,
    },
    LookAtTargetSink(Timed),
    PanicTrigger(Timed),
    WakeUp(bool),
    ReactToBell(bool),
    SetRaidStatus(bool),
    ValidatePoi {
        running: bool,
        filter: PoiFilter,
        which: PoiMem,
    },
    MoveToTargetSink {
        t: Timed,
        cooldown: i32,
        path: Option<Path>,
        last_target: Option<[i32; 3]>,
        speed: f32,
    },
    PoiCompetitorScan(bool),
    FollowTradingPlayer(Timed),
    GoToWantedItem(bool),
    AcquirePoi {
        running: bool,
        filter: PoiFilter,
        /// The memory filled (and checked absent): job site, home, meeting.
        poi: PoiFilter,
        only_adult: bool,
        event: bool,
        next: i64,
        retries: Vec<([i32; 3], Retry)>,
    },
    GoToPotentialJobSite(Timed),
    YieldJobSite(bool),
    AssignProfession(bool),
    ResetProfession(bool),
    Gate(Box<Gate>),
    InteractWith {
        running: bool,
        kind: Interact,
    },
    VillageBoundStroll {
        running: bool,
        speed: f32,
        xz: i32,
        y: i32,
    },
    WalkToLookTarget(bool),
    JumpOnBed {
        t: Timed,
        bed: Option<[i32; 3]>,
        time_left: i32,
        jumps: i32,
        cooldown: i32,
    },
    DoNothing(Timed),
    GiveGiftToHero {
        t: Timed,
        until_next: i32,
        given: bool,
        started: i64,
    },
    SetLookAndInteract(bool),
    ShowTrades {
        t: Timed,
        look_time: i32,
        /// `playerItemStack` (outer `None`: null) and `displayItems`,
        /// `cycleCounter`, `displayIndex`.
        player_item: Option<Option<std::sync::Arc<str>>>,
        items: Vec<crate::items::ItemStack>,
        cycle: i32,
        index: usize,
    },
    TradeWithVillager(Timed),
    MakeLove {
        t: Timed,
        birth: i64,
    },
    LookAtEntity {
        running: bool,
        what: LookAt,
    },
    UpdateActivity(bool),
    WorkAtPoi {
        t: Timed,
        last_check: i64,
        /// `WorkAtComposter` (a farmer's package) rather than `WorkAtPoi`.
        composter: bool,
    },
    StrollAroundPoi {
        running: bool,
        next_ok: i64,
        max_dist: i32,
        which: PoiMem,
    },
    StrollToPoi {
        running: bool,
        next_ok: i64,
    },
    StrollToPoiList {
        running: bool,
        next_ok: i64,
    },
    HarvestFarmland {
        t: Timed,
        next_ok: i64,
        target: Option<[i32; 3]>,
        valid: Vec<[i32; 3]>,
        worked: i32,
    },
    UseBonemeal {
        t: Timed,
        next_work: i64,
        last_session: i64,
        worked: i32,
        crop: Option<[i32; 3]>,
        /// The next crop is picked on the next tick, when the level shows the
        /// age the meal gave (vanilla grows the crop at once).
        repick: bool,
    },
    WalkToBlockMemory {
        running: bool,
        which: PoiMem,
        speed: f32,
        close: i32,
        too_far: i32,
        too_long: i64,
    },
    SleepInBed {
        t: Timed,
        next_ok: i64,
    },
    ClosestHomeAsWalkTarget {
        running: bool,
        last_update: i64,
        cache: Vec<([i32; 3], i64)>,
    },
    InsideBrownianWalk {
        running: bool,
        random: LegacyRandom,
    },
    GoToClosestVillage(bool),
    TriggerGate {
        running: bool,
        random: LegacyRandom,
        kids: Vec<(B, i32, f64)>,
    },
    SocializeAtBell(bool),
    CalmDown(bool),
    WalkAwayFrom {
        running: bool,
        hostile: bool,
    },
    RingBell(bool),
    ResetRaidStatus(bool),
    /// `BehaviorBuilder.sequence(triggerIf(raid condition), then)`.
    RaidSequence {
        running: bool,
        victory: bool,
        then: Box<B>,
    },
    MoveToSkySeeingSpot {
        running: bool,
        speed: f32,
    },
    Celebrate(Timed),
    LocateHidingPlace {
        running: bool,
        radius: i32,
        speed: f32,
        close: i32,
    },
    SetHiddenState {
        running: bool,
        hidden: i32,
    },
    PlayTag(bool),
}
/// `AcquirePoi.JitteredLinearRetry`.
#[derive(Clone, Copy, Debug)]
struct Retry {
    previous: i64,
    next: i64,
    delay: i32,
}
impl Retry {
    fn new(cx: &mut Bx, now: i64) -> Self {
        let mut r = Retry {
            previous: 0,
            next: 0,
            delay: 0,
        };
        r.mark(cx, now);
        r
    }
    fn mark(&mut self, cx: &mut Bx, now: i64) {
        self.previous = now;
        let suggested = self.delay + cx.level.random().next_int_bound(40).unwrap_or(0) + 40;
        self.delay = suggested.min(400);
        self.next = now + i64::from(self.delay);
    }
}

fn one(f: impl FnOnce() -> bool, running: &mut bool) -> bool {
    let r = f();
    if r {
        *running = true;
    }
    r
}

impl B {
    fn running(&self) -> bool {
        match self {
            B::Swim(t)
            | B::LookAtTargetSink(t)
            | B::PanicTrigger(t)
            | B::FollowTradingPlayer(t)
            | B::GoToPotentialJobSite(t)
            | B::DoNothing(t)
            | B::TradeWithVillager(t)
            | B::Celebrate(t) => t.running,
            B::MoveToTargetSink { t, .. }
            | B::GiveGiftToHero { t, .. }
            | B::ShowTrades { t, .. }
            | B::WorkAtPoi { t, .. }
            | B::HarvestFarmland { t, .. }
            | B::UseBonemeal { t, .. }
            | B::JumpOnBed { t, .. }
            | B::MakeLove { t, .. }
            | B::SleepInBed { t, .. } => t.running,
            B::InteractWithDoor { running, .. }
            | B::ValidatePoi { running, .. }
            | B::AcquirePoi { running, .. }
            | B::InteractWith { running, .. }
            | B::VillageBoundStroll { running, .. }
            | B::LookAtEntity { running, .. }
            | B::StrollAroundPoi { running, .. }
            | B::StrollToPoi { running, .. }
            | B::StrollToPoiList { running, .. }
            | B::WalkToBlockMemory { running, .. }
            | B::ClosestHomeAsWalkTarget { running, .. }
            | B::InsideBrownianWalk { running, .. }
            | B::TriggerGate { running, .. }
            | B::WalkAwayFrom { running, .. }
            | B::RaidSequence { running, .. }
            | B::MoveToSkySeeingSpot { running, .. }
            | B::LocateHidingPlace { running, .. }
            | B::SetHiddenState { running, .. } => *running,
            B::WakeUp(r)
            | B::ReactToBell(r)
            | B::SetRaidStatus(r)
            | B::PoiCompetitorScan(r)
            | B::GoToWantedItem(r)
            | B::YieldJobSite(r)
            | B::AssignProfession(r)
            | B::ResetProfession(r)
            | B::WalkToLookTarget(r)
            | B::SetLookAndInteract(r)
            | B::UpdateActivity(r)
            | B::GoToClosestVillage(r)
            | B::SocializeAtBell(r)
            | B::CalmDown(r)
            | B::RingBell(r)
            | B::ResetRaidStatus(r)
            | B::PlayTag(r) => *r,            B::Gate(g) => g.running,
        }
    }
    /// `OneShot` behaviours stop at their first tick.
    fn one_shot_stop(&mut self) -> bool {
        match self {
            B::InteractWithDoor { running, .. }
            | B::ValidatePoi { running, .. }
            | B::AcquirePoi { running, .. }
            | B::InteractWith { running, .. }
            | B::VillageBoundStroll { running, .. }
            | B::LookAtEntity { running, .. }
            | B::StrollAroundPoi { running, .. }
            | B::StrollToPoi { running, .. }
            | B::StrollToPoiList { running, .. }
            | B::WalkToBlockMemory { running, .. }
            | B::ClosestHomeAsWalkTarget { running, .. }
            | B::InsideBrownianWalk { running, .. }
            | B::TriggerGate { running, .. }
            | B::WalkAwayFrom { running, .. }
            | B::RaidSequence { running, .. }
            | B::MoveToSkySeeingSpot { running, .. }
            | B::LocateHidingPlace { running, .. }
            | B::SetHiddenState { running, .. }
            | B::GoToClosestVillage(running)
            | B::SocializeAtBell(running)
            | B::CalmDown(running)
            | B::RingBell(running)
            | B::ResetRaidStatus(running)
            | B::PlayTag(running)
            | B::WakeUp(running)
            | B::ReactToBell(running)
            | B::SetRaidStatus(running)
            | B::PoiCompetitorScan(running)
            | B::GoToWantedItem(running)
            | B::YieldJobSite(running)
            | B::AssignProfession(running)
            | B::ResetProfession(running)
            | B::WalkToLookTarget(running)
            | B::SetLookAndInteract(running)
            | B::UpdateActivity(running) => {
                *running = false;
                true
            }
            _ => false,
        }
    }



}

fn manhattan_sq(a: [i32; 3], b: [i32; 3]) -> i64 {
    let d = [
        i64::from(a[0] - b[0]),
        i64::from(a[1] - b[1]),
        i64::from(a[2] - b[2]),
    ];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}
fn same_path(a: Option<&Path>, b: Option<&Path>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.nodes == b.nodes && a.target == b.target,
        _ => false,
    }
}

/// Farmland subclasses include mod templates.
fn farmland(w: &MobWorld, p: [i32; 3]) -> bool {
    w.facts
        .get(w.world.block(p[0], p[1], p[2]))
        .has_class("FarmlandBlock")
}
/// `HarvestFarmland.validPos`: a ripe CropBlock or air above farmland.
fn farmland_valid(w: &MobWorld, p: [i32; 3]) -> bool {
    super::villager::ripe_crop(w, p)
        || w.facts.is_air(w.world.block(p[0], p[1], p[2])) && farmland(w, [p[0], p[1] - 1, p[2]])
}
/// `HarvestFarmland.getValidFarmland`: one picked by the level random.
fn pick_farmland(valid: &[[i32; 3]], cx: &mut Bx) -> Option<[i32; 3]> {
    if valid.is_empty() {
        return None;
    }
    let i = cx
        .level
        .random()
        .next_int_bound(valid.len() as i32)
        .unwrap_or(0) as usize;
    Some(valid[i])
}
/// A tagged BlockItem plants its block's default state.
fn seed_crop(w: &MobWorld, item: &str) -> Option<horizon_world::registry::BlockStateId> {
    let items = w.items?;
    if !items.item_in_tag("minecraft:villager_plantable_seeds", item) {
        return None;
    }
    items.get(item)?.block
}
/// `UseBonemeal.validPos`: a `CropBlock` below its max age.
fn bonemeal_valid(w: &MobWorld, p: [i32; 3]) -> bool {
    super::villager::crop(w, p) && !super::villager::ripe_crop(w, p)
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
/// Whether the block at `p` is a door mobs open (`MOB_INTERACTABLE_DOORS`), and
/// then whether it is open.
fn door_open(w: &MobWorld, p: [i32; 3]) -> Option<bool> {
    let name = w.block_name(p[0], p[1], p[2]);
    (name.ends_with("_door") && w.has_tag("minecraft:mob_interactable_doors", name))
        .then(|| w.prop(p[0], p[1], p[2], "open") == Some("true"))
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

fn timed(min: i32, max: i32) -> Timed {
    Timed::new(min, max)
}
fn run_one(seed: i64, kids: Vec<(B, i32)>) -> B {
    B::Gate(Box::new(Gate::run_one(seed, kids)))
}
fn look_at(what: LookAt) -> B {
    B::LookAtEntity {
        running: false,
        what,
    }
}
/// `getMinimalLookBehavior`.
fn look_minimal(seed: i64) -> B {
    run_one(
        seed,
        vec![
            (look_at(LookAt::Species(Species::Villager)), 2),
            (look_at(LookAt::Player), 2),
            (B::DoNothing(timed(30, 60)), 8),
        ],
    )
}
/// `getFullLookBehavior`.
fn look_full(seed: i64) -> B {
    run_one(
        seed,
        vec![
            (look_at(LookAt::Species(Species::Cat)), 8),
            (look_at(LookAt::Species(Species::Villager)), 2),
            (look_at(LookAt::Player), 2),
            (look_at(LookAt::Category("creature")), 1),
            (look_at(LookAt::Category("water_creature")), 1),
            (look_at(LookAt::Category("axolotls")), 1),
            (look_at(LookAt::Category("underground_water_creature")), 1),
            (look_at(LookAt::Category("water_ambient")), 1),
            (look_at(LookAt::Category("monster")), 1),
            (B::DoNothing(timed(30, 60)), 2),
        ],
    )
}
fn stroll(speed: f32, xz: i32, y: i32) -> B {
    B::VillageBoundStroll {
        running: false,
        speed,
        xz,
        y,
    }
}
fn walk_to(which: PoiMem, speed: f32, close: i32, too_far: i32, too_long: i64) -> B {
    B::WalkToBlockMemory {
        running: false,
        which,
        speed,
        close,
        too_far,
        too_long,
    }
}
fn trigger_gate(seed: i64, kids: Vec<(B, i32)>) -> B {
    B::TriggerGate {
        running: false,
        random: LegacyRandom::new(seed),
        kids: kids.into_iter().map(|(b, w)| (b, w, 0.0)).collect(),
    }
}
fn gift() -> B {
    B::GiveGiftToHero {
        t: timed(100, 100),
        until_next: 600,
        given: false,
        started: 0,
    }
}
fn show_trades() -> B {
    B::ShowTrades {
        t: timed(400, 1600),
        look_time: 0,
        player_item: None,
        items: Vec::new(),
        cycle: 0,
        index: 0,
    }
}
fn interact(kind: Interact) -> B {
    B::InteractWith {
        running: false,
        kind,
    }
}
fn trade_gate(seed: i64) -> B {
    B::Gate(Box::new(Gate::ordered(
        seed,
        Mem::InteractionTarget,
        B::TradeWithVillager(timed(60, 60)),
    )))
}
fn jump_on_bed() -> B {
    B::JumpOnBed {
        t: timed(60, 60),
        bed: None,
        time_left: 0,
        jumps: 0,
        cooldown: 0,
    }
}
fn move_to_target_sink(min: i32, max: i32) -> B {
    B::MoveToTargetSink {
        t: timed(min, max),
        cooldown: 0,
        path: None,
        last_target: None,
        speed: 0.0,
    }
}
fn acquire(filter: PoiFilter, poi: PoiFilter, only_adult: bool, event: bool) -> B {
    B::AcquirePoi {
        running: false,
        filter,
        poi,
        only_adult,
        event,
        next: 0,
        retries: Vec::new(),
    }
}
fn validate(filter: PoiFilter, which: PoiMem) -> B {
    B::ValidatePoi {
        running: false,
        filter,
        which,
    }
}
/// `VillagerGoalPackages`: the behaviours of every activity, in iteration
/// order. `speedModifier` is 0.5 throughout.
fn packages(baby: bool, seed: i64, farmer: bool) -> Vec<Entry> {
    let mut v: Vec<Entry> = Vec::new();
    let mut add = |prio: i32, act: Act, b: B| v.push(Entry { prio, act, b });
    if baby {
        // PLAY
        let p = Act::Play;
        add(0, p, move_to_target_sink(80, 120));
        add(5, p, look_full(seed));
        add(5, p, B::PlayTag(false));
        let mut play = Gate::run_one(
            seed,
            vec![
                (interact(Interact::Villager), 2),
                (interact(Interact::Cat), 1),
                (stroll(0.5, 10, 7), 1),
                (B::WalkToLookTarget(false), 1),
                (jump_on_bed(), 2),
                (B::DoNothing(timed(20, 40)), 2),
            ],
        );
        play.absent = Some(Mem::VisibleBabies);
        add(5, p, B::Gate(Box::new(play)));
        add(99, p, B::UpdateActivity(false));
    } else {
        // WORK
        let w = Act::Work;
        add(5, w, look_minimal(seed));
        add(
            5,
            w,
            run_one(
                seed,
                vec![
                    (
                        B::WorkAtPoi {
                            t: timed(60, 60),
                            last_check: 0,
                            composter: farmer,
                        },
                        7,
                    ),
                    (
                        B::StrollAroundPoi {
                            running: false,
                            next_ok: 0,
                            max_dist: 4,
                            which: PoiMem::Job,
                        },
                        2,
                    ),
                    (
                        B::StrollToPoi {
                            running: false,
                            next_ok: 0,
                        },
                        5,
                    ),
                    (
                        B::StrollToPoiList {
                            running: false,
                            next_ok: 0,
                        },
                        5,
                    ),
                    (
                        B::HarvestFarmland {
                            t: timed(60, 60),
                            next_ok: 0,
                            target: None,
                            valid: Vec::new(),
                            worked: 0,
                        },
                        if farmer { 2 } else { 5 },
                    ),
                    (
                        B::UseBonemeal {
                            t: timed(60, 60),
                            next_work: 0,
                            last_session: 0,
                            worked: 0,
                            crop: None,
                            repick: false,
                        },
                        if farmer { 4 } else { 7 },
                    ),
                ],
            ),
        );
        add(10, w, show_trades());
        add(10, w, B::SetLookAndInteract(false));
        add(2, w, walk_to(PoiMem::Job, 0.5, 9, 100, 1200));
        add(3, w, gift());
        add(99, w, B::UpdateActivity(false));
    }
    // CORE
    let c = Act::Core;
    add(0, c, B::Swim(timed(60, 60)));
    add(
        0,
        c,
        B::InteractWithDoor {
            running: false,
            last_node: None,
            cooldown: 0,
        },
    );
    add(0, c, B::LookAtTargetSink(timed(45, 90)));
    add(0, c, B::PanicTrigger(timed(60, 60)));
    add(0, c, B::WakeUp(false));
    add(0, c, B::ReactToBell(false));
    add(0, c, B::SetRaidStatus(false));
    add(0, c, validate(PoiFilter::Held, PoiMem::Job));
    add(0, c, validate(PoiFilter::Acquirable, PoiMem::Potential));
    add(1, c, move_to_target_sink(150, 250));
    add(2, c, B::PoiCompetitorScan(false));
    add(3, c, B::FollowTradingPlayer(timed(i32::MAX, i32::MAX)));
    add(5, c, B::GoToWantedItem(false));
    add(
        6,
        c,
        acquire(PoiFilter::Acquirable, PoiFilter::Acquirable, true, false),
    );
    add(7, c, B::GoToPotentialJobSite(timed(1200, 1200)));
    add(8, c, B::YieldJobSite(false));
    add(
        10,
        c,
        acquire(PoiFilter::Home, PoiFilter::Home, false, true),
    );
    add(
        10,
        c,
        acquire(PoiFilter::Meeting, PoiFilter::Meeting, true, true),
    );
    add(10, c, B::AssignProfession(false));
    add(10, c, B::ResetProfession(false));
    // MEET
    let me = Act::Meet;
    add(
        2,
        me,
        trigger_gate(
            seed,
            vec![
                (
                    B::StrollAroundPoi {
                        running: false,
                        next_ok: 0,
                        max_dist: 40,
                        which: PoiMem::Meeting,
                    },
                    2,
                ),
                (B::SocializeAtBell(false), 2),
            ],
        ),
    );
    add(10, me, show_trades());
    add(10, me, B::SetLookAndInteract(false));
    add(2, me, walk_to(PoiMem::Meeting, 0.5, 6, 100, 200));
    add(3, me, gift());
    add(3, me, validate(PoiFilter::Meeting, PoiMem::Meeting));
    add(3, me, trade_gate(seed));
    add(5, me, look_full(seed));
    add(99, me, B::UpdateActivity(false));
    // REST
    let r = Act::Rest;
    add(2, r, walk_to(PoiMem::Home, 0.5, 1, 150, 1200));
    add(3, r, validate(PoiFilter::Home, PoiMem::Home));
    add(
        3,
        r,
        B::SleepInBed {
            t: timed(60, 60),
            next_ok: 0,
        },
    );
    let mut homeless = Gate::run_one(
        seed,
        vec![
            (
                B::ClosestHomeAsWalkTarget {
                    running: false,
                    last_update: 0,
                    cache: Vec::new(),
                },
                1,
            ),
            (
                B::InsideBrownianWalk {
                    running: false,
                    random: LegacyRandom::new(seed),
                },
                4,
            ),
            (B::GoToClosestVillage(false), 2),
            (B::DoNothing(timed(20, 40)), 2),
        ],
    );
    homeless.absent = Some(Mem::Home);
    add(5, r, B::Gate(Box::new(homeless)));
    add(5, r, look_minimal(seed));
    add(99, r, B::UpdateActivity(false));
    // IDLE
    let i = Act::Idle;
    add(
        2,
        i,
        run_one(
            seed,
            vec![
                (interact(Interact::Villager), 2),
                (interact(Interact::BreedVillager), 1),
                (interact(Interact::Cat), 1),
                (stroll(0.5, 10, 7), 1),
                (B::WalkToLookTarget(false), 1),
                (jump_on_bed(), 1),
                (B::DoNothing(timed(30, 60)), 1),
            ],
        ),
    );
    add(3, i, gift());
    add(3, i, B::SetLookAndInteract(false));
    add(3, i, show_trades());
    add(3, i, trade_gate(seed));
    add(
        3,
        i,
        B::Gate(Box::new(Gate::ordered(
            seed,
            Mem::BreedTarget,
            B::MakeLove {
                t: timed(350, 350),
                birth: 0,
            },
        ))),
    );
    add(5, i, look_full(seed));
    add(99, i, B::UpdateActivity(false));
    // PANIC
    let pa = Act::Panic;
    add(0, pa, B::CalmDown(false));
    add(
        1,
        pa,
        B::WalkAwayFrom {
            running: false,
            hostile: true,
        },
    );
    add(
        1,
        pa,
        B::WalkAwayFrom {
            running: false,
            hostile: false,
        },
    );
    add(3, pa, stroll(0.75, 2, 2));
    add(5, pa, look_minimal(seed));
    // PRE_RAID
    let pr = Act::PreRaid;
    add(0, pr, B::RingBell(false));
    add(
        0,
        pr,
        trigger_gate(
            seed,
            vec![
                (walk_to(PoiMem::Meeting, 0.75, 2, 150, 200), 6),
                (stroll(0.75, 10, 7), 2),
            ],
        ),
    );
    add(5, pr, look_minimal(seed));
    add(99, pr, B::ResetRaidStatus(false));
    // RAID
    let ra = Act::Raid;
    add(
        0,
        ra,
        B::RaidSequence {
            running: false,
            victory: true,
            then: Box::new(trigger_gate(
                seed,
                vec![
                    (
                        B::MoveToSkySeeingSpot {
                            running: false,
                            speed: 0.5,
                        },
                        5,
                    ),
                    (stroll(0.55, 10, 7), 2),
                ],
            )),
        },
    );
    add(0, ra, B::Celebrate(timed(600, 600)));
    add(
        2,
        ra,
        B::RaidSequence {
            running: false,
            victory: false,
            then: Box::new(B::LocateHidingPlace {
                running: false,
                radius: 24,
                speed: 0.7,
                close: 1,
            }),
        },
    );
    add(5, ra, look_minimal(seed));
    add(99, ra, B::ResetRaidStatus(false));
    // HIDE
    let h = Act::Hide;
    add(
        0,
        h,
        B::SetHiddenState {
            running: false,
            hidden: 0,
        },
    );
    add(
        1,
        h,
        B::LocateHidingPlace {
            running: false,
            radius: 32,
            speed: 0.625,
            close: 2,
        },
    );
    add(5, h, look_minimal(seed));
    v.sort_by_key(|e| (e.prio, e.act.slot()));
    v
}

// ---- Mod behaviors ------------------------------------------------------


// ---- The probe's view ---------------------------------------------------

fn who(w: Who) -> String {
    match w {
        Who::Player => "player".into(),
        Who::Mob(id) => format!("e{id}"),
    }
}
fn pos(p: [i32; 3]) -> String {
    format!("{},{},{}", p[0], p[1], p[2])
}
fn tracker(t: Tracker) -> String {
    match t {
        Tracker::Entity(e, _) => who(e),
        Tracker::Block(p) => pos(p),
        Tracker::Point(v) => pos([
            v[0].floor() as i32,
            v[1].floor() as i32,
            v[2].floor() as i32,
        ]),
    }
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


