//! The goals of a cow as data and event-driven systems (suivi/ecs-buts.md). `GoalSet` is the goal selector as data
//! (priorities, MOVE/LOOK/JUMP flags, locks); a goal is woken by a deadline in the timer wheel or by an event
//! (damage, a player taking or leaving an item or coming in range, love, a parent that moves, water) and never
//! scans. Only a goal that runs ticks, through its own marker. No system here knows the species: the tempting
//! item, speeds, ranges and priorities are component values.
use crate::cow::*;
use bevy_ecs::{prelude::*, schedule::Schedule};
use horizon_core::geometry::{Vec3, trig};
use std::collections::HashMap;

pub const MOVE: u8 = 1;
pub const LOOK: u8 = 2;
pub const JUMP: u8 = 4;
const WHEEL_MAX: u64 = 4095;
/// The widest tempting range of any species (the index scan radius).
const SCAN: f64 = 10.0;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    Float,
    Panic,
    Breed,
    /// A tempt goal; a species may have several (the pig: the carrot on a stick, then its foods), by instance.
    Tempt(u8),
    FollowParent,
    Stroll,
    LookAtPlayer,
    RandomLook,
    /// `NearestAttackableTargetGoal` (the target selector, no flags): picks the player to hunt.
    Target,
    /// `MeleeAttackGoal`: chase the target and strike it.
    Melee,
    /// `EatBlockGoal` (the sheep): graze (`farm.rs`).
    EatBlock,
    /// The hen's egg deadline (`farm.rs`): a timer of the wheel, not a goal.
    LayEgg,
}
/// `GoalSelector`: (goal, priority, flags) in registration order; `running` and `pending` are slot bitmasks;
/// `lock[b]` is the slot holding flag `b`. A pending goal is blocked by a lock and starts when it frees.
#[derive(Component, Clone, Debug)]
pub struct GoalSet {
    slots: Vec<(Kind, u8, u8)>,
    running: u16,
    pending: u16,
    lock: [Option<u8>; 3],
}
impl GoalSet {
    pub fn new(slots: &[(Kind, u8, u8)]) -> Self {
        Self { slots: slots.to_vec(), running: 0, pending: 0, lock: [None; 3] }
    }
    fn at(&self, k: Kind) -> Option<usize> {
        self.slots.iter().position(|s| s.0 == k)
    }
    pub fn is_running(&self, k: Kind) -> bool {
        self.at(k).is_some_and(|i| self.running >> i & 1 == 1)
    }
    /// The goal holding the MOVE flag.
    pub fn mover(&self) -> Option<Kind> {
        self.lock[0].map(|i| self.slots[i as usize].0)
    }
    /// Every flag it needs is free or held by a goal of lower priority (a larger number).
    pub fn replaceable(&self, k: Kind) -> bool {
        let Some(i) = self.at(k) else { return false };
        let (_, p, f) = self.slots[i];
        (0..3).filter(|b| f >> b & 1 == 1).all(|b| self.lock[b].is_none_or(|j| p < self.slots[j as usize].1))
    }
    /// Starts `k`, taking its flags; returns the goals it stopped.
    pub fn start(&mut self, k: Kind) -> Vec<Kind> {
        let i = self.at(k).unwrap();
        let mut stopped = Vec::new();
        for b in 0..3 {
            if self.slots[i].2 >> b & 1 == 1 {
                if let Some(j) = self.lock[b] {
                    let g = self.slots[j as usize].0;
                    if !stopped.contains(&g) {
                        stopped.push(g);
                    }
                }
            }
        }
        for g in &stopped {
            self.stop(*g);
        }
        for b in 0..3 {
            if self.slots[i].2 >> b & 1 == 1 {
                self.lock[b] = Some(i as u8);
            }
        }
        self.running |= 1 << i;
        self.pending &= !(1 << i);
        stopped
    }
    pub fn stop(&mut self, k: Kind) -> bool {
        let Some(i) = self.at(k) else { return false };
        let was = self.running >> i & 1 == 1;
        self.running &= !(1 << i);
        for l in &mut self.lock {
            if *l == Some(i as u8) {
                *l = None;
            }
        }
        was
    }
}
/// What differs between the farm animals in `farm.rs::register`: the priorities, the speed modifiers, the tempting
/// items (data; the behaviours are the same systems).
#[derive(Clone, Copy, Debug)]
pub struct GoalDef {
    pub slots: &'static [(Kind, u8, u8)],
    /// `LookAtPlayerGoal`'s range.
    pub look: f64,
    pub panic: f64,
    /// The instances of the tempt goal, in registration order (slot `Kind::Tempt(i)` is `tempts[i]`).
    pub tempts: &'static [TemptDef],
    pub follow: f64,
}
/// One `TemptGoal`: its speed modifier and the items that tempt (a bit per item id: `WHEAT`, `CARROT`...).
#[derive(Clone, Copy, Debug)]
pub struct TemptDef {
    pub speed: f64,
    pub items: u32,
}
pub const WHEAT: u32 = 1;
pub const CARROT: u32 = 2;
pub const POTATO: u32 = 3;
pub const BEETROOT: u32 = 4;
pub const CARROT_ON_A_STICK: u32 = 5;
pub const WHEAT_SEEDS: u32 = 6;
pub const BEETROOT_SEEDS: u32 = 7;
pub const MELON_SEEDS: u32 = 8;
pub const PUMPKIN_SEEDS: u32 = 9;
/// The id of an item the players of the bench hold, from its name.
pub fn item_id(name: &str) -> u32 {
    match name {
        "minecraft:wheat" => WHEAT,
        "minecraft:carrot" => CARROT,
        "minecraft:potato" => POTATO,
        "minecraft:beetroot" => BEETROOT,
        "minecraft:carrot_on_a_stick" => CARROT_ON_A_STICK,
        "minecraft:wheat_seeds" => WHEAT_SEEDS,
        "minecraft:beetroot_seeds" => BEETROOT_SEEDS,
        "minecraft:melon_seeds" => MELON_SEEDS,
        "minecraft:pumpkin_seeds" => PUMPKIN_SEEDS,
        _ => 0,
    }
}
/// The phase of the goal selector of the entity: it asks `canUse` on the ticks where tick + phase is even (the id of
/// a `Mob`). `u64::MAX`: the entity's index.
#[derive(Component, Clone, Copy, Debug)]
pub struct Phase(pub u64);
const COW_SLOTS: [(Kind, u8, u8); 8] = [
    (Kind::Float, 0, JUMP),
    (Kind::Panic, 1, MOVE),
    (Kind::Breed, 2, MOVE | LOOK),
    (Kind::Tempt(0), 3, MOVE | LOOK),
    (Kind::FollowParent, 4, 0),
    (Kind::Stroll, 5, MOVE),
    (Kind::LookAtPlayer, 6, LOOK),
    (Kind::RandomLook, 7, MOVE | LOOK),
];
const PIG_SLOTS: [(Kind, u8, u8); 9] = [
    (Kind::Float, 0, JUMP),
    (Kind::Panic, 1, MOVE),
    (Kind::Breed, 3, MOVE | LOOK),
    (Kind::Tempt(0), 4, MOVE | LOOK),
    (Kind::Tempt(1), 4, MOVE | LOOK),
    (Kind::FollowParent, 5, 0),
    (Kind::Stroll, 6, MOVE),
    (Kind::LookAtPlayer, 7, LOOK),
    (Kind::RandomLook, 8, MOVE | LOOK),
];
const ZOMBIE_SLOTS: [(Kind, u8, u8); 5] = [
    (Kind::Target, 2, 0),
    (Kind::Melee, 3, MOVE | LOOK),
    (Kind::Stroll, 7, MOVE),
    (Kind::LookAtPlayer, 8, LOOK),
    (Kind::RandomLook, 8, MOVE | LOOK),
];
const SHEEP_SLOTS: [(Kind, u8, u8); 9] = [
    (Kind::Float, 0, JUMP),
    (Kind::Panic, 1, MOVE),
    (Kind::Breed, 2, MOVE | LOOK),
    (Kind::Tempt(0), 3, MOVE | LOOK),
    (Kind::FollowParent, 4, 0),
    (Kind::EatBlock, 5, MOVE | LOOK | JUMP),
    (Kind::Stroll, 6, MOVE),
    (Kind::LookAtPlayer, 7, LOOK),
    (Kind::RandomLook, 8, MOVE | LOOK),
];
const SHEEP_TEMPTS: [TemptDef; 1] = [TemptDef { speed: 1.1, items: 1 << WHEAT }];
const CHICKEN_SLOTS: [(Kind, u8, u8); 8] = COW_SLOTS;
const CHICKEN_TEMPTS: [TemptDef; 1] = [TemptDef { speed: 1.0, items: 1 << WHEAT_SEEDS | 1 << BEETROOT_SEEDS | 1 << MELON_SEEDS | 1 << PUMPKIN_SEEDS }];
const COW_TEMPTS: [TemptDef; 1] = [TemptDef { speed: 1.25, items: 1 << WHEAT }];
const PIG_TEMPTS: [TemptDef; 2] = [
    TemptDef { speed: 1.2, items: 1 << CARROT_ON_A_STICK },
    TemptDef { speed: 1.2, items: 1 << CARROT | 1 << POTATO | 1 << BEETROOT },
];
/// The goals of a farm animal by its type (`Cow.registerGoals`, `Pig.registerGoals`, in `farm.rs`). The pig registers
/// two tempt goals of one priority (the carrot on a stick, then its foods), each with its own calm-down.
pub fn goal_def(species: &str) -> GoalDef {
    match species {
        "minecraft:pig" => GoalDef { slots: &PIG_SLOTS, look: 6.0, panic: 1.25, tempts: &PIG_TEMPTS, follow: 1.1 },
        // `Zombie.registerGoals`: melee 3, stroll 7, look 8, 8; the target selector's nearest player.
        "minecraft:sheep" => GoalDef { slots: &SHEEP_SLOTS, look: 6.0, panic: 1.25, tempts: &SHEEP_TEMPTS, follow: 1.1 },
        "minecraft:chicken" => GoalDef { slots: &CHICKEN_SLOTS, look: 6.0, panic: 1.4, tempts: &CHICKEN_TEMPTS, follow: 1.1 },
        "minecraft:zombie" => GoalDef { slots: &ZOMBIE_SLOTS, look: 8.0, panic: 0.0, tempts: &[], follow: 0.0 },
        _ => GoalDef { slots: &COW_SLOTS, look: 6.0, panic: 2.0, tempts: &COW_TEMPTS, follow: 1.25 },
    }
}
/// The goals of the cow (the bench's default species), for the unit tests of the selector.
pub fn cow_slots() -> Vec<(Kind, u8, u8)> {
    goal_def("minecraft:cow").slots.to_vec()
}

