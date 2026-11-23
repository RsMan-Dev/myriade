//! The `Brain`s of the sniffer (`SnifferAi`) and the ghastling
//! (`HappyGhastAi`), ported behaviour for behaviour: memories that expire,
//! the sensors and their scan rates, the activities and their
//! requirements, and the behaviours of each activity, started and ticked in
//! vanilla's order (priority, then the activity's slot in
//! `Brain.availableBehaviorsByPriority`'s hash map, then insertion order).
//!
//! The level random (`ServerLevel.getRandom`) times behaviours; the mob's
//! own random drives positions, sounds and jumps. `RunOne` shuffles with a
//! random of its own, seeded with the brain's `shuffle_seed` when it is
//! built.
use super::{
    MobEvent, MobWorld, Species, Who,
    brain::{Timed, living_near},
    path::{Path, PathNode},
    sniffer,
};
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

/// `PositionTracker`: an entity (`EntityTracker`: at its eyes or feet, and
/// whether its block is the one at its eyes) or a block (`BlockPosTracker`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tracker {
    Entity {
        who: Who,
        eye: bool,
        block_eye: bool,
    },
    Block([i32; 3]),
}
impl Tracker {
    fn entity(who: Who, eye: bool) -> Self {
        Tracker::Entity {
            who,
            eye,
            block_eye: false,
        }
    }
    /// `currentPosition`.
    pub(super) fn position(&self, w: &MobWorld) -> Option<[f64; 3]> {
        match *self {
            Tracker::Entity { who, eye, .. } => w.target_info(who).map(|t| {
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
        }
    }
    /// `currentBlockPosition`.
    fn block(&self, w: &MobWorld) -> Option<[i32; 3]> {
        match *self {
            Tracker::Entity { who, block_eye, .. } => w.target_info(who).map(|t| {
                if block_eye {
                    block_of(Vec3::new(t.position.x, t.eye_y, t.position.z))
                } else {
                    block_of(t.position)
                }
            }),
            Tracker::Block(p) => Some(p),
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
    /// `new WalkTarget(Vec3, ...)`: the block containing the point.
    fn point(p: [f64; 3], speed: f32, close: i32) -> Self {
        Self {
            target: Tracker::Block([
                p[0].floor() as i32,
                p[1].floor() as i32,
                p[2].floor() as i32,
            ]),
            speed,
            close,
        }
    }
}

/// `NearestVisibleLivingEntities`: the nearby entities and a lazily filled
/// cache of `Sensor.isEntityTargetable`.
#[derive(Clone, Debug, Default)]
pub struct Visible {
    list: Vec<Who>,
    cache: Vec<(Who, bool)>,
}

/// The memories these brains register.
#[derive(Clone, Debug, Default)]
pub struct Memories {
    pub mobs: Slot<Vec<Who>>,
    pub visible_mobs: Slot<Visible>,
    pub nearest_players: Slot<Vec<Who>>,
    pub nearest_visible_player: Slot<Who>,
    pub nearest_visible_targetable_player: Slot<Who>,
    pub nearest_visible_targetable_players: Slot<Vec<Who>>,
    pub nearest_visible_adult: Slot<Who>,
    pub walk_target: Slot<WalkTarget>,
    pub look_target: Slot<Tracker>,
    pub breed_target: Slot<Who>,
    pub path: Slot<Path>,
    /// `HURT_BY`: whether the damage is one of `#panic_causes`.
    pub hurt_by: Slot<bool>,
    pub hurt_by_entity: Slot<Who>,
    pub cant_reach_walk_target_since: Slot<i64>,
    pub tempting_player: Slot<Who>,
    pub temptation_cooldown_ticks: Slot<i32>,
    pub is_panicking: Slot<bool>,
    pub sniffer_digging: Slot<bool>,
    pub sniffer_sniffing_target: Slot<[i32; 3]>,
    pub sniffer_happy: Slot<bool>,
    pub sniff_cooldown: Slot<()>,
    pub gaze_cooldown_ticks: Slot<i32>,
    pub transport_items_cooldown_ticks: Slot<i32>,
    pub nearest_attackable: Slot<Who>,
    pub attack_target: Slot<Who>,
    pub attack_cooling_down: Slot<bool>,
    pub hunting_cooldown: Slot<bool>,
    pub play_dead_ticks: Slot<i32>,
    pub visited_block_positions: Slot<Vec<[i32; 3]>>,
    pub unreachable_transport_block_positions: Slot<Vec<[i32; 3]>>,
    /// `DOORS_TO_CLOSE`: the doors it opened or passed through.
    pub doors_to_close: Slot<Vec<[i32; 3]>>,
}
impl Memories {
    fn tick(&mut self) {
        self.nearest_attackable.tick();
        self.attack_target.tick();
        self.attack_cooling_down.tick();
        self.hunting_cooldown.tick();
        self.play_dead_ticks.tick();
        self.mobs.tick();
        self.visible_mobs.tick();
        self.nearest_players.tick();
        self.nearest_visible_player.tick();
        self.nearest_visible_targetable_player.tick();
        self.nearest_visible_targetable_players.tick();
        self.nearest_visible_adult.tick();
        self.walk_target.tick();
        self.look_target.tick();
        self.breed_target.tick();
        self.path.tick();
        self.hurt_by.tick();
        self.hurt_by_entity.tick();
        self.cant_reach_walk_target_since.tick();
        self.tempting_player.tick();
        self.temptation_cooldown_ticks.tick();
        self.is_panicking.tick();
        self.sniffer_digging.tick();
        self.sniffer_sniffing_target.tick();
        self.sniffer_happy.tick();
        self.sniff_cooldown.tick();
        self.gaze_cooldown_ticks.tick();
        self.transport_items_cooldown_ticks.tick();
        self.visited_block_positions.tick();
        self.unreachable_transport_block_positions.tick();
        self.doors_to_close.tick();
    }
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

/// An animal's brain.
#[derive(Clone, Debug)]
pub struct Brain {
    pub kind: Kind,
    pub mem: Memories,
    sensors: Vec<(Sensor, i64)>,
    /// The active non-core activity (`CORE` is always active).
    pub activity: Act,
    list: Vec<Entry>,
}
#[derive(Clone, Debug)]
struct Entry {
    prio: i32,
    act: Act,
    b: B,
}


fn kind_for_species(species: Species) -> Option<Kind> {
    match species {
        Species::Axolotl => Some(Kind::Axolotl),
        Species::Sniffer => Some(Kind::Sniffer),
        // Every happy ghast has the brain; only the ghastling's ticks.
        Species::HappyGhast => Some(Kind::Ghastling),
        Species::CopperGolem => Some(Kind::CopperGolem),
        Species::Tadpole => Some(Kind::Tadpole),
        _ => None,
    }
}






/// What behaviours read and emit during a brain tick.
pub struct Bx<'a, 'w, 'l> {
    pub w: &'a MobWorld<'w>,
    pub level: &'a mut DamageLevel<'l>,
    pub events: &'a mut Vec<MobEvent>,
    pub now: i64,
}



/// The explored positions are the sniffer's `SNIFFER_EXPLORED_POSITIONS`.
const EXPLORED_MAX: usize = 20;

// ---- Sensors ------------------------------------------------------------

/// `#followable_friendly_mobs`.
fn followable_friendly(s: Species) -> bool {
    matches!(
        s,
        Species::Camel
            | Species::Cat
            | Species::Chicken
            | Species::Cow
            | Species::Donkey
            | Species::Fox
            | Species::Goat
            | Species::HappyGhast
            | Species::Horse
            | Species::SkeletonHorse
            | Species::Llama
            | Species::Mule
            | Species::Ocelot
            | Species::Panda
            | Species::Parrot
            | Species::Pig
            | Species::PolarBear
            | Species::Rabbit
            | Species::Sheep
            | Species::Sniffer
            | Species::Strider
            | Species::Villager
            | Species::Wolf
    )
}

// ---- Behaviours ------------------------------------------------------------

/// `RunOne`: shuffles its children by weight and starts the first that can.
#[derive(Clone, Debug)]
struct Gate {
    running: bool,
    shuffle: bool,
    try_all: bool,
    random: LegacyRandom,
    kids: Vec<(B, i32, f64)>,
    /// The gate's memory requirements.
    when: Option<fn(&Memories) -> bool>,
}
impl Gate {
    fn run_one(seed: i64, kids: Vec<(B, i32)>) -> Self {
        Self {
            running: false,
            shuffle: true,
            try_all: false,
            random: LegacyRandom::new(seed),
            kids: kids.into_iter().map(|(b, w)| (b, w, 0.0)).collect(),
            when: None,
        }
    }
    fn when(mut self, f: fn(&Memories) -> bool) -> Self {
        self.when = Some(f);
        self
    }
}

#[derive(Clone, Debug)]
enum B {
    AxolotlAction {
        action: AxolotlAction,
        running: bool,
        until: i64,
    },
    PlayDead(Timed),
    Swim(Timed),
    /// `AnimalPanic`: flying (`AirAndWaterRandomPos`) or on land, and
    /// whether the sniffer's override resets its sniffing first.
    AnimalPanic {
        t: Timed,
        fly: bool,
        speed: f32,
    },
    MoveToTargetSink {
        t: Timed,
        cooldown: i32,
        path: Option<Path>,
        last_target: Option<[i32; 3]>,
        speed: f32,
    },
    CountDownTemptation(Timed),
    MakeLove {
        t: Timed,
        spawn_at: i64,
    },
    /// `FollowTemptation` and how close is close enough.
    FollowTemptation {
        t: Timed,
        look_in_the_eyes: bool,
    },
    LookAtTargetSink(Timed),
    FeelingHappy(Timed),
    Digging(Timed),
    FinishedDigging(Timed),
    Searching(Timed),
    Scenting(Timed),
    Sniffing(Timed),
    DoNothing(Timed),
    /// `SetWalkTargetFromLookTarget.create(speed, close)`.
    WalkToLookTarget {
        running: bool,
        speed: f32,
        close: i32,
    },
    /// `SetEntityLookTarget.create(EntityType.PLAYER, 6)`.
    LookAtPlayer(bool),
    /// `RandomStroll.stroll` or `RandomStroll.fly`.
    Stroll {
        running: bool,
        fly: bool,
        speed: f32,
        /// Horizontal and vertical range (land strolls).
        range: (i32, i32),
    },
    /// `BabyFollowAdult.create(3..16, 1.1, memory, true)`: the nearest
    /// visible player or adult.
    FollowAdult {
        running: bool,
        player: bool,
    },
    Gate(Box<Gate>),
    /// `CountDownCooldownTicks(GAZE_COOLDOWN_TICKS)` and
    /// `(TRANSPORT_ITEMS_COOLDOWN_TICKS)`.
    CountDownGaze(Timed),
    CountDownTransport(Timed),
    /// `SetEntityLookTargetSometimes.create(PLAYER, 6, 40..80)`: running,
    /// and the ticker's ticks until the next start.
    LookAtPlayerSometimes(bool, i32),
    /// `InteractWithDoor.create()`: opens the doors on the path and closes
    /// the ones it has passed.
    InteractWithDoor {
        running: bool,
        last_node: Option<PathNode>,
        cooldown: i32,
    },
    /// `TransportItemsBetweenContainers`.
    Transport(Box<super::transport_items::Transport>),
}

impl B {
    fn running(&self) -> bool {
        match self {
            B::PlayDead(t)
            | B::Swim(t)
            | B::AnimalPanic { t, .. }
            | B::MoveToTargetSink { t, .. }
            | B::CountDownTemptation(t)
            | B::MakeLove { t, .. }
            | B::FollowTemptation { t, .. }
            | B::LookAtTargetSink(t)
            | B::FeelingHappy(t)
            | B::Digging(t)
            | B::FinishedDigging(t)
            | B::Searching(t)
            | B::Scenting(t)
            | B::Sniffing(t)
            | B::DoNothing(t)
            | B::CountDownGaze(t)
            | B::CountDownTransport(t) => t.running,
            B::Transport(x) => x.running,
            B::LookAtPlayerSometimes(running, _) => *running,
            B::InteractWithDoor { running, .. } => *running,
            B::AxolotlAction { running, .. }
            | B::WalkToLookTarget { running, .. }
            | B::LookAtPlayer(running)
            | B::Stroll { running, .. }
            | B::FollowAdult { running, .. } => *running,            B::Gate(g) => g.running,
        }
    }
    /// Stopped without its stop effects (the memories they would touch are
    /// cleared with the rest).
    fn reset(&mut self) {
        match self {
            B::PlayDead(t)
            | B::Swim(t)
            | B::AnimalPanic { t, .. }
            | B::CountDownTemptation(t)
            | B::MakeLove { t, .. }
            | B::FollowTemptation { t, .. }
            | B::LookAtTargetSink(t)
            | B::FeelingHappy(t)
            | B::Digging(t)
            | B::FinishedDigging(t)
            | B::Searching(t)
            | B::Scenting(t)
            | B::Sniffing(t)
            | B::DoNothing(t)
            | B::CountDownGaze(t)
            | B::CountDownTransport(t) => t.running = false,
            B::Transport(x) => x.running = false,
            B::MoveToTargetSink { t, path, .. } => {
                t.running = false;
                *path = None;
            }
            B::Gate(g) => {
                g.running = false;
                for k in &mut g.kids {
                    k.0.reset();
                }
            }            _ => self.set_one_shot(false),
        }
    }
    /// A one-shot (`OneShot`) runs only until its first tick.
    fn one_shot(&self) -> bool {
        matches!(
            self,
            B::AxolotlAction { .. }
                | B::WalkToLookTarget { .. }
                | B::LookAtPlayer(_)
                | B::LookAtPlayerSometimes(..)
                | B::InteractWithDoor { .. }
                | B::Stroll { .. }
                | B::FollowAdult { .. }
        )
    }
    fn set_one_shot(&mut self, on: bool) {
        match self {
            B::AxolotlAction { running, .. }
            | B::WalkToLookTarget { running, .. }
            | B::LookAtPlayer(running)
            | B::LookAtPlayerSometimes(running, _)
            | B::InteractWithDoor { running, .. }
            | B::Stroll { running, .. }
            | B::FollowAdult { running, .. } => *running = on,
            _ => {}
        }
    }




}

fn same_path(a: Option<&Path>, b: Option<&Path>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.nodes == b.nodes && a.target == b.target,
        _ => false,
    }
}
fn dist_sqr(a: [i32; 3], b: [i32; 3]) -> i64 {
    let d = |i: usize| i64::from(a[i] - b[i]);
    d(0) * d(0) + d(1) * d(1) + d(2) * d(2)
}
fn manhattan(a: [i32; 3], b: [i32; 3]) -> i32 {
    (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs()
}
fn partner_panicking(_w: &MobWorld, _partner: Who) -> bool {
    false
}

// ---- The sniffer --------------------------------------------------------


/// `SnifferAi.getActivities`, in `availableBehaviorsByPriority` order: by
/// priority (`ActivityData.create(CORE, 0, ...)` numbers the core
/// behaviours 0, 1, 2...), then by the activity's bucket in the 16-bucket
/// `HashMap<Activity, ...>` (core 1, dig 3, idle 5, sniff 6, panic 15),
/// then in insertion order.
fn sniffer_package(seed: i64) -> Vec<Entry> {
    let e = |prio: i32, act: Act, b: B| Entry { prio, act, b };
    let mut l = vec![
        // Priority 0: core, dig, idle, sniff.
        e(0, Act::Core, B::Swim(Timed::new(60, 60))),
        e(0, Act::Dig, B::Digging(Timed::new(160, 180))),
        e(0, Act::Dig, B::FinishedDigging(Timed::new(40, 40))),
        e(
            0,
            Act::Idle,
            B::MakeLove {
                t: Timed::new(110, 110),
                spawn_at: 0,
            },
        ),
        e(0, Act::Sniff, B::Searching(Timed::new(600, 600))),
        // Priority 1.
        e(
            1,
            Act::Core,
            B::AnimalPanic {
                t: Timed::new(100, 120),
                fly: false,
                speed: 2.0,
            },
        ),
        e(
            1,
            Act::Idle,
            B::FollowTemptation {
                t: Timed::new(60, 60),
                look_in_the_eyes: false,
            },
        ),
        // Priority 2.
        e(2, Act::Core, move_to_target_sink(500, 700)),
        e(2, Act::Idle, B::LookAtTargetSink(Timed::new(45, 90))),
        // Priority 3.
        e(3, Act::Core, B::CountDownTemptation(Timed::new(60, 60))),
        e(3, Act::Idle, B::FeelingHappy(Timed::new(40, 100))),
    ];
    // Priority 4.
    l.push(e(
        4,
        Act::Idle,
        B::Gate(Box::new(Gate::run_one(
            seed,
            vec![
                (
                    B::WalkToLookTarget {
                        running: false,
                        speed: 1.0,
                        close: 3,
                    },
                    2,
                ),
                (B::Scenting(Timed::new(40, 80)), 1),
                (B::Sniffing(Timed::new(40, 80)), 1),
                (B::LookAtPlayer(false), 1),
                (
                    B::Stroll {
                        running: false,
                        fly: false,
                        speed: 1.0,
                        range: (10, 7),
                    },
                    1,
                ),
                (B::DoNothing(Timed::new(5, 20)), 2),
            ],
        ))),
    ));
    l
}
/// `HappyGhastAi.getActivities`, in `availableBehaviorsByPriority` order.
/// `CopperGolemAi.getActivities`, in tick order: the core behaviours take
/// priorities 0 to 5 in turn (`ActivityData.create(CORE, 0, ..)`), the idle
/// ones 0, 1 and 2.
fn copper_golem_package(seed: i64) -> Vec<Entry> {
    let e = |prio: i32, act: Act, b: B| Entry { prio, act, b };
    vec![
        // Priority 0.
        e(
            0,
            Act::Core,
            B::AnimalPanic {
                t: Timed::new(100, 120),
                fly: false,
                speed: 1.5,
            },
        ),
        e(0, Act::Idle, B::Transport(Box::default())),
        // Priority 1.
        e(1, Act::Core, B::LookAtTargetSink(Timed::new(45, 90))),
        e(1, Act::Idle, B::LookAtPlayerSometimes(false, 0)),
        // Priority 2: RunOne(stroll or do nothing) while there is no walk
        // target and the transport cools down.
        e(2, Act::Core, move_to_target_sink(150, 250)),
        e(
            2,
            Act::Idle,
            B::Gate(Box::new(
                Gate::run_one(
                    seed,
                    vec![
                        (
                            B::Stroll {
                                running: false,
                                fly: false,
                                speed: 1.0,
                                range: (2, 2),
                            },
                            1,
                        ),
                        (B::DoNothing(Timed::new(30, 60)), 1),
                    ],
                )
                .when(|mm| {
                    !mm.walk_target.present() && mm.transport_items_cooldown_ticks.present()
                }),
            )),
        ),
        // Priority 3.
        e(
            3,
            Act::Core,
            B::InteractWithDoor {
                running: false,
                last_node: None,
                cooldown: 0,
            },
        ),
        // Priorities 4 and 5.
        e(4, Act::Core, B::CountDownGaze(Timed::new(60, 60))),
        e(5, Act::Core, B::CountDownTransport(Timed::new(60, 60))),
    ]
}

fn ghastling_package(seed: i64) -> Vec<Entry> {
    let e = |prio: i32, act: Act, b: B| Entry { prio, act, b };
    vec![
        // Priority 0.
        e(0, Act::Core, B::Swim(Timed::new(60, 60))),
        // Priority 1.
        e(
            1,
            Act::Core,
            B::AnimalPanic {
                t: Timed::new(100, 120),
                fly: true,
                speed: 2.0,
            },
        ),
        e(
            1,
            Act::Idle,
            B::FollowTemptation {
                t: Timed::new(60, 60),
                look_in_the_eyes: true,
            },
        ),
        // Priority 2.
        e(2, Act::Core, B::LookAtTargetSink(Timed::new(45, 90))),
        e(
            2,
            Act::Idle,
            B::FollowAdult {
                running: false,
                player: true,
            },
        ),
        // Priority 3.
        e(3, Act::Core, move_to_target_sink(150, 250)),
        e(
            3,
            Act::Idle,
            B::FollowAdult {
                running: false,
                player: false,
            },
        ),
        // Priority 4.
        e(4, Act::Core, B::CountDownTemptation(Timed::new(60, 60))),
        e(
            4,
            Act::Idle,
            B::Gate(Box::new(Gate::run_one(
                seed,
                vec![
                    (
                        B::Stroll {
                            running: false,
                            fly: true,
                            speed: 1.0,
                            range: (10, 7),
                        },
                        1,
                    ),
                    (
                        B::WalkToLookTarget {
                            running: false,
                            speed: 1.0,
                            close: 3,
                        },
                        1,
                    ),
                ],
            ))),
        ),
    ]
}
fn move_to_target_sink(min: i32, max: i32) -> B {
    B::MoveToTargetSink {
        t: Timed::new(min, max),
        cooldown: 0,
        path: None,
        last_target: None,
        speed: 0.0,
    }
}

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
fn validate_play_dead(b: &mut Brain) -> bool {
    let Some(ticks) = b.mem.play_dead_ticks.value() else {
        return false;
    };
    if ticks <= 0 {
        b.mem.play_dead_ticks.clear();
        b.mem.hurt_by_entity.clear();
        b.activity = Act::Idle;
    } else {
        b.mem.play_dead_ticks.set(ticks - 1);
    }
    true
}
fn update_axolotl_activity(b: &mut Brain) {
    let old = b.activity;
    // PLAY_DEAD remains active until ValidatePlayDead resets to default IDLE.
    if old == Act::PlayDead {
        return;
    }
    b.activity = if b.mem.play_dead_ticks.present() {
        Act::PlayDead
    } else if b.mem.attack_target.present() {
        Act::Fight
    } else {
        Act::Idle
    };
    if old != b.activity && old == Act::Fight {
        b.mem
            .hunting_cooldown
            .set_ttl(true, super::axolotl::HUNTING_COOLDOWN);
        b.mem.attack_target.clear();
    }
}
/// `TadpoleAi`: the core activity (panic 2.0, the look and move sinks, the temptation
/// cooldown) and the idle one (look at a player now and then, follow frog food at 1.25,
/// then swim about, walk to what it looks at, or stay put in water).
fn tadpole_package(seed: i64) -> Vec<Entry> {
    use Act::*;
    use AxolotlAction::*;
    let e = |prio: i32, act, b| Entry { prio, act, b };
    let action = |action| B::AxolotlAction {
        action,
        running: false,
        until: 0,
    };
    vec![
        // Priority 0.
        e(
            0,
            Core,
            B::AnimalPanic {
                t: Timed::new(100, 120),
                fly: false,
                speed: 2.0,
            },
        ),
        e(0, Idle, B::LookAtPlayerSometimes(false, 0)),
        // Priority 1.
        e(1, Core, B::LookAtTargetSink(Timed::new(45, 90))),
        e(
            1,
            Idle,
            B::FollowTemptation {
                t: Timed::new(60, 60),
                look_in_the_eyes: false,
            },
        ),
        // Priority 2.
        e(2, Core, move_to_target_sink(150, 250)),
        e(
            2,
            Idle,
            B::Gate(Box::new(Gate {
                running: false,
                shuffle: false,
                try_all: true,
                random: horizon_core::random::LegacyRandom::new(seed),
                kids: vec![
                    (action(SwimStroll), 2, 0.0),
                    (action(TadpoleWalkFromLook), 3, 0.0),
                    (action(StayWater), 5, 0.0),
                ],
                when: Some(|mm| !mm.walk_target.present()),
            })),
        ),
        // Priority 3.
        e(3, Core, B::CountDownTemptation(Timed::new(60, 60))),
    ]
}

fn axolotl_package(seed: i64) -> Vec<Entry> {
    use Act::*;
    use AxolotlAction::*;
    let e = |prio: i32, act, b| Entry { prio, act, b };
    let action = |action| B::AxolotlAction {
        action,
        running: false,
        until: 0,
    };
    vec![
        // Native activity slots, merged by behavior priority.
        e(0, Core, B::LookAtTargetSink(Timed::new(45, 90))),
        e(0, Idle, B::LookAtPlayerSometimes(false, 0)),
        e(0, Fight, action(StopAttack)),
        e(0, PlayDead, B::PlayDead(Timed::new(200, 200))),
        e(1, Core, move_to_target_sink(150, 250)),
        e(
            1,
            Idle,
            B::MakeLove {
                t: Timed::new(110, 110),
                spawn_at: 0,
            },
        ),
        e(1, Fight, action(Chase)),
        e(1, PlayDead, action(EraseDeadBreeding)),
        e(2, Core, action(ValidateDead)),
        e(
            2,
            Idle,
            B::Gate(Box::new(Gate::run_one(
                seed,
                vec![
                    (
                        B::FollowTemptation {
                            t: Timed::new(60, 60),
                            look_in_the_eyes: false,
                        },
                        1,
                    ),
                    (
                        B::FollowAdult {
                            running: false,
                            player: false,
                        },
                        1,
                    ),
                ],
            ))),
        ),
        e(2, Fight, action(Bite)),
        e(3, Core, B::CountDownTemptation(Timed::new(60, 60))),
        e(3, Idle, action(StartAttack)),
        e(3, Idle, action(FindLiquid)),
        e(3, Fight, action(EraseAttackBreeding)),
        e(
            4,
            Idle,
            B::Gate(Box::new(Gate {
                running: false,
                shuffle: false,
                try_all: true,
                random: horizon_core::random::LegacyRandom::new(seed),
                kids: vec![
                    (action(SwimStroll), 2, 0.0),
                    (
                        B::Stroll {
                            running: false,
                            fly: false,
                            speed: 0.15,
                            range: (10, 7),
                        },
                        2,
                        0.0,
                    ),
                    (action(WalkFromLook), 3, 0.0),
                    (action(StayWater), 5, 0.0),
                    (action(StayGround), 5, 0.0),
                ],
                when: Some(|mm| !mm.walk_target.present()),
            })),
        ),
    ]
}

// ---- Debug output (the mob AI harness) ------------------------------------

fn pos(p: [i32; 3]) -> String {
    format!("{},{},{}", p[0], p[1], p[2])
}
fn who(w: Who) -> String {
    match w {
        Who::Player => "player".into(),
        Who::Mob(id) => format!("e{id}"),
    }
}
fn tracker(t: Tracker) -> String {
    match t {
        Tracker::Entity { who: e, .. } => who(e),
        Tracker::Block(p) => pos(p),
    }
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

/// Whether the block at `p` is a door mobs open (`MOB_INTERACTABLE_DOORS`),
/// and then whether it is open.
fn door_open(w: &MobWorld, p: [i32; 3]) -> Option<bool> {
    let name = w.block_name(p[0], p[1], p[2]);
    (name.ends_with("_door") && w.has_tag("minecraft:mob_interactable_doors", name))
        .then(|| w.prop(p[0], p[1], p[2], "open") == Some("true"))
}