// ---- species data and state: components, data only --------------------------------------------------------
/// `PanicGoal`: the speed modifier, the spot's range, and the end of the window after the last damage (state).
#[derive(Component, Clone, Copy, Debug)]
pub struct Panic {
    pub speed: f64,
    pub h: i32,
    pub v: i32,
    pub window: u64,
    pub until: u64,
}
/// `TemptGoal`: the items, speed, stop distance, range, the look speed, and its state (one per instance).
#[derive(Clone, Copy, Debug)]
pub struct Tempt {
    /// The items that tempt, a bit per item id.
    pub items: u32,
    pub speed: f64,
    pub stop: f64,
    pub range: f64,
    pub look_y: f32,
    pub calm: u64,
    pub calm_until: u64,
    /// The tick since which the calm-down stands still because the goal cannot be asked (0: it counts).
    pub paused_at: u64,
    pub player: u32,
    pub aim: (i32, i32),
}
/// The tempt goals of the animal (one instance each: its own items, speed, calm-down and player).
#[derive(Component, Clone, Debug)]
pub struct Tempts(pub Vec<Tempt>);
/// `BreedGoal` and the animal's breeding times.
#[derive(Component, Clone, Copy, Debug)]
pub struct BreedData {
    pub speed: f64,
    pub range: f64,
    pub delay: u64,
    pub love: u64,
    pub rest: u64,
    pub baby_age: u64,
}
/// In love until this tick; resting (age 6000) until this tick.
#[derive(Component, Clone, Copy, Debug)]
pub struct Love(pub u64);
#[derive(Component, Clone, Copy, Debug)]
pub struct Resting(pub u64);
#[derive(Component, Clone, Copy, Debug)]
pub struct FollowParent {
    pub speed: f64,
    pub recalc: u64,
}
/// The adult a baby was born to: it wakes the baby's follow-parent goal when it moves.
#[derive(Component, Clone, Debug)]
pub struct Mother(pub Vec<Entity>);
/// `LookAtPlayerGoal`: range, ask period (1 chance in `period` per ask, one ask every 2 ticks), state `Glancing`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Glance {
    pub range: f64,
    pub period: i32,
}
/// `RandomLookAroundGoal`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Scan {
    pub period: i32,
}
/// Which timers are in the wheel.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct GoalTimers {
    pub look: bool,
    pub random: bool,
    pub target: bool,
    pub graze: bool,
}
/// `LookControl`: where to look, how fast, the eye height, the head's limits.
#[derive(Component, Clone, Copy, Debug)]
pub struct Look {
    pub y_default: f32,
    pub target: [f64; 3],
    pub cooldown: u8,
    pub y_speed: f32,
    pub x_max: f32,
    pub max_head_y: f32,
    pub eye: f64,
}
/// The head and the body (`yBodyRot`) and the state of `BodyRotationControl`; `yaw` is NaN until the head first moves.
#[derive(Component, Clone, Copy, Debug)]
pub struct Head {
    pub yaw: f32,
    pub pitch: f32,
    pub body: f32,
    pub stable: f32,
    pub stable_time: i32,
    pub at: (f64, f64),
}
/// `Mob.getFluidJumpThreshold`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Buoyancy(pub f64);
/// The jump the goals ask for this tick (the physics consumes it).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Jump(pub bool);

// ---- run markers: a goal that runs is a marker its system visits ------------------------------------------
macro_rules! markers { ($($n:ident),*) => { $(#[derive(Component, Clone, Copy, Debug)] #[component(storage = "SparseSet")] pub struct $n;)* } }
markers!(Floating, Panicking, Looking);
/// A tempt goal runs: which instance.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Tempting(pub u8);
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Breeding {
    pub partner: Entity,
    pub since: u64,
    pub aim: (i32, i32),
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Trailing {
    pub parent: Entity,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Glancing {
    pub player: u32,
    pub until: u64,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Scanning {
    pub dx: f64,
    pub dz: f64,
    pub until: u64,
}

/// The data of the goals of an animal of the species (the vanilla values of its `registerGoals`, `Animal` and `Mob`).
pub fn goals_bundle(sp: &Species) -> impl Bundle {
    let d = sp.goals;
    (
        GoalSet::new(d.slots),
        Panic { speed: d.panic, h: 5, v: 4, window: 40, until: 0 },
        Tempts(d.tempts.iter().map(|t| Tempt { items: t.items, speed: t.speed, stop: 2.5, range: 10.0, look_y: 95.0, calm: 100, calm_until: 0, paused_at: 0, player: 0, aim: (i32::MIN, 0) }).collect()),
        BreedData { speed: 1.0, range: 8.0, delay: 60, love: 600, rest: 6000, baby_age: 24000 },
        FollowParent { speed: d.follow, recalc: 0 },
        Glance { range: d.look, period: 50 },
        Scan { period: 50 },
        GoalTimers::default(),
        Look { target: [0.0; 3], cooldown: 0, y_default: 10.0, y_speed: 10.0, x_max: 40.0, max_head_y: 75.0, eye: sp.eye_height },
        Head { yaw: f32::NAN, pitch: 0.0, body: 0.0, stable: 0.0, stable_time: 0, at: (0.0, 0.0) },
        Buoyancy(if sp.eye_height < 0.4 { 0.0 } else { 0.4 }),
        Jump(false),
        Phase(u64::MAX),
    )
}

// ---- resources: the events, as data ------------------------------------------------------------------------
#[derive(Resource, Default)]
pub struct Fired(pub Vec<(Entity, Kind)>);
#[derive(Resource, Default)]
pub struct Entered(pub Vec<Entity>);
#[derive(Resource, Default)]
pub struct Hurts(pub Vec<Entity>);
pub enum Req {
    /// The target changed (a hit, an acquisition): the target goal runs and the melee goal looks at its next check.
    Hunt(Entity),
    Arm(Entity),
    Start(Entity, Kind),
    End(Entity, Kind),
}
#[derive(Resource, Default)]
pub struct Requests(pub Vec<Req>);
/// The option of the lazy look: when on, the look goals (`LookAtPlayer`, `RandomLookAround`) and the look control run
/// only for an animal that a player can see, i.e. within `range` blocks, the client tracking range of an animal
/// (10 chunks = 160 blocks; the client renders an entity at most 64 x 2.5 = 160 blocks away), rounded out to 16-block
/// bins. Off by default: the owner decides.
#[derive(Resource)]
pub struct LazyLook {
    pub enabled: bool,
    pub range: f64,
    /// How many players watch each 16-block bin.
    counts: HashMap<(i32, i32), u32>,
    /// The bin each player was last counted in.
    at: HashMap<u32, (i32, i32)>,
}
impl Default for LazyLook {
    fn default() -> Self {
        Self { enabled: false, range: 160.0, counts: HashMap::new(), at: HashMap::new() }
    }
}
fn bin16(x: f64, z: f64) -> (i32, i32) {
    ((x / 16.0).floor() as i32, (z / 16.0).floor() as i32)
}
impl LazyLook {
    /// Whether a player can see an animal at (x, z); always true with the option off.
    pub fn watched(&self, x: f64, z: f64) -> bool {
        !self.enabled || self.counts.get(&bin16(x, z)).is_some_and(|&n| n > 0)
    }
    /// A player is now at `to` (None: gone); returns the bins that went from unwatched to watched.
    fn moved(&mut self, id: u32, to: Option<(f64, f64)>) -> Vec<(i32, i32)> {
        let reach = (self.range / 16.0).ceil() as i32;
        let new = to.map(|(x, z)| bin16(x, z));
        let old = self.at.get(&id).copied();
        if old == new {
            return Vec::new();
        }
        if let Some(o) = old {
            for bz in o.1 - reach..=o.1 + reach {
                for bx in o.0 - reach..=o.0 + reach {
                    if let Some(c) = self.counts.get_mut(&(bx, bz)) {
                        *c -= 1;
                    }
                }
            }
            self.at.remove(&id);
        }
        let mut fresh = Vec::new();
        if let Some(n) = new {
            for bz in n.1 - reach..=n.1 + reach {
                for bx in n.0 - reach..=n.0 + reach {
                    let c = self.counts.entry((bx, bz)).or_insert(0);
                    *c += 1;
                    if *c == 1 {
                        fresh.push((bx, bz));
                    }
                }
            }
            self.at.insert(id, n);
        }
        fresh
    }
}
/// Switches the lazy look (see `LazyLook`) on or off in a world.
pub fn set_lazy_look(world: &mut World, on: bool) {
    world.resource_mut::<LazyLook>().enabled = on;
    if !on {
        arm_all(world);
    }
}
/// Imposed random draws, for comparing with a `Mob` that drew its own: the spot (x, z) a goal of this cow takes next
/// instead of drawing one. Empty outside the tests.
#[derive(Resource, Default)]
pub struct Forced {
    pub spots: HashMap<(Entity, Kind), (f64, f64)>,
    /// The partner a cow in love takes instead of the nearest one.
    pub partners: HashMap<Entity, Entity>,
}

#[derive(Clone, Copy, Debug)]
pub struct Player {
    pub id: u32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub main: u32,
    pub off: u32,
}
impl Player {
    pub fn holds(&self, item: u32) -> bool {
        item != 0 && (self.main == item || self.off == item)
    }
    /// Whether a hand holds one of the items of the mask (a bit per item id).
    pub fn holds_any(&self, items: u32) -> bool {
        let has = |i: u32| i != 0 && i < 32 && items >> i & 1 == 1;
        has(self.main) || has(self.off)
    }
    pub fn eye(&self) -> f64 {
        self.y + 1.62
    }
}
fn bin(x: f64, z: f64) -> (i32, i32) {
    ((x / 8.0).floor() as i32, (z / 8.0).floor() as i32)
}
/// The players of the world, in 8-block bins; every change is an event (old, new) read by `online_events`.
#[derive(Resource, Default)]
pub struct Online {
    list: Vec<Option<Player>>,
    bins: HashMap<(i32, i32), Vec<u32>>,
    events: Vec<(Option<Player>, Option<Player>)>,
}
impl Online {
    pub fn set(&mut self, p: Player) {
        let id = p.id as usize;
        if self.list.len() <= id {
            self.list.resize(id + 1, None);
        }
        let old = self.list[id];
        match old {
            Some(o) if bin(o.x, o.z) == bin(p.x, p.z) => {}
            _ => {
                if let Some(o) = old {
                    self.unbin(o);
                }
                self.bins.entry(bin(p.x, p.z)).or_default().push(p.id);
            }
        }
        self.list[id] = Some(p);
        self.events.push((old, Some(p)));
    }
    pub fn remove(&mut self, id: u32) {
        if let Some(o) = self.list.get_mut(id as usize).and_then(Option::take) {
            self.unbin(o);
            self.events.push((Some(o), None));
        }
    }
    fn unbin(&mut self, o: Player) {
        if let Some(v) = self.bins.get_mut(&bin(o.x, o.z)) {
            v.retain(|&i| i != o.id);
        }
    }
    pub fn get(&self, id: u32) -> Option<Player> {
        self.list.get(id as usize).copied().flatten()
    }
    pub fn for_near(&self, x: f64, z: f64, r: f64, mut f: impl FnMut(Player)) {
        let ((x0, z0), (x1, z1)) = (bin(x - r, z - r), bin(x + r, z + r));
        for bz in z0..=z1 {
            for bx in x0..=x1 {
                for &id in self.bins.get(&(bx, bz)).into_iter().flatten() {
                    if let Some(p) = self.get(id) {
                        f(p);
                    }
                }
            }
        }
    }
}

/// Registers the resources, the systems; call from `Herd::new` / `schedule()`.
pub fn insert_resources(world: &mut World) {
    world.insert_resource(Fired::default());
    world.insert_resource(Entered::default());
    world.insert_resource(Hurts::default());
    world.insert_resource(Requests::default());
    world.insert_resource(Forced::default());
    world.insert_resource(LazyLook::default());
    world.insert_resource(crate::farm::Grown::default());
    world.insert_resource(crate::farm::LaidEggs::default());
    world.insert_resource(Online::default());
    world.insert_resource(CellMoves::default());
    world.insert_resource(Babies::default());
}
pub fn register(s: &mut Schedule) {
    s.add_systems(online_events.after(due).before(player_near));
    s.add_systems(hurt_events.after(apply_damage));
    s.add_systems(
        (entered_events, mother_moved, cell_moves, tempt_run, breed_run, follow_parent_run, glance_run, scan_run, float_events, float_run, arbitrate, look_control)
            .chain()
            .after(online_events)
            .after(hurt_events)
            .before(pathfind),
    );
}

// ---- events -------------------------------------------------------------------------------------------------
/// Damage that makes the cow panic (`DamageType.causes_panic`): call from the damage code.
pub fn hurt(world: &mut World, e: Entity) {
    world.resource_mut::<Hurts>().0.push(e);
}
fn floats(w: &World, e: Entity) -> bool {
    let (f, b) = (w.get::<FluidState>(e).unwrap(), w.get::<Buoyancy>(e).unwrap());
    f.water > b.0 || f.in_lava
}
/// `Animal.setInLove` by feeding: refused for a baby, a resting or a loving cow. Wakes the goal, and the loving
/// cows around (their partner just appeared).
pub fn feed(world: &mut World, e: Entity) -> bool {
    let now = world.resource::<Clock>().0;
    if world.get::<GrowAt>(e).is_some()
        || world.get::<Resting>(e).is_some_and(|r| r.0 > now)
        || world.get::<Love>(e).is_some_and(|l| l.0 > now)
    {
        return false;
    }
    // An animal that died is not fed.
    let Some(b) = world.get::<BreedData>(e).copied() else { return false };
    world.entity_mut(e).insert(Love(now + b.love));
    let p = world.get::<Pos>(e).unwrap().0;
    let mut near = Vec::new();
    world.resource::<Index>().for_radius(p.x, p.z, b.range, |c| near.push(c));
    let mut reqs = vec![Req::Start(e, Kind::Breed)];
    for c in near {
        if c != e && world.get::<Love>(c).is_some_and(|l| l.0 > now) {
            reqs.push(Req::Start(c, Kind::Breed));
        }
    }
    world.resource_mut::<Requests>().0.extend(reqs);
    true
}
/// `parent` is watched for `baby`: its moves wake the baby's follow-parent goal (an event on the adult that was
/// the nearest when the baby last looked, not a scan of the herd).
fn watch(world: &mut World, parent: Entity, baby: Entity) {
    match world.get_mut::<Mother>(parent) {
        Some(mut m) => {
            if !m.0.contains(&baby) {
                m.0.push(baby);
            }
        }
        None => {
            world.entity_mut(parent).insert(Mother(vec![baby]));
        }
    }
}
/// Makes `parent` the mother of `baby` and wakes the baby.
pub fn adopt(world: &mut World, parent: Entity, baby: Entity) {
    watch(world, parent, baby);
    world.resource_mut::<Requests>().0.push(Req::Start(baby, Kind::FollowParent));
}
/// A baby of `ticks` ticks of age left.
pub fn make_baby(world: &mut World, e: Entity, ticks: u64) {
    let now = world.resource::<Clock>().0;
    let sizes = *world.get::<Sizes>(e).unwrap();
    let p = world.get::<Pos>(e).unwrap().0;
    let mut b = world.get_mut::<Body>(e).unwrap();
    b.width = sizes.baby.0;
    b.height = sizes.baby.1;
    world.get_mut::<Bounds>(e).unwrap().0 = box_at(p, sizes.baby.0, sizes.baby.1);
    world.entity_mut(e).insert(GrowAt(now + ticks));
    world.resource_mut::<Babies>().0.push(e);
    world.resource_mut::<Requests>().0.push(Req::Start(e, Kind::FollowParent));
    world.resource_mut::<Wheel>().push((now + ticks).min(now + WHEEL_MAX), e, Due::Grow);
}

pub(crate) fn with_rng<T>(w: &mut World, e: Entity, f: impl FnOnce(&mut Rng) -> T) -> T {
    f(&mut w.get_mut::<Rng>(e).unwrap())
}
pub(crate) fn arm(w: &mut World, e: Entity, k: Kind, wait: i32) {
    let now = w.resource::<Clock>().0;
    w.resource_mut::<Wheel>().push((now + wait.max(1) as u64).min(now + WHEEL_MAX), e, Due::Goal(k));
}
/// Arms the look timers of a cow that has none (1 chance in `period` per ask, one ask every 2 ticks).
fn arm_timers(w: &mut World, e: Entity) {
    let Some(mut t) = w.get::<GoalTimers>(e).copied() else { return };
    let (g, s) = (*w.get::<Glance>(e).unwrap(), *w.get::<Scan>(e).unwrap());
    let seen = {
        let p = w.get::<Pos>(e).unwrap().0;
        w.resource::<LazyLook>().watched(p.x, p.z)
    };
    if !t.look && seen {
        let wait = with_rng(w, e, |r| 2 * geometric(r, g.period));
        arm(w, e, Kind::LookAtPlayer, wait);
        t.look = true;
    }
    if !t.random && seen {
        let wait = with_rng(w, e, |r| 2 * geometric(r, s.period));
        arm(w, e, Kind::RandomLook, wait);
        t.random = true;
    }
    // The target selector asks every other tick, 1 in `reduced(10)` = 5.
    if !t.target && w.get::<crate::hostile::Attack>(e).is_some() {
        let wait = with_rng(w, e, |r| 2 * geometric(r, 5));
        arm(w, e, Kind::Target, wait);
        t.target = true;
    }
    *w.get_mut::<GoalTimers>(e).unwrap() = t;
    if !t.graze && w.get::<crate::farm::Graze>(e).is_some() {
        crate::farm::arm_graze(w, e);
    }
}
fn arm_stroll_now(w: &mut World, e: Entity) {
    let mut s = *w.get::<Stroll>(e).unwrap();
    if s.scheduled {
        return;
    }
    let wait = with_rng(w, e, |r| idle_wait(r, s.interval));
    arm(w, e, Kind::Stroll, wait);
    s.scheduled = true;
    *w.get_mut::<Stroll>(e).unwrap() = s;
}
/// Arms the timers of every cow (the herd starts active).
pub fn arm_all(world: &mut World) {
    let ids: Vec<Entity> = world.query_filtered::<Entity, With<GoalTimers>>().iter(world).collect();
    for e in ids {
        arm_timers(world, e);
    }
}
fn entered_events(mut en: ResMut<Entered>, mut reqs: ResMut<Requests>) {
    if !en.0.is_empty() {
        reqs.0.extend(std::mem::take(&mut en.0).into_iter().map(Req::Arm));
    }
}
/// `PanicGoal.canUse` after a damage: only the types of `#panic_causes` (a fall, a drowning, a wall do not), then a
/// window of 40 ticks; a burning cow panics as long as it burns.
pub(crate) fn hurt_events(
    clock: Res<Clock>,
    mut h: ResMut<Hurts>,
    causes: Res<HurtCauses>,
    mut q: Query<&mut Panic>,
    mut reqs: ResMut<Requests>,
) {
    for (i, e) in std::mem::take(&mut h.0).into_iter().enumerate() {
        let panics = causes.0.get(i).filter(|c| c.0 == e).is_none_or(|c| !matches!(c.1, Cause::Fall | Cause::Drown | Cause::InWall));
        if let Ok(mut p) = q.get_mut(e)
            && panics
        {
            p.until = clock.0 + p.window;
            reqs.0.push(Req::Start(e, Kind::Panic));
        }
    }
}
/// A player took an item, put it away, came or went: the cows it newly draws are woken (those that were already in
/// range, and those whose goal runs, notice by themselves).
fn online_events(
    clock: Res<Clock>,
    mut on: ResMut<Online>,
    mut pl: ResMut<Players>,
    idx: Res<Index>,
    cows: Query<(&Pos, &Tempts)>,
    mut lazy: ResMut<LazyLook>,
    mut reqs: ResMut<Requests>,
) {
    if on.events.is_empty() {
        return;
    }
    let events = std::mem::take(&mut on.events);
    // The animals that a player now watches wake their look timers (the head state is made again).
    if lazy.enabled {
        for (old, new) in &events {
            let id = new.or(*old).map(|p| p.id).unwrap();
            for (bx, bz) in lazy.moved(id, new.map(|p| (p.x, p.z))) {
                let (x, z) = (f64::from(bx) * 16.0 + 8.0, f64::from(bz) * 16.0 + 8.0);
                idx.for_radius(x, z, 12.0, |c| {
                    if let Ok((p, _)) = cows.get(c)
                        && bin16(p.0.x, p.0.z) == (bx, bz)
                    {
                        reqs.0.push(Req::Arm(c));
                    }
                });
            }
        }
    }
    pl.list = on.list.iter().flatten().map(|p| (p.x, p.z)).collect();
    pl.dirty = true;
    let d2 = |p: &Player, c: Vec3| (p.x - c.x).powi(2) + (p.y - c.y).powi(2) + (p.z - c.z).powi(2);
    for (old, new) in events {
        let Some(n) = new else { continue };
        idx.for_radius(n.x, n.z, SCAN, |c| {
            let Ok((pos, ts)) = cows.get(c) else { return };
            for (i, t) in ts.0.iter().enumerate() {
                let reach = t.range.max(2.0).powi(2);
                let was = old.is_some_and(|o| o.holds_any(t.items) && d2(&o, pos.0) <= reach);
                if clock.0 >= t.calm_until && n.holds_any(t.items) && d2(&n, pos.0) <= reach && !was {
                    reqs.0.push(Req::Start(c, Kind::Tempt(i as u8)));
                }
            }
        });
    }
}
/// A mother moved: her babies that are 3 blocks or more away may follow her.
fn mother_moved(
    mut mothers: Query<(Entity, &Pos, &mut Mother), Changed<Bounds>>,
    babies: Query<(&Pos, Has<GrowAt>)>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    for (e, pos, mut m) in &mut mothers {
        m.0.retain(|&b| babies.get(b).is_ok_and(|(_, baby)| baby));
        for &b in &m.0 {
            let bp = babies.get(b).unwrap().0;
            if (bp.0.x - pos.0.x).powi(2) + (bp.0.z - pos.0.z).powi(2) >= 9.0 {
                reqs.0.push(Req::Start(b, Kind::FollowParent));
            }
        }
        if m.0.is_empty() {
            cmd.entity(e).remove::<Mother>();
        }
    }
}
/// The cows that changed column of the spatial index (written by `reindex`) and the babies of the herd.
#[derive(Resource, Default)]
pub struct CellMoves(pub Vec<Entity>);
#[derive(Resource, Default)]
pub struct Babies(pub Vec<Entity>);
/// `FollowParentGoal.canUse` looks for the nearest adult of the species within 8 x 4 x 8: a baby is woken when an
/// adult comes within 9 blocks of it (an adult that changed column), or when it changes column itself.
fn cell_moves(
    mut moves: ResMut<CellMoves>,
    mut babies: ResMut<Babies>,
    pos: Query<(&Pos, Has<GrowAt>)>,
    timers: Query<(&Pos, &GoalTimers)>,
    lazy: Res<LazyLook>,
    mut reqs: ResMut<Requests>,
) {
    let moved = std::mem::take(&mut moves.0);
    if moved.is_empty() {
        return;
    }
    // An animal that walks into a watched bin takes its look timers again.
    if lazy.enabled {
        for &m in &moved {
            if let Ok((p, t)) = timers.get(m)
                && (!t.look || !t.random)
                && lazy.watched(p.0.x, p.0.z)
            {
                reqs.0.push(Req::Arm(m));
            }
        }
    }
    babies.0.retain(|&b| pos.get(b).is_ok_and(|(_, baby)| baby));
    for &b in &babies.0 {
        let bp = pos.get(b).unwrap().0.0;
        for &m in &moved {
            let Ok((mp, _)) = pos.get(m) else { continue };
            if m == b || (mp.0.x - bp.x).abs() <= 9.0 && (mp.0.z - bp.z).abs() <= 9.0 {
                reqs.0.push(Req::Start(b, Kind::FollowParent));
            }
        }
    }
}
/// The path a goal asked for ended (or none was found): the goal that owns it ends (called by `cow::path_ended`).
pub fn path_ended(e: Entity, g: &GoalSet, reqs: &mut Requests) {
    if let Some(k @ (Kind::Stroll | Kind::Panic | Kind::Melee)) = g.mover() {
        reqs.0.push(Req::End(e, k));
    }
}

pub(crate) fn go_to(cmd: &mut Commands, e: Entity, x: f64, z: f64, speed: f64) {
    cmd.queue(move |w: &mut World| {
        if let Some(mut s) = w.get_mut::<Stroll>(e) {
            s.tx = x;
            s.tz = z;
            s.ty = 1;
        }
        if let Some(mut n) = w.get_mut::<Nav>(e) {
            n.speed = speed;
        }
        w.entity_mut(e).insert(NeedsPath);
    });
}
pub(crate) fn stop_nav(w: &mut World, e: Entity) {
    if let Some(mut n) = w.get_mut::<Nav>(e) {
        n.path = None;
    }
    if let Some(mut d) = w.get_mut::<Drive>(e) {
        d.zza = 0.0;
        d.jump = false;
    }
    if let Some(mut m) = w.get_mut::<MoveTo>(e) {
        m.active = false;
    }
    w.entity_mut(e).remove::<(Following, NeedsPath)>();
}
pub(crate) fn aim(cmd: &mut Commands, e: Entity, look: &mut Look, looking: bool, at: [f64; 3], y_speed: Option<f32>) {
    look.target = at;
    look.cooldown = 2;
    look.y_speed = y_speed.unwrap_or(look.y_default);
    if !looking {
        cmd.entity(e).insert(Looking);
    }
}
pub(crate) fn d2(a: Vec3, b: Vec3) -> f64 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)
}

// ---- running goals: the only things that tick -----------------------------------------------------------------
pub(crate) type Aim<'a> = (Entity, &'a Pos, &'a mut Look, Has<Looking>, Has<Following>, Has<NeedsPath>);
/// `TemptGoal.canContinue` and `tick`: look at the player, stop near it, walk to it when its block changed.
fn tempt_run(
    clock: Res<Clock>,
    on: Res<Online>,
    mut q: Query<(Aim, &mut Tempts, &Tempting)>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    for ((e, pos, mut look, looking, following, asked), mut ts, inst) in &mut q {
        let Some(t) = ts.0.get_mut(inst.0 as usize) else { continue };
        let max_d = t.range.max(2.0).powi(2);
        let p = on.get(t.player).filter(|p| p.holds_any(t.items) && d2(Vec3::new(p.x, p.y, p.z), pos.0) <= max_d);
        let Some(p) = p else {
            reqs.0.push(Req::End(e, Kind::Tempt(inst.0)));
            continue;
        };
        aim(&mut cmd, e, &mut look, looking, [p.x, p.eye(), p.z], Some(t.look_y));
        if d2(Vec3::new(p.x, p.y, p.z), pos.0) < t.stop * t.stop {
            if following || asked || t.aim.0 != i32::MIN {
                t.aim = (i32::MIN, 0);
                cmd.queue(move |w: &mut World| stop_nav(w, e));
            }
        } else {
            let a = (p.x.floor() as i32, p.z.floor() as i32);
            if a != t.aim || (!following && !asked) {
                t.aim = a;
                go_to(&mut cmd, e, p.x, p.z, t.speed);
            }
        }
        let _ = clock.0;
    }
}
/// `BreedGoal.canContinue` and `tick`: walk to the partner, and after the delay, if close, give birth.
#[allow(clippy::type_complexity)]
fn breed_run(
    clock: Res<Clock>,
    mut q: Query<(Aim, &mut Breeding, &BreedData, Option<&Love>, Option<&Phase>)>,
    partners: Query<(&Pos, Option<&Love>, Has<Panicking>)>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    let now = clock.0;
    for ((e, pos, mut look, looking, following, asked), mut b, data, love, phase) in &mut q {
        let partner = partners.get(b.partner).ok().filter(|(_, l, panicking)| l.is_some_and(|l| l.0 > now) && !panicking);
        let (Some((pp, _, _)), true) = (partner, love.is_some_and(|l| l.0 > now)) else {
            reqs.0.push(Req::End(e, Kind::Breed));
            continue;
        };
        let at = [pp.0.x, pp.0.y + look.eye, pp.0.z];
        let d = d2(pos.0, pp.0);
        aim(&mut cmd, e, &mut look, looking, at, None);
        let a = (pp.0.x.floor() as i32, pp.0.z.floor() as i32);
        if a != b.aim || (!following && !asked && d >= 9.0) {
            b.aim = a;
            go_to(&mut cmd, e, pp.0.x, pp.0.z, data.speed);
        }
        // `BreedGoal` counts `loveTime` once per selector tick (its `tick` runs then): it breeds when the count reaches
        // `reducedTickDelay(60)` and the partner is within 3 blocks, and gives up at 60.
        let phase = phase.map_or(0, |p| if p.0 == u64::MAX { e.index().index() as u64 } else { p.0 });
        let selector = |t: u64| (t + phase) % 2 == 0;
        let first = if selector(b.since) { b.since } else { b.since + 1 };
        let love_time = if now >= first { (now - first) / 2 + 1 } else { 0 };
        if selector(now) && love_time >= data.delay / 2 && d < 9.0 {
            let partner = b.partner;
            cmd.queue(move |w: &mut World| birth(w, e, partner));
        } else if love_time > data.delay {
            reqs.0.push(Req::End(e, Kind::Breed));
        }
    }
}
/// `Animal.spawnChildFromBreeding`: a baby of age -`baby_age`, both parents rest, love ends.
fn birth(w: &mut World, a: Entity, b: Entity) {
    let now = w.resource::<Clock>().0;
    if w.get::<Breeding>(a).is_none() || w.get::<Love>(a).is_none_or(|l| l.0 <= now) {
        return;
    }
    let data = *w.get::<BreedData>(a).unwrap();
    let (pos, active) = (w.get::<Pos>(a).unwrap().0, w.get::<ActiveUntil>(a).unwrap().0);
    for e in [a, b] {
        w.entity_mut(e).insert(Resting(now + data.rest)).remove::<Love>();
    }
    let seed = with_rng(w, a, |r| u64::from(r.next_u32()) << 32 | u64::from(r.next_u32()));
    // The baby is of its parents' species (the mate was checked to be the same).
    let sp = w.resource::<SpeciesTable>().0[w.get::<SpeciesId>(a).unwrap().0 as usize];
    let baby = w.spawn(spawn_bundle(&sp, pos.x, pos.z, seed)).id();
    crate::farm::equip(w, baby, &sp);
    w.get_mut::<ActiveUntil>(baby).unwrap().0 = active;
    make_baby(w, baby, data.baby_age);
    adopt(w, a, baby);
    if active > now {
        arm_stroll_now(w, baby);
    }
    arm_timers(w, baby);
    w.resource_mut::<Requests>().0.extend([Req::End(a, Kind::Breed), Req::End(b, Kind::Breed)]);
}
/// `FollowParentGoal.canContinue` and `tick`: the path to the parent is made again every 10 ticks.
fn follow_parent_run(
    clock: Res<Clock>,
    mut q: Query<(Entity, &Pos, &mut FollowParent, &Trailing, Has<GrowAt>)>,
    parents: Query<(&Pos, Has<GrowAt>)>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    for (e, pos, mut f, t, baby) in &mut q {
        let parent = parents.get(t.parent).ok().filter(|(pp, pb)| !pb && (9.0..=256.0).contains(&d2(pos.0, pp.0)));
        let (Some((pp, _)), true) = (parent, baby) else {
            reqs.0.push(Req::End(e, Kind::FollowParent));
            continue;
        };
        if clock.0 >= f.recalc {
            f.recalc = clock.0 + 10;
            go_to(&mut cmd, e, pp.0.x, pp.0.z, f.speed);
        }
    }
}
/// `LookAtPlayerGoal.canContinue` and `tick`.
fn glance_run(
    clock: Res<Clock>,
    on: Res<Online>,
    mut q: Query<(Aim, &Glancing, &Glance)>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    for ((e, pos, mut look, looking, _, _), g, data) in &mut q {
        let p = on.get(g.player).filter(|p| d2(Vec3::new(p.x, p.y, p.z), pos.0) <= data.range * data.range);
        match p {
            Some(p) if clock.0 < g.until => aim(&mut cmd, e, &mut look, looking, [p.x, p.eye(), p.z], None),
            _ => reqs.0.push(Req::End(e, Kind::LookAtPlayer)),
        }
    }
}
/// `RandomLookAroundGoal.tick`.
fn scan_run(clock: Res<Clock>, mut q: Query<(Aim, &Scanning)>, mut reqs: ResMut<Requests>, mut cmd: Commands) {
    for ((e, pos, mut look, looking, _, _), s) in &mut q {
        if clock.0 >= s.until {
            reqs.0.push(Req::End(e, Kind::RandomLook));
        } else {
            let at = [pos.0.x + s.dx, pos.0.y + look.eye, pos.0.z + s.dz];
            aim(&mut cmd, e, &mut look, looking, at, None);
        }
    }
}
/// A cow in a fluid (the fluid module visits only these) that gets deeper than the threshold, or in lava, starts
/// floating; one that gets shallower, or leaves the fluid, stops.
#[allow(clippy::type_complexity)]
fn float_events(
    q: Query<(Entity, &FluidState, &Buoyancy, Has<Floating>), With<InFluid>>,
    left: Query<Entity, (With<Floating>, Without<InFluid>)>,
    mut reqs: ResMut<Requests>,
) {
    for (e, f, b, floating) in &q {
        let deep = f.water > b.0 || f.in_lava;
        if deep != floating {
            reqs.0.push(if deep { Req::Start(e, Kind::Float) } else { Req::End(e, Kind::Float) });
        }
    }
    reqs.0.extend(left.iter().map(|e| Req::End(e, Kind::Float)));
}
/// `FloatGoal.tick`: a jump with 80 % each tick.
fn float_run(mut q: Query<(&mut Rng, &mut Jump), With<Floating>>) {
    for (mut r, mut j) in &mut q {
        j.0 = f64::from(r.next_u32() >> 8) / f64::from(1u32 << 24) < 0.8;
    }
}
fn rotate_towards(a: f32, b: f32, max: f32) -> f32 {
    a + wrap(b - a).clamp(-max, max)
}
fn wrap(a: f32) -> f32 {
    let mut a = a % 360.0;
    if a >= 180.0 {
        a -= 360.0;
    }
    if a < -180.0 {
        a += 360.0;
    }
    a
}
fn rotate_if_necessary(base: f32, target: f32, max: f32) -> f32 {
    target - wrap(target - base).clamp(-max, max)
}
/// `LookControl.tick` (default kind: the pitch is reset, the head turns to the target for 2 ticks after each ask,
/// then back to the body; clamped to the body while a path is followed) and `BodyRotationControl.clientTick` (a
/// body that moved takes the yaw; a head turned by more than 15 degrees drags the body; a head still for 10 ticks
/// brings it back over 10 more). Leaves when the head, the pitch and the body are at rest.
#[allow(clippy::type_complexity)]
fn look_control(mut q: Query<(Entity, &Pos, &Yaw, &mut Look, &mut Head, Has<Following>), With<Looking>>, mut cmd: Commands) {
    const ARRIVED: f64 = 2.500_000_3e-7_f32 as f64;
    for (e, pos, yaw, mut look, mut h, walking) in &mut q {
        if h.yaw.is_nan() {
            *h = Head { yaw: yaw.0, pitch: 0.0, body: yaw.0, stable: yaw.0, stable_time: 0, at: (pos.0.x, pos.0.z) };
        }
        h.pitch = 0.0;
        if look.cooldown > 0 {
            look.cooldown -= 1;
            let (dx, dy, dz) = (look.target[0] - pos.0.x, look.target[1] - (pos.0.y + look.eye), look.target[2] - pos.0.z);
            let sd = (dx * dx + dz * dz).sqrt();
            if dz.abs() > 1.0e-5 || dx.abs() > 1.0e-5 {
                h.yaw = rotate_towards(h.yaw, trig::atan2_degrees(dz, dx) - 90.0, look.y_speed);
            }
            if dy.abs() > 1.0e-5 || sd > 1.0e-5 {
                h.pitch = rotate_towards(h.pitch, -trig::atan2_degrees(dy, sd), look.x_max);
            }
        } else {
            h.yaw = rotate_towards(h.yaw, h.body, 10.0);
        }
        if walking {
            h.yaw = rotate_if_necessary(h.yaw, h.body, look.max_head_y);
        }
        let moved = (pos.0.x - h.at.0).powi(2) + (pos.0.z - h.at.1).powi(2) > ARRIVED;
        h.at = (pos.0.x, pos.0.z);
        if moved {
            h.body = yaw.0;
            h.yaw = rotate_if_necessary(h.yaw, h.body, look.max_head_y);
            (h.stable, h.stable_time) = (h.yaw, 0);
        } else if (h.yaw - h.stable).abs() > 15.0 {
            (h.stable, h.stable_time) = (h.yaw, 0);
            h.body = rotate_if_necessary(h.body, h.yaw, look.max_head_y);
        } else {
            h.stable_time += 1;
            if h.stable_time > 10 {
                let f = ((h.stable_time - 10) as f32 / 10.0).clamp(0.0, 1.0);
                h.body = rotate_if_necessary(h.body, h.yaw, look.max_head_y * (1.0 - f));
            }
        }
        if look.cooldown == 0 && !moved && wrap(h.yaw - h.body).abs() < 0.01 && h.pitch == 0.0 {
            cmd.entity(e).remove::<Looking>();
        }
    }
}

// ---- the selector: start and stop, with the locks -------------------------------------------------------------
#[derive(Clone, Copy)]
enum Pick {
    Yes,
    Spot(f64, f64),
    Water(f64, i32, f64),
    Partner(Entity),
    Player(u32),
    Parent(Entity),
}
pub(crate) fn arbitrate(world: &mut World) {
    crate::farm::on_grown(world);
    let fired = std::mem::take(&mut world.resource_mut::<Fired>().0);
    let reqs = std::mem::take(&mut world.resource_mut::<Requests>().0);
    for (e, k) in fired {
        timer_fired(world, e, k);
    }
    for r in reqs {
        match r {
            // A cow entering the active zone: its look timers are armed.
            Req::Arm(e) => {
                if world.resource::<LazyLook>().enabled
                    && let Some(mut h) = world.get_mut::<Head>(e)
                {
                    // A head that was not simulated starts from the body's heading.
                    h.yaw = f32::NAN;
                }
                arm_timers(world, e);
            }
            Req::Hunt(e) => crate::hostile::hunt(world, e),
            // The goal selector asks `canUse` only on the ticks where tick + id is even: an event that wakes a goal is
            // heard at the next such tick (the same as the next one at most).
            Req::Start(e, k) => {
                let now = world.resource::<Clock>().0;
                let phase = world.get::<Phase>(e).map_or(0, |p| if p.0 == u64::MAX { e.index().index() as u64 } else { p.0 });
                if now > 1 && (now + phase) % 2 == 1 {
                    arm(world, e, k, 1);
                } else {
                    start_goal(world, e, k);
                }
            }
            Req::End(e, k) => end_goal(world, e, k),
        }
    }
}
fn timer_fired(w: &mut World, e: Entity, k: Kind) {
    let now = w.resource::<Clock>().0;
    let active = w.get::<ActiveUntil>(e).map_or(0, |a| a.0);
    match k {
        Kind::Stroll => {
            let Some(mut s) = w.get_mut::<Stroll>(e) else { return };
            s.scheduled = false;
            if s.walking || now >= active {
                return;
            }
        }
        Kind::LookAtPlayer | Kind::RandomLook => {
            let Some(mut t) = w.get_mut::<GoalTimers>(e) else { return };
            if k == Kind::LookAtPlayer {
                t.look = false;
            } else {
                t.random = false;
            }
            // With the lazy look, a timer that comes up for an animal no player sees lapses (nothing re-arms it until a
            // player's range reaches it again).
            let p = w.get::<Pos>(e).unwrap().0;
            if !w.resource::<LazyLook>().watched(p.x, p.z) {
                return;
            }
        }
        Kind::Target => {
            let Some(mut t) = w.get_mut::<GoalTimers>(e) else { return };
            t.target = false;
            // Out of the active zone the selector does not ask (the next entry arms it again); with a target it runs.
            if now >= active || w.get::<crate::hostile::Aggro>(e).is_some() {
                return;
            }
        }
        Kind::Melee => {
            if let Some(mut a) = w.get_mut::<crate::hostile::Attack>(e) {
                a.armed = false;
            }
            if w.get::<crate::hostile::Aggro>(e).is_none() {
                return;
            }
            start_goal(w, e, k);
            if w.get::<GoalSet>(e).is_some_and(|g| !g.is_running(Kind::Melee)) {
                crate::hostile::arm_melee(w, e);
            }
            return;
        }
        Kind::Breed if w.get::<Love>(e).is_none_or(|l| l.0 <= now) => return,
        Kind::LayEgg => {
            crate::farm::lay_deadline(w, e);
            return;
        }
        Kind::EatBlock => {
            // A deadline of the goal that runs (the block, the end) or an ask of one that does not.
            if w.get::<crate::farm::Grazing>(e).is_some() {
                crate::farm::graze_deadline(w, e);
                return;
            }
            if let Some(mut t) = w.get_mut::<GoalTimers>(e) {
                t.graze = false;
            }
        }
        _ => {}
    }
    start_goal(w, e, k);
}
fn bit(g: &GoalSet, k: Kind) -> u16 {
    1 << g.at(k).unwrap()
}
fn start_goal(w: &mut World, e: Entity, k: Kind) {
    let Some(g) = w.get::<GoalSet>(e) else { return };
    if g.at(k).is_none() || g.is_running(k) {
        return;
    }
    let b = bit(g, k);
    if !g.replaceable(k) {
        match k {
            Kind::Stroll => arm_stroll_now(w, e),
            Kind::LookAtPlayer | Kind::RandomLook => rearm_timer(w, e, k),
            Kind::EatBlock => crate::farm::arm_graze(w, e),
            _ => w.get_mut::<GoalSet>(e).unwrap().pending |= b,
        }
        return;
    }
    w.get_mut::<GoalSet>(e).unwrap().pending &= !b;
    let Some(pick) = can_use(w, e, k) else { return };
    let stopped = w.get_mut::<GoalSet>(e).unwrap().start(k);
    for s in stopped {
        stop_effects(w, e, s);
    }
    start_effects(w, e, k, pick);
    calm_pause(w, e);
}
/// `TemptGoal.canUse` counts its `calmDown` down each time the selector asks, and the selector asks only a goal that
/// could take its flags: while another goal of an equal or higher priority holds them, the calm-down stands still.
fn calm_pause(w: &mut World, e: Entity) {
    let now = w.resource::<Clock>().0;
    let Some(n) = w.get::<Tempts>(e).map(|t| t.0.len()) else { return };
    for i in 0..n {
        let k = Kind::Tempt(i as u8);
        let g = w.get::<GoalSet>(e).unwrap();
        let blocked = !g.is_running(k) && !g.replaceable(k);
        let t = &mut w.get_mut::<Tempts>(e).unwrap().0[i];
        if blocked && t.calm_until > now && t.paused_at == 0 {
            t.paused_at = now;
        }
    }
}
/// The goals that blocked a tempt goal are gone: its calm-down runs again, from where it stood.
fn calm_resume(w: &mut World, e: Entity) {
    let now = w.resource::<Clock>().0;
    let Some(n) = w.get::<Tempts>(e).map(|t| t.0.len()) else { return };
    for i in 0..n {
        let k = Kind::Tempt(i as u8);
        let free = w.get::<GoalSet>(e).unwrap().replaceable(k);
        let t = &mut w.get_mut::<Tempts>(e).unwrap().0[i];
        if free && t.paused_at != 0 {
            t.calm_until += now - t.paused_at;
            t.paused_at = 0;
            let left = t.calm_until.saturating_sub(now);
            arm(w, e, k, left as i32);
        }
    }
}
fn rearm_timer(w: &mut World, e: Entity, k: Kind) {
    let g = *w.get::<Glance>(e).unwrap();
    let s = *w.get::<Scan>(e).unwrap();
    let mut t = *w.get::<GoalTimers>(e).unwrap();
    match k {
        Kind::LookAtPlayer => t.look = false,
        Kind::Target => t.target = false,
        _ => t.random = false,
    }
    let _ = (g, s);
    *w.get_mut::<GoalTimers>(e).unwrap() = t;
    arm_timers(w, e);
}
pub(crate) fn end_goal(w: &mut World, e: Entity, k: Kind) {
    let Some(mut g) = w.get_mut::<GoalSet>(e) else { return };
    if !g.stop(k) {
        return;
    }
    stop_effects(w, e, k);
    // The selector asks the goals in registration order: a panic that is still wanted comes back before a lower goal
    // that was waiting for its flag.
    if k == Kind::Panic {
        start_goal(w, e, k);
    }
    // A goal that was waiting for the flags it held tries again, in registration order.
    let g = w.get::<GoalSet>(e).unwrap();
    let waiting: Vec<Kind> = g.slots.iter().enumerate().filter(|(i, _)| g.pending >> i & 1 == 1).map(|s| s.1.0).collect();
    calm_resume(w, e);
    for p in waiting {
        start_goal(w, e, p);
    }
}
/// `canUse`: whether the goal can start now, and what it picked (a spot, a partner, a player, a parent). When it
/// cannot yet but may later, the next try is a deadline.
fn can_use(w: &mut World, e: Entity, k: Kind) -> Option<Pick> {
    let now = w.resource::<Clock>().0;
    let pos = w.get::<Pos>(e)?.0;
    match k {
        Kind::Float => floats(w, e).then_some(Pick::Yes),
        Kind::Panic => {
            let p = *w.get::<Panic>(e)?;
            let burning = w.get::<Burning>(e).is_some_and(|b| b.until > now);
            if now > p.until && !burning {
                return None;
            }
            if let Some((x, z)) = w.resource_mut::<Forced>().spots.remove(&(e, Kind::Panic)) {
                return Some(Pick::Spot(x, z));
            }
            // `PanicGoal.lookForWater`: a burning cow looks for water within 5 blocks first (nearest first).
            if burning && let Some((x, y, z)) = water_spot(w, pos, 5) {
                return Some(Pick::Water(x, y, z));
            }
            let lim = w.resource::<Level>().half - 2;
            // DefaultRandomPos: 10 tries; on the slab only the standing level is a stable destination.
            let tries: Vec<(i32, i32, i32)> = with_rng(w, e, |r| {
                (0..10)
                    .map(|_| {
                        let (dx, dy, dz) = (r.next_int(2 * p.h + 1) - p.h, r.next_int(2 * p.v + 1) - p.v, r.next_int(2 * p.h + 1) - p.h);
                        ((pos.x + f64::from(dx)).floor() as i32, dy, (pos.z + f64::from(dz)).floor() as i32)
                    })
                    .collect()
            });
            // `DefaultRandomPos.isValid`: a path type of malus 0 only (no cactus, fire, lava, water, powder snow).
            let hazards = w.get_resource::<Hazards>();
            let spot = tries.into_iter().find_map(|(x, dy, z)| {
                let at = box_at(Vec3::new(f64::from(x) + 0.5, 1.0, f64::from(z) + 0.5), 0.2, 0.2);
                let safe = hazards.is_none_or(|h| h.of_box(&at) == 0);
                (dy == 0 && x.abs() < lim && z.abs() < lim && safe).then(|| (f64::from(x) + 0.5, f64::from(z) + 0.5))
            });
            match spot {
                Some((x, z)) => Some(Pick::Spot(x, z)),
                None => {
                    // `canUse` asked again at the next selector tick, while the window lasts.
                    arm(w, e, Kind::Panic, 2);
                    None
                }
            }
        }
        Kind::Breed => {
            let b = *w.get::<BreedData>(e)?;
            if w.get::<Love>(e).is_none_or(|l| l.0 <= now) {
                return None;
            }
            let mut near = Vec::new();
            w.resource::<Index>().for_radius(pos.x, pos.z, b.range, |c| near.push(c));
            let mut best: Option<(Entity, f64)> = None;
            for c in near {
                let Some(cp) = w.get::<Pos>(c) else { continue };
                let d = d2(pos, cp.0);
                let lovers = w.get::<SpeciesId>(c) == w.get::<SpeciesId>(e) && w.get::<Love>(c).is_some_and(|l| l.0 > now) && w.get::<GrowAt>(c).is_none() && w.get::<Panicking>(c).is_none();
                if c != e && lovers && d <= b.range * b.range && best.is_none_or(|x| d < x.1) {
                    best = Some((c, d));
                }
            }
            if let Some(c) = w.resource_mut::<Forced>().partners.remove(&e)
                && w.get::<Love>(c).is_some_and(|l| l.0 > now)
            {
                best = Some((c, 0.0));
            }
            match best {
                Some((c, _)) => Some(Pick::Partner(c)),
                None => {
                    // No partner yet: the next look is a deadline, not a poll of every tick.
                    arm(w, e, Kind::Breed, 10);
                    None
                }
            }
        }
        Kind::Tempt(i) => {
            let t = *w.get::<Tempts>(e)?.0.get(i as usize)?;
            if now < t.calm_until {
                return None;
            }
            let mut best: Option<(u32, f64)> = None;
            let reach = t.range.max(2.0);
            w.resource::<Online>().for_near(pos.x, pos.z, reach, |p| {
                let d = d2(pos, Vec3::new(p.x, p.y, p.z));
                if p.holds_any(t.items) && d <= reach * reach && best.is_none_or(|b| d < b.1) {
                    best = Some((p.id, d));
                }
            });
            best.map(|b| Pick::Player(b.0))
        }
        Kind::FollowParent => {
            w.get::<GrowAt>(e)?;
            let mut near = Vec::new();
            w.resource::<Index>().for_radius(pos.x, pos.z, 9.0, |c| near.push(c));
            let mut best: Option<(Entity, f64)> = None;
            for c in near {
                let Some(cp) = w.get::<Pos>(c) else { continue };
                let (dx, dy, dz) = ((cp.0.x - pos.x).abs(), (cp.0.y - pos.y).abs(), (cp.0.z - pos.z).abs());
                if c != e && w.get::<SpeciesId>(c) == w.get::<SpeciesId>(e) && w.get::<GrowAt>(c).is_none() && dx <= 8.0 && dy <= 4.0 && dz <= 8.0 {
                    let d = d2(pos, cp.0);
                    if best.is_none_or(|b| d <= b.1) {
                        best = Some((c, d));
                    }
                }
            }
            // The adult found, near or far, is watched: it moving wakes the baby again.
            if let Some(b) = best {
                watch(w, b.0, e);
            }
            best.filter(|b| b.1 >= 9.0).map(|b| Pick::Parent(b.0))
        }
        Kind::Stroll => {
            let level = *w.resource::<Level>();
            let (mut s, mut r) = (*w.get::<Stroll>(e)?, Rng(w.get::<Rng>(e)?.0));
            let forced = w.resource_mut::<Forced>().spots.remove(&(e, Kind::Stroll));
            let ok = match forced {
                Some((x, z)) => {
                    (s.walking, s.tx, s.tz) = (true, x, z);
                    true
                }
                None => pick_spot(&level, &Pos(pos), &mut s, &mut r),
            };
            w.get_mut::<Rng>(e)?.0 = r.0;
            *w.get_mut::<Stroll>(e)? = s;
            if ok {
                Some(Pick::Spot(0.0, 0.0))
            } else {
                arm_stroll_now(w, e);
                None
            }
        }
        Kind::LookAtPlayer => {
            let g = *w.get::<Glance>(e)?;
            let mut best: Option<(u32, f64)> = None;
            w.resource::<Online>().for_near(pos.x, pos.z, g.range, |p| {
                let d = d2(pos, Vec3::new(p.x, p.y, p.z));
                if d <= g.range * g.range && best.is_none_or(|b| d < b.1) {
                    best = Some((p.id, d));
                }
            });
            match best {
                Some(b) => Some(Pick::Player(b.0)),
                None => {
                    rearm_timer(w, e, k);
                    None
                }
            }
        }
        Kind::RandomLook => Some(Pick::Yes),
        Kind::Target => match crate::hostile::pick_target(w, e) {
            Some(p) => Some(Pick::Player(p)),
            None => {
                rearm_timer(w, e, k);
                None
            }
        },
        Kind::Melee => crate::hostile::melee_target(w, e).map(Pick::Player),
        Kind::EatBlock => crate::farm::graze_can_use(w, e).then_some(Pick::Yes),
        Kind::LayEgg => None,
    }
}
/// `water_pos`: the water block nearest to the cow among the blocks within `range` on each side and one up or down,
/// unless the cow stands in a block with a collision shape.
fn water_spot(w: &World, pos: Vec3, range: i32) -> Option<(f64, i32, f64)> {
    use horizon_client_lib::blocks::Fluid;
    let mw = w.resource::<Level>().mw.0;
    let at = [pos.x.floor() as i32, pos.y.floor() as i32, pos.z.floor() as i32];
    if mw.data.state(mw.world.block(at[0], at[1], at[2])).top != 0.0 {
        return None;
    }
    mw.nearest_block(at, range, 1, |p, _| mw.fluid(p[0], p[1], p[2]) == Fluid::Water).map(|p| (f64::from(p[0]) + 0.5, p[1], f64::from(p[2]) + 0.5))
}
/// Sets the cow on fire for `ticks`: its damage every 20 ticks, and the panic that a burning cow has.
pub fn ignite(world: &mut World, e: Entity, ticks: u64) {
    let now = world.resource::<Clock>().0;
    world.entity_mut(e).insert(Burning { until: now + ticks });
    // `Entity.baseTick`: the fire hurts on the ticks where the remaining fire is a multiple of 20, the first included.
    world.resource_mut::<Wheel>().push(now + if ticks % 20 == 0 { 1 } else { 20 }, e, Due::Burn);
    world.resource_mut::<Requests>().0.push(Req::Start(e, Kind::Panic));
}
fn start_effects(w: &mut World, e: Entity, k: Kind, pick: Pick) {
    let now = w.resource::<Clock>().0;
    if k == Kind::EatBlock {
        // `EatBlockGoal.start`: the navigation stops.
        stop_nav(w, e);
        crate::farm::graze_start(w, e);
        return;
    }
    match (k, pick) {
        (Kind::Float, _) => {
            w.entity_mut(e).insert(Floating);
        }
        (Kind::Panic, Pick::Spot(..) | Pick::Water(..)) => {
            let (x, y, z) = match pick {
                Pick::Water(x, y, z) => (x, y, z),
                Pick::Spot(x, z) => (x, 1, z),
                _ => unreachable!(),
            };
            let speed = w.get::<Panic>(e).unwrap().speed;
            w.entity_mut(e).insert(Panicking);
            w.get_mut::<Nav>(e).unwrap().speed = speed;
            let mut s = w.get_mut::<Stroll>(e).unwrap();
            (s.tx, s.ty, s.tz) = (x, y, z);
            w.entity_mut(e).insert(NeedsPath);
        }
        (Kind::Breed, Pick::Partner(c)) => {
            w.entity_mut(e).insert(Breeding { partner: c, since: now, aim: (i32::MIN, 0) });
            w.resource_mut::<Requests>().0.push(Req::Start(c, Kind::Breed));
        }
        (Kind::Tempt(i), Pick::Player(id)) => {
            let mut ts = w.get_mut::<Tempts>(e).unwrap();
            let t = &mut ts.0[i as usize];
            t.player = id;
            t.aim = (i32::MIN, 0);
            w.entity_mut(e).insert(Tempting(i));
        }
        (Kind::FollowParent, Pick::Parent(c)) => {
            w.get_mut::<FollowParent>(e).unwrap().recalc = now;
            w.entity_mut(e).insert(Trailing { parent: c });
        }
        (Kind::Stroll, _) => {
            w.get_mut::<Nav>(e).unwrap().speed = 1.0;
            w.entity_mut(e).insert(NeedsPath);
        }
        (Kind::LookAtPlayer, Pick::Player(id)) => {
            // `reducedTickDelay(40 + nextInt(40))` asks of 2 ticks.
            let n = with_rng(w, e, |r| r.next_int(40));
            w.entity_mut(e).insert(Glancing { player: id, until: now + 2 * ((40 + n as u64 + 1) / 2) });
        }
        (Kind::Target, Pick::Player(id)) => crate::hostile::acquire(w, e, id),
        (Kind::Melee, Pick::Player(id)) => crate::hostile::start_melee(w, e, id),
        (Kind::RandomLook, _) => {
            let (a, n) = with_rng(w, e, |r| (std::f64::consts::TAU * r.next_f64(), r.next_int(20)));
            w.entity_mut(e).insert(Scanning { dx: a.cos(), dz: a.sin(), until: now + 20 + n as u64 + 1 });
        }
        _ => {}
    }
}
fn stop_effects(w: &mut World, e: Entity, k: Kind) {
    let now = w.resource::<Clock>().0;
    match k {
        Kind::Float => {
            w.entity_mut(e).remove::<Floating>();
            w.get_mut::<Jump>(e).unwrap().0 = false;
        }
        Kind::Panic => {
            w.entity_mut(e).remove::<Panicking>();
            // `canUse` is asked again at the next selector tick while the damage is recent or the cow burns.
            let until = w.get::<Panic>(e).unwrap().until;
            if now <= until || w.get::<Burning>(e).is_some_and(|b| b.until > now) {
                arm(w, e, Kind::Panic, 2);
            }
        }
        Kind::Breed => {
            w.entity_mut(e).remove::<Breeding>();
        }
        Kind::Tempt(i) => {
            w.entity_mut(e).remove::<Tempting>();
            stop_nav(w, e);
            // `calmDown = reducedTickDelay(100)`: no new temptation by this goal for 100 ticks, then it is looked at
            // again; the other tempt goal of the animal has its own.
            let mut ts = w.get_mut::<Tempts>(e).unwrap();
            let calm = ts.0[i as usize].calm;
            ts.0[i as usize].calm_until = now + calm;
            arm(w, e, Kind::Tempt(i), calm as i32);
        }
        Kind::FollowParent => {
            w.entity_mut(e).remove::<Trailing>();
        }
        Kind::Stroll => {
            w.get_mut::<Stroll>(e).unwrap().walking = false;
            stop_nav(w, e);
            arm_stroll_now(w, e);
        }
        Kind::LookAtPlayer => {
            w.entity_mut(e).remove::<Glancing>();
            rearm_timer(w, e, k);
        }
        Kind::Target => crate::hostile::lose_target(w, e),
        Kind::Melee => crate::hostile::stop_melee(w, e),
        Kind::EatBlock => crate::farm::graze_stop(w, e),
        Kind::LayEgg => {}
        Kind::RandomLook => {
            w.entity_mut(e).remove::<Scanning>();
            rearm_timer(w, e, k);
        }
    }
}

/// (name, cows with the module active) for the bench and the tests.
pub fn counts(world: &mut World) -> Vec<(&'static str, usize)> {
    macro_rules! n {
        ($t:ty) => {
            world.query_filtered::<(), With<$t>>().iter(world).count()
        };
    }
    vec![
        ("following", n!(Following)),
        ("moving", n!(Moving)),
        ("hunting", n!(crate::hostile::Aggro)),
        ("meleeing", n!(crate::hostile::Meleeing)),
        ("panicking", n!(Panicking)),
        ("tempting", n!(Tempting)),
        ("breeding", n!(Breeding)),
        ("trailing", n!(Trailing)),
        ("glancing", n!(Glancing)),
        ("scanning", n!(Scanning)),
        ("floating", n!(Floating)),
        ("looking", n!(Looking)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The item that tempts the species of the process (`HORIZON_SPECIES`).
    fn item() -> u32 {
        (1..32).find(|i| species().goals.tempts.last().unwrap().items >> i & 1 == 1).unwrap()
    }
    fn herd() -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        Some(Herd::new(0, 6.0, true, false, 3))
    }
    /// A cow that does nothing by itself: always active, no stroll and no look timer armed.
    fn cow(h: &mut Herd, x: f64, z: f64, seed: u64) -> Entity {
        let e = h.world.spawn(cow_bundle(x, z, seed)).id();
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.get_mut::<Stroll>(e).unwrap().scheduled = true;
        h.world.get_mut::<Vel>(e).unwrap().0.y = -0.08 * f64::from(0.98_f32);
        *h.world.get_mut::<GoalTimers>(e).unwrap() = GoalTimers { look: true, random: true, target: true, graze: true };
        e
    }
    fn player(h: &mut Herd, id: u32, x: f64, z: f64, item: u32) {
        h.world.resource_mut::<Online>().set(Player { id, x, y: 1.0, z, main: item, off: 0 });
    }
    fn ticks(h: &mut Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }
    fn has<T: Component>(h: &Herd, e: Entity) -> bool {
        h.world.get::<T>(e).is_some()
    }
    fn at(h: &Herd, e: Entity) -> Vec3 {
        h.world.get::<Pos>(e).unwrap().0
    }
    fn dist(a: Vec3, b: Vec3) -> f64 {
        ((a.x - b.x).powi(2) + (a.z - b.z).powi(2)).sqrt()
    }
    fn running(h: &Herd, e: Entity, k: Kind) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(k)
    }

    /// The interruption rules of `GoalSelector`: a lower number replaces a higher one that holds a flag it needs.
    #[test]
    fn the_selector_interrupts_by_priority_and_flags() {
        let mut g = GoalSet::new(&cow_slots());
        g.start(Kind::Stroll);
        assert!(!g.replaceable(Kind::RandomLook), "random look (7) cannot take MOVE from the stroll (5)");
        assert!(g.replaceable(Kind::Tempt(0)) && g.replaceable(Kind::Panic) && g.replaceable(Kind::Float));
        g.start(Kind::LookAtPlayer);
        assert_eq!(g.start(Kind::Panic), vec![Kind::Stroll]);
        assert!(!g.replaceable(Kind::Tempt(0)) && !g.replaceable(Kind::Stroll) && !g.replaceable(Kind::Breed));
        assert!(g.is_running(Kind::LookAtPlayer), "panic holds only MOVE");
        g.stop(Kind::Panic);
        assert_eq!(g.start(Kind::Tempt(0)), vec![Kind::LookAtPlayer], "tempt (3) takes LOOK from look-at-player (6)");
        assert!(g.replaceable(Kind::FollowParent) && g.start(Kind::FollowParent).is_empty(), "no flags: never conflicts");
        assert!(!g.replaceable(Kind::LookAtPlayer));
        g.stop(Kind::Tempt(0));
        g.start(Kind::RandomLook);
        assert_eq!(g.start(Kind::Stroll), vec![Kind::RandomLook]);
    }

    /// A struck cow flees at the speed modifier 2 to a spot within 5 blocks, during the walk only; the stroll it
    /// interrupted stops; a tempting player met during the panic waits for the flag and then tempts.
    #[test]
    fn a_struck_cow_panics_at_double_speed_and_blocks_the_lower_goals() {
        if crate::cow::skip_unless(crate::goals::Kind::Panic, "a_struck_cow_panics_at_double_speed_and_blocks_the_lower_goals") {
            return;
        }
        let Some(mut h) = herd() else { return };
        let (e, s) = (cow(&mut h, 0.5, 0.5, 11), cow(&mut h, -10.5, -10.5, 12));
        player(&mut h, 1, 0.5, 6.5, 0);
        ticks(&mut h, 2);
        // The reference stroll: the same walk at modifier 1.
        h.world.resource_mut::<Fired>().0.push((s, Kind::Stroll));
        let mut stroll_steps = Vec::new();
        let mut prev = at(&h, s);
        for _ in 0..60 {
            h.tick();
            if has::<Following>(&h, s) {
                stroll_steps.push(dist(prev, at(&h, s)));
            }
            prev = at(&h, s);
        }
        assert!(running(&h, s, Kind::Stroll) || stroll_steps.len() > 10);
        // The first cow walks (stroll), is struck, and panics.
        h.world.resource_mut::<Fired>().0.push((e, Kind::Stroll));
        ticks(&mut h, 3);
        assert!(running(&h, e, Kind::Stroll));
        let from = at(&h, e);
        h.world.resource_mut::<DamageQueue>().add(e, 1.0, crate::cow::Cause::Entity);
        let mut steps = Vec::new();
        let (mut started, mut ended) = (None, None);
        let mut prev = at(&h, e);
        for t in 0..300 {
            h.tick();
            let p = has::<Panicking>(&h, e);
            if p && started.is_none() {
                started = Some(t);
                assert!(!running(&h, e, Kind::Stroll), "panic took MOVE from the stroll");
                assert_eq!(h.world.get::<Nav>(e).unwrap().speed, species().goals.panic);
                let c = at(&h, e);
                player(&mut h, 2, c.x + 3.0, c.z, item());
            }
            assert!(!(running(&h, e, Kind::Panic) && running(&h, e, Kind::Tempt(0))), "panic and tempt share MOVE");
            if p {
                steps.push(dist(prev, at(&h, e)));
            }
            // The goal restarts for as long as the damage is recent (40 ticks): it is over after that.
            if started.is_some() && !p && ended.is_none() && h.world.resource::<Clock>().0 > h.world.get::<Panic>(e).unwrap().until {
                ended = Some(t);
                let c = at(&h, e);
                player(&mut h, 2, c.x + 3.0, c.z, item());
            }
            prev = at(&h, e);
            if ended.is_some() && t > ended.unwrap() + 120 || ended.is_some() && has::<Tempting>(&h, e) {
                break;
            }
        }
        let (started, ended) = (started.expect("no panic"), ended.expect("panic never ended"));
        assert!(started <= 4 && ended - started > 5, "panic from {started} to {ended}");
        assert!(dist(from, at(&h, e)) < 25.0);
        // Moving: modifier 2 gives 2 x 2 the pull of modifier 1 (`setSpeed` is both the speed and the input).
        let fast = steps.iter().cloned().fold(0.0, f64::max);
        let slow = stroll_steps.iter().cloned().fold(0.0, f64::max);
        let want = species().goals.panic.powi(2);
        assert!(fast / slow > want * 0.75 && fast / slow < want * 1.15, "{fast} / {slow}, expected about {want}");
        // Player 2 held wheat in range while it panicked: tempting began after the panic (and after the calm that the
        // interrupted tempt left), not during it.
        assert!(running(&h, e, Kind::Tempt(0)) || has::<Tempting>(&h, e));
        eprintln!("panic {started}..{ended}, step {fast:.3} vs stroll {slow:.3}");
    }

    /// Wheat within 10 blocks tempts, 10.5 does not; putting it away stops, with 100 ticks of calm.
    #[test]
    fn wheat_tempts_within_ten_blocks_and_stops_when_put_away() {
        if crate::cow::skip_unless(crate::goals::Kind::Tempt(0), "wheat_tempts_within_ten_blocks_and_stops_when_put_away") {
            return;
        }
        let Some(mut h) = herd() else { return };
        let (near, far) = (cow(&mut h, -8.5, 0.5, 21), cow(&mut h, -10.5, 5.5, 22));
        ticks(&mut h, 2);
        player(&mut h, 1, 0.5, 0.5, item());
        ticks(&mut h, 3);
        assert!(has::<Tempting>(&h, near), "9 blocks away");
        assert!(!has::<Tempting>(&h, far), "11.2 blocks away");
        assert_eq!(h.world.get::<Nav>(near).unwrap().speed, species().goals.tempts.last().unwrap().speed);
        ticks(&mut h, 120);
        let d = dist(at(&h, near), Vec3::new(0.5, 1.0, 0.5));
        assert!(d < 3.5 && d > 1.5, "stopped {d} from the player");
        // The player takes a step toward the far cow: it comes into range, the event wakes it.
        player(&mut h, 1, -1.5, 1.5, item());
        ticks(&mut h, 2);
        assert!(has::<Tempting>(&h, far), "came into range");
        // Wheat away: both stop within a tick; a new grab inside the 100 ticks of calm does not tempt.
        player(&mut h, 1, -1.5, 1.5, 0);
        ticks(&mut h, 2);
        assert!(!has::<Tempting>(&h, near) && !has::<Tempting>(&h, far));
        player(&mut h, 1, -1.5, 1.5, item());
        ticks(&mut h, 50);
        assert!(!has::<Tempting>(&h, near), "calm down");
        ticks(&mut h, 60);
        assert!(has::<Tempting>(&h, near), "tempted again after the calm");
    }

    /// Two cows in love: after the 60-tick delay close to each other, a baby of age -24000; both rest 6000 ticks.
    #[test]
    fn two_cows_in_love_make_a_baby_after_the_delay() {
        if crate::cow::skip_unless(crate::goals::Kind::Breed, "two_cows_in_love_make_a_baby_after_the_delay") {
            return;
        }
        let Some(mut h) = herd() else { return };
        let (a, b) = (cow(&mut h, 0.5, 0.5, 31), cow(&mut h, 5.5, 0.5, 32));
        ticks(&mut h, 2);
        assert!(feed(&mut h.world, a) && feed(&mut h.world, b));
        assert!(!feed(&mut h.world, a), "already in love");
        let (mut began, mut born) = (None, None);
        for t in 0..200 {
            h.tick();
            if began.is_none() && (has::<Breeding>(&h, a) || has::<Breeding>(&h, b)) {
                began = Some(t);
            }
            if born.is_none() && h.world.query::<&GrowAt>().iter(&h.world).count() > 0 {
                born = Some(t);
                break;
            }
        }
        let (began, born) = (began.expect("never began"), born.expect("no baby"));
        assert!(began <= 12 && born - began >= 56 && born - began <= 100, "began {began}, born {born}");
        let now = h.world.resource::<Clock>().0;
        let baby = h.world.query::<(Entity, &GrowAt)>().iter(&h.world).map(|(e, g)| (e, g.0)).next().unwrap();
        assert_eq!(baby.1, now + 24000);
        assert!(h.world.get::<Body>(baby.0).unwrap().width < 0.5);
        for p in [a, b] {
            assert_eq!(h.world.get::<Resting>(p).unwrap().0, now + 6000);
            assert!(!has::<Love>(&h, p) && !has::<Breeding>(&h, p));
        }
        assert!(!feed(&mut h.world, a), "resting");
        assert_eq!(h.world.query::<&GrowAt>().iter(&h.world).count(), 1);
    }

    /// A baby 3 blocks or more from its parent follows it; closer, or beyond 8 blocks, it does not; a parent that
    /// walks away wakes it again.
    #[test]
    fn a_baby_follows_its_parent_beyond_three_blocks() {
        if crate::cow::skip_unless(crate::goals::Kind::FollowParent, "a_baby_follows_its_parent_beyond_three_blocks") {
            return;
        }
        let Some(mut h) = herd() else { return };
        let mom = cow(&mut h, 0.5, 0.5, 41);
        let (far, close, lost) = (cow(&mut h, 6.5, 0.5, 42), cow(&mut h, 1.5, 1.5, 43), cow(&mut h, -12.5, 0.5, 44));
        for b in [far, close, lost] {
            make_baby(&mut h.world, b, 5000);
        }
        ticks(&mut h, 2);
        for b in [far, close, lost] {
            adopt(&mut h.world, mom, b);
        }
        ticks(&mut h, 3);
        assert!(has::<Trailing>(&h, far));
        assert!(!has::<Trailing>(&h, close) && !has::<Trailing>(&h, lost), "1.4 apart / 13 apart");
        ticks(&mut h, 200);
        assert!(dist(at(&h, far), at(&h, mom)) < 3.3 && !has::<Trailing>(&h, far));
        // The mother walks 6 blocks: her baby is woken by her move and follows.
        let to = Vec3::new(6.5, 1.0, 0.5);
        h.world.get_mut::<Pos>(mom).unwrap().0 = to;
        h.world.get_mut::<Bounds>(mom).unwrap().0 = box_at(to, 0.9, 1.4);
        ticks(&mut h, 3);
        assert!(has::<Trailing>(&h, far), "woken by the mother's move");
    }

    /// A baby 1.4 blocks from an adult that is not its mother: the adult walks off, and the baby follows (no scan).
    #[test]
    fn a_baby_follows_the_nearest_adult_that_walks_off() {
        if crate::cow::skip_unless(crate::goals::Kind::FollowParent, "a_baby_follows_the_nearest_adult_that_walks_off") {
            return;
        }
        let Some(mut h) = herd() else { return };
        let adult = cow(&mut h, 0.5, 0.5, 61);
        let baby = cow(&mut h, 1.5, 1.5, 62);
        ticks(&mut h, 2);
        make_baby(&mut h.world, baby, 5000);
        ticks(&mut h, 4);
        assert!(!has::<Trailing>(&h, baby), "1.4 blocks: no need");
        let to = Vec3::new(-2.0, 1.0, 0.5);
        h.world.get_mut::<Pos>(adult).unwrap().0 = to;
        h.world.get_mut::<Bounds>(adult).unwrap().0 = box_at(to, 0.9, 1.4);
        ticks(&mut h, 3);
        assert!(has::<Trailing>(&h, baby), "the adult moved 3.7 blocks away");
    }

    /// Look at a player within 6 blocks: the rate of a 1-in-50 ask every 2 ticks; none beyond; the head turns.
    #[test]
    fn cows_glance_at_a_near_player_and_look_around() {
        let Some(mut h) = herd() else { return };
        let mut cows = Vec::new();
        for i in 0..15 {
            for j in 0..13 {
                let (x, z) = (-9.0 + 1.3 * f64::from(i), -8.0 + 1.3 * f64::from(j));
                cows.push((cow(&mut h, x, z, 100 + cows.len() as u64), x, z));
            }
        }
        for (e, _, _) in &cows {
            *h.world.get_mut::<GoalTimers>(*e).unwrap() = GoalTimers::default();
        }
        player(&mut h, 1, 0.0, 0.0, 0);
        arm_all(&mut h.world);
        let (mut glanced, mut scanned) = (vec![false; cows.len()], 0);
        let mut seen_scan = vec![false; cows.len()];
        for _ in 0..150 {
            h.tick();
            for (i, (e, _, _)) in cows.iter().enumerate() {
                glanced[i] |= has::<Glancing>(&h, *e);
                seen_scan[i] |= has::<Scanning>(&h, *e);
            }
        }
        let (mut near, mut near_hit, mut far_hit) = (0, 0, 0);
        for (i, (_, x, z)) in cows.iter().enumerate() {
            if (x * x + z * z).sqrt() <= species().goals.look - 0.5 {
                near += 1;
                near_hit += usize::from(glanced[i]);
            } else if (x * x + z * z).sqrt() > species().goals.look + 0.5 {
                far_hit += usize::from(glanced[i]);
            }
            scanned += usize::from(seen_scan[i]);
        }
        let (rate, scan) = (near_hit as f64 / near as f64, scanned as f64 / cows.len() as f64);
        // 1 - 0.98^75 = 0.78 expected for a ask every 2 ticks over 150 ticks.
        assert!(far_hit == 0 && rate > 0.62 && rate < 0.92, "near {near_hit}/{near}, far {far_hit}");
        assert!(scan > 0.62 && scan < 0.92, "scan {scan}");
        let turned = cows.iter().filter(|c| h.world.get::<Head>(c.0).is_some_and(|hd| !hd.yaw.is_nan())).count();
        assert!(turned > 50, "{turned} heads moved");
        eprintln!("glance {rate:.2}, scan {scan:.2}");
    }

    /// A cow dropped in a pond floats: a jump 8 ticks in 10 while the water is deeper than 0.4, and it stays afloat.
    #[test]
    fn a_cow_in_a_pond_jumps_eight_ticks_in_ten() {
        if crate::cow::skip_unless(crate::goals::Kind::Float, "a_cow_in_a_pond_jumps_eight_ticks_in_ten") {
            return;
        }
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            return;
        }
        let mut h = Herd::with_hazards(1, 100.0, true, false, 5, true);
        let e = h.world.spawn(cow_bundle(-39.0, -47.0, 51)).id();
        h.world.get_mut::<Pos>(e).unwrap().0.y = 3.0;
        h.world.get_mut::<Bounds>(e).unwrap().0 = box_at(Vec3::new(-39.0, 3.0, -47.0), 0.9, 1.4);
        ticks(&mut h, 60);
        let (mut floating, mut jumps, mut low, mut was) = (0, 0, 9.0_f64, false);
        for _ in 0..1500 {
            h.tick();
            low = low.min(at(&h, e).y);
            let now = has::<Floating>(&h, e);
            // The first tick of a spell has no jump yet (the goal starts after its tick).
            if now && was {
                floating += 1;
                jumps += usize::from(h.world.get::<Jump>(e).unwrap().0);
            }
            was = now;
        }
        eprintln!("floating {floating}/1500 ticks, jumps {jumps}, lowest y {low:.2}");
        // A hen (eyes at 0.64) is in the water less of the time: only the rate of jumps is the goal's.
        assert!(floating > 30 && low > -0.5, "{floating} ticks afloat, lowest {low}");
        let rate = jumps as f64 / floating as f64;
        assert!(rate > 0.72 && rate < 0.88, "{rate}");
    }
}
