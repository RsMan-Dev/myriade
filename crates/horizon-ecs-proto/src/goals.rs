//! The goal selector, as data and event-driven systems (suivi/ecs-buts.md). `GoalSet` is the selector of one animal:
//! the list of its goals (module, priority, flags), the locks, what runs and what waits. It knows NO goal: a goal is a
//! `behaviours::Module` (one file) that a species lists with its parameters (`species.rs`). A goal is woken by a
//! deadline of the timer wheel or by an event and never scans; only a goal that runs ticks, through its own marker.
use crate::IFloor as _;
use crate::behaviours::{Change, Entry, Inst, Module};
use crate::cow::*;
use bevy_ecs::{prelude::*, schedule::Schedule};
use horizon_client_lib::neoforge_mob_maps::NeoForgeMobMaps;
use horizon_core::geometry::{Vec3, trig};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

pub use crate::behaviours::{
    breed::{BreedData, Breeding, Love, Resting, feed},
    float::{Buoyancy, Floating},
    follow_parent::{Babies, FollowParent, Mother, Trailing, adopt, make_baby},
    look_at_player::Glancing,
    panic::{Hurts, Panic, Panicking, hurt, ignite},
    random_look::{Scan, Scanning},
    tempt::{Tempt, Tempting, Tempts},
};

pub const MOVE: u8 = 1;
pub const LOOK: u8 = 2;
pub const JUMP: u8 = 4;
pub const TARGET: u8 = 8;
// (the wheel takes any deadline: no ceiling)
/// The widest tempting range of any species (the index scan radius).
pub(crate) const SCAN: f64 = 10.0;

/// The animals that changed column of the spatial index this tick (written by `reindex`, cleared by `arbitrate`).
#[derive(Resource, Default)]
pub struct CellMoves(pub Vec<Entity>);
/// The system set of the module of rank `.0` in the table: the sets are chained in the order of the table.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GoalSlot(pub u8);

/// The most entries (goals and targets together) a selector holds: a species that has more is an error of the registry at the freeze.
pub const MAX_ENTRIES: usize = 32;
/// `GoalSelector`: the animal's entries (module, priority, flags from the module) in order; `running` and `pending` are
/// slot bitmasks (32 goals at most, `MAX_ENTRIES`); `lock[b]` is the slot holding flag `b`. A pending goal is blocked by a lock and
/// starts when it frees. Resolved once at creation: nothing is looked up by name afterwards.
#[derive(Component, Clone)]
#[component(on_add = goalset_added, on_insert = goalset_counted, on_discard = goalset_uncounted)]
pub struct GoalSet {
    entries: &'static [Entry],
    nth: [u8; MAX_ENTRIES],
    running: u32,
    pending: u32,
    lock: [Option<u8>; 4],
    /// Some module wants to hear that a goal started or ended.
    watchers: bool,
    /// Flags no goal may take (a creature steers this entity: `mount.rs`).
    disabled: u8,
}
impl GoalSet {
    pub fn new(entries: &'static [Entry]) -> Self {
        debug_assert!(entries.len() <= MAX_ENTRIES, "a selector holds {MAX_ENTRIES} entries at most: the registry refuses a larger species when it freezes");
        let mut nth = [0; MAX_ENTRIES];
        for (i, e) in entries.iter().enumerate() {
            nth[i] = entries[..i].iter().filter(|o| std::ptr::eq(o.module, e.module)).count() as u8;
        }
        let watchers = entries.iter().any(|e| e.module.after_change.is_some());
        Self { entries, nth, running: 0, pending: 0, lock: [None; 4], watchers, disabled: 0 }
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn entries(&self) -> &'static [Entry] {
        self.entries
    }
    /// The same entries in the same order with other conditions or parameters (tests; the list's length must not change).
    pub fn replace_entries(&mut self, entries: &'static [Entry]) {
        assert_eq!(entries.len(), self.entries.len());
        self.entries = entries;
    }
    pub fn module(&self, slot: u8) -> &'static Module {
        self.entries[slot as usize].module
    }
    pub fn inst(&self, slot: u8) -> Inst {
        Inst { slot, nth: self.nth[slot as usize] }
    }
    /// The rank of the `nth` entry of the module.
    pub fn slot_of(&self, m: &'static Module) -> Option<u8> {
        self.slot_nth(m, 0)
    }
    pub fn slot_nth(&self, m: &'static Module, nth: u8) -> Option<u8> {
        (0..self.entries.len()).find(|&i| std::ptr::eq(self.entries[i].module, m) && self.nth[i] == nth).map(|i| i as u8)
    }
    pub fn has(&self, m: &'static Module) -> bool {
        self.entries.iter().any(|e| std::ptr::eq(e.module, m))
    }
    pub fn slot_running(&self, slot: u8) -> bool {
        self.running >> slot & 1 == 1
    }
    pub fn is_running(&self, m: &'static Module) -> bool {
        (0..self.entries.len()).any(|i| std::ptr::eq(self.entries[i].module, m) && self.running >> i & 1 == 1)
    }
    /// The goal holding the MOVE flag.
    pub fn mover(&self) -> Option<u8> {
        self.lock[0]
    }
    /// Every flag it needs is free or held by a goal of lower priority (a larger number).
    pub fn replaceable(&self, slot: u8) -> bool {
        let e = &self.entries[slot as usize];
        if e.module.flags & self.disabled != 0 {
            return false;
        }
        (0..4).filter(|b| e.module.flags >> b & 1 == 1).all(|b| self.lock[b].is_none_or(|j| e.priority < self.entries[j as usize].priority && self.entries[j as usize].module.interruptable))
    }
    /// Starts `slot`, taking its flags; returns the mask of the goals it stopped.
    pub fn start(&mut self, slot: u8) -> u32 {
        let flags = self.entries[slot as usize].module.flags;
        let mut stopped = 0u32;
        for b in 0..4 {
            if flags >> b & 1 == 1
                && let Some(j) = self.lock[b]
            {
                stopped |= 1 << j;
            }
        }
        for j in 0..MAX_ENTRIES as u8 {
            if stopped >> j & 1 == 1 {
                self.stop(j);
                // A goal that is polled every tick (`retry_on_end`) and that a stronger one preempted is asked again when that one ends.
                if self.entries[j as usize].module.retry_on_end {
                    self.pending |= 1 << j;
                }
            }
        }
        for b in 0..4 {
            if flags >> b & 1 == 1 {
                self.lock[b] = Some(slot);
            }
        }
        self.running |= 1 << slot;
        self.pending &= !(1 << slot);
        stopped
    }
    pub fn stop(&mut self, slot: u8) -> bool {
        let was = self.running >> slot & 1 == 1;
        self.running &= !(1 << slot);
        for l in &mut self.lock {
            if *l == Some(slot) {
                *l = None;
            }
        }
        was
    }
}

pub use crate::items::item_id;
macro_rules! item_fns {
    ($($f:ident => $n:expr),* $(,)?) => { $(pub fn $f() -> u32 { item_id($n) })* };
}
item_fns!(wheat => "minecraft:wheat", carrot => "minecraft:carrot", potato => "minecraft:potato", beetroot => "minecraft:beetroot", carrot_on_a_stick => "minecraft:carrot_on_a_stick",
    wheat_seeds => "minecraft:wheat_seeds", beetroot_seeds => "minecraft:beetroot_seeds", melon_seeds => "minecraft:melon_seeds", pumpkin_seeds => "minecraft:pumpkin_seeds",
    shears => "minecraft:shears", bone => "minecraft:bone", lead => "minecraft:lead", warped_fungus_on_a_stick => "minecraft:warped_fungus_on_a_stick", saddle => "minecraft:saddle", trident => crate::species::lists::TRIDENT, crossbow => "minecraft:crossbow");
/// The phase of the goal selector of the entity: it asks `canUse` on the ticks where tick + phase is even (the id of
/// a `Mob`). By default the animal's `Uid`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Phase(pub u64);
/// The animal's number, by order of creation in its world: the seed of what is drawn by default (phase, breath). Unlike
/// the index of the bevy entity, it does not move when a resource or another entity is added before the animals.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Uid(pub u64);
thread_local! {
    static NEXT_UID: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}
/// Restarts the numbering (a new world, on this thread).
pub fn reset_uids() {
    NEXT_UID.with(|c| c.set(0));
}
pub(crate) fn next_uid() -> u64 {
    NEXT_UID.with(|c| {
        let n = c.get();
        c.set(n + 1);
        n
    })
}
/// `LookControl`: where to look, how fast, the eye height, the head's limits.
#[derive(Component, Clone, Copy, Debug)]
pub struct Look {
    pub y_default: f32,
    pub target: [f64; 3],
    pub cooldown: u8,
    pub y_speed: f32,
    pub x_max: f32,
    /// `x_max` of the look the entity has when nothing asks for another (`aim_clamped` overrides it for one look).
    pub x_default: f32,
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
    /// The angles to the look target, kept while neither the target nor the animal moves (the arc tangents are not
    /// worked out every tick of a look that lasts 40).
    pub angles: Option<([f64; 3], Vec3, Option<f32>, Option<f32>)>,
    /// (target, position, turn speed, walking) of a look that left the head, the pitch and the body unchanged for a whole
    /// tick with the body at rest: while the same four hold, the next ticks of the look change nothing and are skipped.
    pub settled: Option<([f64; 3], Vec3, f32, bool)>,
}
/// The jump the goals ask for this tick (the physics consumes it).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Jump(pub bool);

/// The head turns toward a target (`look_control` visits it).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Looking;

// ---- resources: the events, as data ------------------------------------------------------------------------
#[derive(Resource, Default)]
pub struct Fired(pub Vec<(Entity, u8)>);
#[derive(Resource, Default)]
pub struct Entered(pub Vec<Entity>);
pub enum Req {
    /// Run a module's function for the animal, in the order of the requests (a hit that makes a hunt, for example).
    Call(Entity, fn(&mut World, Entity)),
    Arm(Entity),
    /// The goal of this slot (rank in the animal's configuration) is woken or ends.
    Start(Entity, u8),
    End(Entity, u8),
}
#[derive(Resource, Default)]
pub struct Requests(pub Vec<Req>);
/// The option of the lazy look: when on, the look goals (`LookAtPlayer`, `RandomLookAround`) and the look control run
/// only for an animal that a player can see, i.e. within `range` blocks, the client tracking range of an animal
/// (10 chunks = 160 blocks; the client renders an entity at most 64 x 2.5 = 160 blocks away), rounded out to 16-block
/// bins. ON by default (owner decision 2026-11-19: "useless for inactive and invisible mobs"); `set_lazy_look(world, false)` turns it off.
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
        Self { enabled: true, range: 160.0, counts: HashMap::new(), at: HashMap::new() }
    }
}
fn bin16(x: f64, z: f64) -> (i32, i32) {
    ((x / 16.0).ifloor(), (z / 16.0).ifloor())
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
/// instead of drawing one. Absent outside the tests. `slot` is the goal's slot; the component is removed when taken.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct ForcedSpot(pub u8, pub (f64, f64));
/// The row (y) of an imposed spot, when the test gives one (`impose_spot`): the `Mob` draws a row too.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct ForcedRow(pub u8, pub f64);
/// The partner a cow in love takes instead of the nearest one.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct ForcedPartner(pub Entity);
/// The strafe state (clockwise, back) the bow goal takes when its strafe time comes up, instead of drawing the flips.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct ForcedStrafe(pub (bool, bool));
/// The order the kids of a shuffled composite of the brain are tried in (indices of the kids), instead of a draw.
#[derive(Component, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct ForcedShuffle(pub std::collections::VecDeque<Vec<usize>>);

/// A spot in 3D for the goal of `slot`: the module takes it with `take_spot` instead of drawing one.
pub fn impose_spot(w: &mut World, e: Entity, slot: u8, at: [f64; 3]) {
    w.entity_mut(e).insert((ForcedSpot(slot, (at[0], at[2])), ForcedRow(slot, at[1])));
}
/// Takes the imposed (x, z) of the goal of `slot`, or `None`.
pub fn take_xz(w: &mut World, e: Entity, slot: u8) -> Option<(f64, f64)> {
    let ForcedSpot(s, xz) = *w.get::<ForcedSpot>(e)?;
    if s != slot {
        return None;
    }
    w.entity_mut(e).remove::<ForcedSpot>();
    Some(xz)
}
/// The imposed spot of this goal (x, y, z), taken, or `None`; `default_y` is the row when only (x, z) was imposed.
pub fn take_spot(w: &mut World, e: Entity, slot: u8, default_y: f64) -> Option<[f64; 3]> {
    let (x, z) = take_xz(w, e, slot)?;
    let row = w.get::<ForcedRow>(e).copied();
    let y = match row {
        Some(ForcedRow(s, y)) if s == slot => {
            w.entity_mut(e).remove::<ForcedRow>();
            y
        }
        _ => default_y,
    };
    Some([x, y, z])
}
/// What stops an entity from acting (`Mob.isImmobile` / the goals' "cannot act" tests): a bit per reason, set by the module that
/// causes it and read by any goal, which never needs to know that module. Absent: free.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Occupied(pub u32);
pub const OCC_GRAZING: u32 = 1;
pub const OCC_MOUNTED: u32 = 2;
pub const OCC_SITTING: u32 = 4;
pub const OCC_REARING: u32 = 8;
pub const OCC_SLEEPING: u32 = 16;
pub const OCC_ROLLED: u32 = 32;
pub fn occupy(w: &mut World, e: Entity, reason: u32) {
    let now = w.get::<Occupied>(e).map_or(0, |o| o.0);
    w.entity_mut(e).insert(Occupied(now | reason));
}
pub fn release(w: &mut World, e: Entity, reason: u32) {
    if let Some(mut o) = w.get_mut::<Occupied>(e) {
        o.0 &= !reason;
    }
}
/// Whether the entity is busy with any reason of `mask` (`u32::MAX`: any).
pub fn is_occupied(w: &World, e: Entity, mask: u32) -> bool {
    w.get::<Occupied>(e).is_some_and(|o| o.0 & mask != 0)
}
/// The equipment slots a goal reads (an item id, 0 empty): `Equipment` is absent on an entity that wears nothing.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Equipment {
    pub head: u32,
    pub chest: u32,
    pub legs: u32,
    pub feet: u32,
    pub main: u32,
    pub off: u32,
    pub body: u32,
    pub saddle: u32,
}
/// The whole stack held in the main hand (`LivingEntity.getMainHandItem`), when an item of the ground was picked up or put there; `Equipment.main`
/// mirrors its item id for the goals that read ids.
#[derive(Component, Clone, Debug, Default)]
pub struct HandStack(pub Option<horizon_client_lib::items::ItemStack>);
/// `LivingEntity.setItemSlot(MAINHAND, stack)`: the stack in the hand, its id in `Equipment` (created if the entity wore nothing).
pub fn set_main_hand(w: &mut World, e: Entity, stack: Option<horizon_client_lib::items::ItemStack>) {
    let id = stack.as_ref().map_or(0, |s| item_id(&s.item));
    if let Some(mut q) = w.get_mut::<Equipment>(e) {
        q.main = id;
    } else {
        w.entity_mut(e).insert(Equipment { main: id, ..Default::default() });
    }
    w.entity_mut(e).insert(HandStack(stack));
}
/// The stack in the main hand.
pub fn main_hand(w: &World, e: Entity) -> Option<&horizon_client_lib::items::ItemStack> {
    w.get::<HandStack>(e)?.0.as_ref()
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Head,
    Chest,
    Legs,
    Feet,
    Main,
    Off,
    /// The body slot of a mount or a wolf (horse armor, wolf armor, carpet) and the saddle.
    Body,
    Saddle,
}
/// The item in a slot of the entity (0 when it has none or no equipment at all).
pub fn equipped(w: &World, e: Entity, slot: Slot) -> u32 {
    w.get::<Equipment>(e).map_or(0, |q| match slot {
        Slot::Head => q.head,
        Slot::Chest => q.chest,
        Slot::Legs => q.legs,
        Slot::Feet => q.feet,
        Slot::Main => q.main,
        Slot::Off => q.off,
        Slot::Body => q.body,
        Slot::Saddle => q.saddle,
    })
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Player {
    pub id: u32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub main: u32,
    pub off: u32,
    /// The armor worn (0: nothing).
    pub head: u32,
    pub chest: u32,
    pub legs: u32,
    pub feet: u32,
}
impl Player {
    pub fn holds(&self, item: u32) -> bool {
        item != 0 && (self.main == item || self.off == item)
    }
    /// Whether a hand holds one of the items of the set.
    pub fn holds_any(&self, items: crate::items::ItemSet) -> bool {
        items.contains(self.main) || items.contains(self.off)
    }
    pub fn eye(&self) -> f64 {
        self.y + 1.62
    }
}
fn bin(x: f64, z: f64) -> (i32, i32) {
    ((x / 8.0).ifloor(), (z / 8.0).ifloor())
}
/// The players of the world, in 8-block bins; every change is an event (old, new) read by `online_events`.
#[derive(Resource, Default)]
pub struct Online {
    list: Vec<Option<Player>>,
    bins: HashMap<(i32, i32), Vec<u32>>,
    events: Vec<(Option<Player>, Option<Player>)>,
    /// The events of this tick, kept for the modules' wake-up systems (cleared by the next `online_events`).
    pub seen: Vec<(Option<Player>, Option<Player>)>,
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
    /// Every player online.
    pub fn players(&self) -> impl Iterator<Item = Player> + '_ {
        self.list.iter().flatten().copied()
    }
    /// Every player, in order of id (no bins: the cost is the list, not an area).
    pub fn for_all(&self, f: impl FnMut(Player)) {
        self.list.iter().flatten().copied().for_each(f);
    }
    /// Whether `f` holds for some player of the window (as `for_near`, but it stops at the first: the order does not matter).
    pub fn any_near(&self, x: f64, z: f64, r: f64, mut f: impl FnMut(Player) -> bool) -> bool {
        let ((x0, z0), (x1, z1)) = (bin(x - r, z - r), bin(x + r, z + r));
        if (i64::from(x1 - x0) + 1) * (i64::from(z1 - z0) + 1) > 2 * self.bins.len() as i64 {
            return self.bins.iter().filter(|&(&(bx, bz), _)| (x0..=x1).contains(&bx) && (z0..=z1).contains(&bz)).any(|(_, v)| v.iter().any(|&id| self.get(id).is_some_and(&mut f)));
        }
        // The middle first: the players nearest the asker are in the bins around it.
        let c = bin(x, z);
        if self.bins.get(&c).into_iter().flatten().any(|&id| self.get(id).is_some_and(&mut f)) {
            return true;
        }
        for bz in z0..=z1 {
            for bx in x0..=x1 {
                if (bx, bz) != c && self.bins.get(&(bx, bz)).into_iter().flatten().any(|&id| self.get(id).is_some_and(&mut f)) {
                    return true;
                }
            }
        }
        false
    }
    /// The nearest player of the window (as `for_near`) that `pass(player, squared distance)` accepts: the one `for_near` would keep taking the first
    /// of the smallest distance (order: bin by (z, x), then the bin's order). The bins are visited from the middle outwards, and the ones farther than the
    /// best found are not visited: a crowd costs the players near the asker, not the whole window.
    pub fn nearest_where(&self, x: f64, y: f64, z: f64, r: f64, mut pass: impl FnMut(Player, f64) -> bool) -> Option<u32> {
        let best: std::cell::Cell<Option<(u32, f64, (i32, i32, usize))>> = std::cell::Cell::new(None);
        let mut visit = |bx: i32, bz: i32, v: &Vec<u32>| {
            for (i, &id) in v.iter().enumerate() {
                let Some(p) = self.get(id) else { continue };
                let d = (x - p.x).powi(2) + (y - p.y).powi(2) + (z - p.z).powi(2);
                let key = (bz, bx, i);
                if best.get().is_some_and(|b| d > b.1 || (d == b.1 && key > b.2)) {
                    continue;
                }
                if pass(p, d) {
                    best.set(Some((id, d, key)));
                }
            }
        };
        let ((x0, z0), (x1, z1)) = (bin(x - r, z - r), bin(x + r, z + r));
        if (i64::from(x1 - x0) + 1) * (i64::from(z1 - z0) + 1) > 2 * self.bins.len() as i64 {
            for (&(bx, bz), v) in &self.bins {
                if (x0..=x1).contains(&bx) && (z0..=z1).contains(&bz) {
                    visit(bx, bz, v);
                }
            }
            return best.get().map(|b| b.0);
        }
        let (cx, cz) = bin(x, z);
        let last = (cx - x0).max(x1 - cx).max(cz - z0).max(z1 - cz);
        for k in 0..=last {
            if let Some(b) = best.get() {
                // Every player of ring `k` is at least `k - 1` whole bins away.
                let gap = f64::from((k - 1).max(0)) * 8.0;
                if gap * gap > b.1 {
                    break;
                }
            }
            for bz in (cz - k).max(z0)..=(cz + k).min(z1) {
                let edge = (bz - cz).abs() == k;
                for bx in [cx - k, cx + k].into_iter().filter(|_| !edge).chain(if edge { (cx - k).max(x0)..=(cx + k).min(x1) } else { 1..=0 }) {
                    if (x0..=x1).contains(&bx) && let Some(v) = self.bins.get(&(bx, bz)) {
                        visit(bx, bz, v);
                    }
                }
            }
        }
        best.get().map(|b| b.0)
    }
    pub fn for_near(&self, x: f64, z: f64, r: f64, mut f: impl FnMut(Player)) {
        let ((x0, z0), (x1, z1)) = (bin(x - r, z - r), bin(x + r, z + r));
        // A wide window over few players (a ghast's 100 blocks, 625 bins, none occupied): the occupied bins are visited, in the same (z, x) order.
        if (i64::from(x1 - x0) + 1) * (i64::from(z1 - z0) + 1) > 2 * self.bins.len() as i64 {
            let mut hit: Vec<(i32, i32)> = self.bins.iter().filter(|&(&(bx, bz), v)| !v.is_empty() && (x0..=x1).contains(&bx) && (z0..=z1).contains(&bz)).map(|(&(bx, bz), _)| (bz, bx)).collect();
            hit.sort_unstable();
            for (bz, bx) in hit {
                for &id in &self.bins[&(bx, bz)] {
                    if let Some(p) = self.get(id) {
                        f(p);
                    }
                }
            }
            return;
        }
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


/// The animal's number, by order of creation (see `Uid`) and the components of the selector. The modules' state
/// components are created by their `equip`, once, when the selector is added.
pub fn goals_bundle(sp: &Species, uid: u64) -> impl Bundle {
    (
        GoalSet::new(sp.goals.entries),
        Jump(false),
        Phase(uid),
    )
}
/// `Head`: the look and the head state (the look goals and any goal that aims).
pub fn head_bundle(sp: &Species) -> impl Bundle {
    (
        Look { target: [0.0; 3], cooldown: 0, y_default: 10.0, y_speed: 10.0, x_max: 40.0, x_default: 40.0, max_head_y: 75.0, eye: sp.eye_height },
        Head { yaw: f32::NAN, pitch: 0.0, body: 0.0, stable: 0.0, stable_time: 0, at: (0.0, 0.0), angles: None, settled: None },
    )
}
/// Creates the state of every module of the animal's configuration from the parameters of its entries (once).
/// Queued by the `on_add` hook of `GoalSet`: it runs as soon as the spawn is flushed.
fn equip_goals(w: &mut World, e: Entity) {
    let Some(g) = w.get::<GoalSet>(e) else { return };
    let (entries, n) = (g.entries, g.len());
    let insts: Vec<Inst> = (0..n).map(|i| g.inst(i as u8)).collect();
    for (en, inst) in entries.iter().zip(insts) {
        (en.module.equip)(w, e, inst, en.params);
    }
}

/// The live carriers of each module (by its rank in the registry): a module with none has its systems skipped by a run condition
/// on its set, tested once per tick. Kept by the hooks of `GoalSet` and `Modes`, at creation and death.
#[derive(Resource, Default)]
pub struct Carriers(pub Vec<u32>, pub crate::cow::Census, pub u128);
pub(crate) fn count_carriers(world: &mut bevy_ecs::world::DeferredWorld, entries: &[Entry], delta: i32) {
    let reg = crate::species::registry();
    let Some(mut c) = world.get_resource_mut::<Carriers>() else { return };
    for en in entries {
        if let Some(i) = reg.module_id(en.module)
            && let Some(n) = c.0.get_mut(i)
        {
            *n = n.saturating_add_signed(delta);
            // The modules that have a tick and a carrier, one bit each (their rank in the list of ticks): `tick_modules` reads this word only.
            if let Some(j) = tick_pos(i).filter(|j| *j < 128) {
                let on = u128::from(c.0[i] > 0) << j;
                c.2 = c.2 & !(1 << j) | on;
            }
        }
    }
}
// Counted at every insertion (a replaced component counts again) and uncounted when it is replaced or removed.
fn goalset_uncounted(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let Some(entries) = world.get::<GoalSet>(ctx.entity).map(|g| g.entries) else { return };
    count_carriers(&mut world, entries, -1);
}
fn goalset_counted(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    if let Some(entries) = world.get::<GoalSet>(ctx.entity).map(|g| g.entries) {
        count_carriers(&mut world, entries, 1);
    }
}
fn goalset_added(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let e = ctx.entity;
    world.commands().queue(move |w: &mut World| equip_goals(w, e));
}

/// Registers the resources, the systems; call from `Herd::new` / `schedule()`.
pub fn insert_resources(world: &mut World) {
    world.insert_resource(Carriers(vec![0; crate::species::registry().modules.len()], Default::default(), 0));
    world.insert_resource(Fired::default());
    world.insert_resource(Entered::default());
    world.insert_resource(Requests::default());
    world.insert_resource(LazyLook::default());
    world.insert_resource(Events::default());
    world.insert_resource(crate::farm::Grown::default());
    world.insert_resource(crate::farm::LaidEggs::default());
    world.insert_resource(Online::default());
    world.insert_resource(CellMoves::default());
    crate::projectile::insert_resources(world);
    crate::brain::insert_resources(world);
    crate::item::insert_resources(world);
    crate::orb::insert_resources(world);
    crate::hanging::insert_resources(world);
    crate::species::cushion::insert_resources(world);
    crate::species::leash_knot::insert_resources(world);
    crate::end_crystal::insert_resources(world);
    crate::firework::insert_resources(world);
    crate::trident::insert_resources(world);
    world.insert_resource(crate::block_events::BlockEvents::default());
    crate::poi::insert_resources(world);
    crate::block_watch::insert_resources(world);
    crate::explosion::insert_resources(world);
    crate::tame::insert_resources(world);
    world.insert_resource(crate::targeting::TargetWatchers::default());
    insert_mob_maps(world, NeoForgeMobMaps::vanilla().clone());
    for (i, m) in crate::species::registry().modules.iter().enumerate() {
        if crate::species::registry().carried(i) {
            (m.resources)(world);
        }
    }
}

/// The data maps of the mobs (`neoforge:*`), read once from the packs: `MobMaps` holds the snapshot (the hero gifts of
/// `species::villager`), `DistanceMaps` the villager distances of `behaviours::hostiles_near`, filled from the same snapshot.
#[derive(Resource)]
pub struct MobMaps(pub Arc<NeoForgeMobMaps>);

/// Installs the mobs' data maps: the server calls it with the snapshot of its packs (replacing the vanilla ones of `insert_resources`).
pub fn insert_mob_maps(world: &mut World, maps: Arc<NeoForgeMobMaps>) {
    let names = crate::species::registry().species.iter().map(|s| s.name).chain(std::iter::once("minecraft:player"));
    let distances = names.filter_map(|n| maps.villager_distance(n).map(|d| (n.to_string(), f64::from(d)))).collect();
    let table = "neoforge:acceptable_villager_distances".to_string();
    world.insert_resource(crate::behaviours::hostiles_near::DistanceMaps(BTreeMap::from([(table, distances)])));
    world.insert_resource(MobMaps(maps));
}
/// The framework's systems, then the modules' (in the order of the table), then the selector and the look control.
pub fn register(s: &mut Schedule) {
    s.add_systems(online_events.after(due).before(player_near));
    s.add_systems(player_watch.after(online_events).before(player_near));
    s.add_systems((entered_events, arbitrate).chain().before(pathfind));
    s.add_systems((arbitrate_end, look_control).chain().after(arbitrate).before(pathfind));
    s.add_systems((crate::targeting::sight_eval, crate::targeting::target_continue).chain().after(apply_damage));
    let modules = &crate::species::registry().modules;
    // `Mob.serverAiStep` ticks the target selector, then the goal selector: the target selectors (`TARGET`) run before the
    // starts of the tick, the others after them, so a goal's `canUse` sees the target of its own tick. Each class keeps the registry order.
    let mut last = [None, None];
    for i in 0..modules.len() {
        let slot = GoalSlot(i as u8);
        let goal = modules[i].flags & TARGET == 0;
        s.configure_sets(slot.after(online_events).after(entered_events).after(crate::targeting::target_continue).before(pathfind));
        if goal {
            s.configure_sets(slot.after(arbitrate).before(arbitrate_end));
        } else {
            s.configure_sets(slot.before(arbitrate));
        }
        // The registry order within a class, and after the target slot just before it (the only cross-class order the split keeps).
        if let Some(prev) = last[goal as usize] {
            s.configure_sets(slot.after(GoalSlot(prev)));
        }
        if goal && i > 0 && modules[i - 1].flags & TARGET != 0 {
            s.configure_sets(slot.after(GoalSlot(i as u8 - 1)));
        }
        last[goal as usize] = Some(i as u8);
        // A module no resolved species carries has no systems in the schedule: it costs nothing (`Registry::carried`).
        if crate::species::registry().carried(i) && modules[i].tick.is_none() {
            (modules[i].systems)(s, slot);
            // Registered but nobody alive carries it: the whole set is skipped (one test per tick).
            s.configure_sets(slot.run_if(move |c: Res<Carriers>| c.0.get(i).is_some_and(|n| *n > 0)));
        }
    }
}

// ---- the events of the framework (a module never polls a component) ------------------------------------------
/// The entities one event happened to. THE GUARANTEE: `iter` yields the events of the PREVIOUS tick, all of them, and only
/// those: a system that runs every tick sees each event exactly once, whatever its place in the schedule (`due`, the first
/// system of the tick, swaps the two lists). The events of the current tick are not visible before the next one. Nobody
/// clears or takes them; a system that does not run every tick may miss events.
pub struct Ev<T = Entity> {
    cur: Vec<T>,
    prev: Vec<T>,
}
impl<T> Default for Ev<T> {
    fn default() -> Self {
        Self { cur: Vec::new(), prev: Vec::new() }
    }
}
impl<T: Copy> Ev<T> {
    pub fn push(&mut self, e: T) {
        self.cur.push(e);
    }
    pub fn iter(&self) -> impl Iterator<Item = T> + '_ {
        self.prev.iter().copied()
    }
    pub fn is_empty(&self) -> bool {
        self.prev.is_empty()
    }
    pub(crate) fn swap(&mut self) {
        std::mem::swap(&mut self.prev, &mut self.cur);
        self.cur.clear();
    }
}
/// The state changes other modules wait for (an entity list per kind, read the tick AFTER it happened, once; `Hurts` and
/// `Grown` keep their older form). Written by the framework's systems; a new kind is one field and one line in `swap`.
#[derive(Resource, Default)]
pub struct Events {
    /// Outputs toward the host (block writes, sounds, particles): a field here and not a resource of its own, because one more
    /// resource moves the cost of the solicited scenes (+6,6 % measured at 14 448 against 13 544 on the cow).
    pub host: crate::host_out::HostOut,
    /// `Burning` was set (from not burning).
    pub ignited: Ev,
    /// `Burning` ended (water, or the fire burnt out).
    pub extinguished: Ev,
    pub entered_fluid: Ev,
    pub left_fluid: Ev,
    /// The feet reached the ground after being in the air.
    pub landed: Ev,
    /// The active mode of locomotion changed (`locomotion::switch`: a fluid, or a goal that takes off or lands).
    pub mode_changed: Ev,
    /// A baby grew up (same as `farm::Grown`, readable by several).
    pub grew: Ev,
    /// A grazing animal ate its block (`eat_block`).
    pub ate: Ev,
    /// Hurt by anything / by an entity (a player or a creature): every module reads these, once (the attacker: `HurtBy`).
    pub hurt: Ev,
    pub hurt_by_entity: Ev,
    /// The box entered / left powder snow.
    pub entered_powder_snow: Ev,
    pub left_powder_snow: Ev,
    /// The feet left the ground (after being on it).
    pub left_ground: Ev,
    /// A player's view vector changed (the id).
    pub gaze_changed: Ev<u32>,
    /// A player entered / left the `PlayerWatch` radius of an entity (the watcher, the player id).
    pub player_entered: Ev<(Entity, u32)>,
    pub player_left: Ev<(Entity, u32)>,
    /// An attacker got a target / lost it (`targeting::set_target`, `clear_target`).
    pub target_acquired: Ev,
    pub target_lost: Ev,
    /// A projectile hurt this creature or player-less target (`projectile.rs`): the victim.
    pub projectile_hit: Ev,
    pub exploded: Ev<crate::projectile::Blast>,
    pub effect_changed: Ev,
    /// A creature picked up an item from the ground (`item::pick_up`): the creature.
    pub picked_up: Ev,
    /// A rider took the seat of this mount / left it (`mount.rs`).
    pub mounted: Ev,
    pub dismounted: Ev,
    /// A lead was put on this creature / taken off it (`leash.rs`).
    pub leashed: Ev,
    pub unleashed: Ev,
    /// The vehicle of a player moved, stopped or went (`vehicle.rs`): the player's id.
    pub vehicle_moved: Ev<u32>,
    /// A parent took `Carrying` (a pregnancy, `breed`).
    pub carrying: Ev,
    /// A slot of `Equipment` changed (`equipment::set_slot`).
    pub equipment_changed: Ev,
    /// A vibration happened (`cow::apply_damage`, `projectile.rs` at impact, the host's pushes): read by `vibration_listen`.
    pub vibration: Ev<crate::behaviours::vibration_listen::Vibration>,
    /// A creature touched another (`touch.rs`): (the toucher, the other).
    pub touched: Ev<(Entity, Entity)>,
    /// A toucher touched an item on the ground (`touch.rs`): (the toucher, the item).
    pub item_touched: Ev<(Entity, Entity)>,
    /// An item appeared on the ground (`item::drop_item`): the item (read by `move_to_item`).
    pub item_dropped: Ev,
    /// A creature died of a damage hit (`cow::apply_damage`): read by `celebrate`, the targeters of the victim party.
    pub killed: Ev<crate::behaviours::celebrate::Kill>,
}
impl Events {
    pub fn swap(&mut self) {
        self.ignited.swap();
        self.extinguished.swap();
        self.entered_fluid.swap();
        self.left_fluid.swap();
        self.item_dropped.swap();
        self.landed.swap();
        self.mode_changed.swap();
        self.grew.swap();
        self.ate.swap();
        self.target_acquired.swap();
        self.target_lost.swap();
        self.projectile_hit.swap();
        self.exploded.swap();
        self.effect_changed.swap();
        self.hurt.swap();
        self.hurt_by_entity.swap();
        self.entered_powder_snow.swap();
        self.left_powder_snow.swap();
        self.left_ground.swap();
        self.gaze_changed.swap();
        self.player_entered.swap();
        self.player_left.swap();
        self.picked_up.swap();
        self.mounted.swap();
        self.dismounted.swap();
        self.leashed.swap();
        self.unleashed.swap();
        self.vehicle_moved.swap();
        self.carrying.swap();
        self.equipment_changed.swap();
        self.vibration.swap();
        self.touched.swap();
        self.item_touched.swap();
        self.killed.swap();
    }
}

// ---- the wheel and the helpers the modules use --------------------------------------------------------------
/// One draw of the entity's own generator against the probability `p`.
pub(crate) fn chance(w: &mut World, e: Entity, p: f64) -> bool {
    with_rng(w, e, |r| r.next_f64() < p)
}
pub(crate) fn with_rng<T>(w: &mut World, e: Entity, f: impl FnOnce(&mut Rng) -> T) -> T {
    f(&mut w.get_mut::<Rng>(e).unwrap())
}
/// Arms a deadline of the goal of this slot, `wait` ticks from now.
pub(crate) fn arm(w: &mut World, e: Entity, slot: u8, wait: i32) {
    let now = crate::cow::now_of(w, e);
    crate::cow::arm_at(w, e, now + wait.max(1) as u64, Due::Goal(slot));
}
/// The rank of the module's (first) entry in the animal's selector.
pub fn slot_of(w: &World, e: Entity, m: &'static Module) -> Option<u8> {
    w.get::<GoalSet>(e)?.slot_of(m)
}
/// Arms a deadline of the module's first entry (for the code that knows the module, not the slot).
pub fn arm_module(w: &mut World, e: Entity, m: &'static Module, wait: i32) {
    if let Some(s) = slot_of(w, e, m) {
        arm(w, e, s, wait);
    }
}
/// Asks the selector to wake the module's first entry at the next selector tick.
pub fn wake(w: &mut World, e: Entity, m: &'static Module) {
    if let Some(s) = slot_of(w, e, m) {
        w.resource_mut::<Requests>().0.push(Req::Start(e, s));
    }
}
/// The goal of this slot waits for its flags to free, then starts (the selector's own pending mark).
pub(crate) fn pend(w: &mut World, e: Entity, slot: u8) {
    if let Some(mut g) = w.get_mut::<GoalSet>(e) {
        g.pending |= 1 << slot;
    }
}
/// Asks the selector to end the module's first entry.
pub fn end(w: &mut World, e: Entity, m: &'static Module) {
    if let Some(s) = slot_of(w, e, m) {
        w.resource_mut::<Requests>().0.push(Req::End(e, s));
    }
}
/// Imposes the spot the module's goal takes next instead of drawing one (the comparison tests).
pub fn force_spot(w: &mut World, e: Entity, m: &'static Module, spot: (f64, f64)) {
    if let Some(s) = slot_of(w, e, m) {
        w.entity_mut(e).insert(ForcedSpot(s, spot));
    }
}
/// Fires the module's deadline of the animal now (the comparison tests).
pub fn fire(w: &mut World, e: Entity, m: &'static Module) {
    if let Some(s) = slot_of(w, e, m) {
        w.resource_mut::<Fired>().0.push((e, s));
    }
}
/// The phase of the animal's selector (the tick + phase even ones are its ticks).
pub(crate) fn phase_of(w: &World, e: Entity) -> u64 {
    w.get::<Phase>(e).map_or(0, |p| p.0)
}
/// Asks every module of the animal for its first deadlines.
pub fn arm_timers(w: &mut World, e: Entity) {
    crate::brain::arm(w, e);
    let Some(g) = w.get::<GoalSet>(e) else {
        // No goals (a mannequin, a display...): it is still an entity the zone puts to sleep where no chunk ticks.
        crate::zone::admit(w, &[e]);
        return;
    };
    let (entries, n) = (g.entries, g.len());
    let insts: [Inst; MAX_ENTRIES] = std::array::from_fn(|i| Inst { slot: i as u8, nth: g.nth[i] });
    for (i, en) in entries.iter().enumerate().take(n) {
        (en.module.arm)(w, e, insts[i]);
    }
    // A creature created in a chunk that does not tick is born asleep (a `Mob` there is not ticked either): this is the last
    // step of every creation, once its deadlines exist.
    crate::zone::admit(w, &[e]);
}
/// Arms the timers of every animal (the herd starts active).
pub fn arm_all(world: &mut World) {
    let ids: Vec<Entity> = world.query_filtered::<Entity, With<GoalSet>>().iter(world).collect();
    for e in ids {
        arm_timers(world, e);
    }
    // The entities without goals have nothing to arm, but the zone owns them like any other non-player entity.
    let bare: Vec<Entity> = world.query_filtered::<Entity, (With<crate::cow::Cell>, Without<GoalSet>, Without<crate::zone::Dormant>)>().iter(world).collect();
    crate::zone::admit(world, &bare);
}
fn entered_events(mut en: ResMut<Entered>, mut reqs: ResMut<Requests>) {
    if !en.0.is_empty() {
        reqs.0.extend(std::mem::take(&mut en.0).into_iter().map(Req::Arm));
    }
}
/// The player events of the tick: the lazy look wakes the animals a player now watches, the list of the players is
/// given to the other systems; the modules read `Online::seen` for their own wake-ups (a player takes an item...).
/// An entity that wants to know when players come within `radius` blocks and go: the framework keeps `inside` and writes
/// `Events.player_entered` / `player_left` when a player moves, arrives or leaves, or when the entity changes cell (by the index
/// and the cell moves, never by a scan of everyone). Limit: an entity that moves inside its cell does not fire until its cell
/// changes or the player moves; the radius is at most 64 blocks. The players inside are a sorted list (any number of players).
#[derive(Component, Clone, Debug)]
pub struct PlayerWatch {
    pub radius: f32,
    inside: Vec<u32>,
}
impl PlayerWatch {
    pub const fn new(radius: f32) -> Self {
        Self { radius, inside: Vec::new() }
    }
    pub fn is_inside(&self, player: u32) -> bool {
        self.inside.binary_search(&player).is_ok()
    }
}
/// Updates the watchers touched by the player events of the tick and by the cell moves.
fn player_watch(on: Res<Online>, moves: Res<CellMoves>, idx: Res<Index>, mut q: Query<(&Pos, &mut PlayerWatch)>, mut ev: ResMut<Events>) {
    const R: f64 = 64.0;
    // Nobody watches (the common case): not one index scan per moving player.
    if (on.seen.is_empty() && moves.0.is_empty()) || q.is_empty() {
        return;
    }
    let mut eval = |e: Entity, pos: &Pos, w: &mut PlayerWatch, id: u32, p: Option<Player>, ev: &mut Events| {
        let now = p.is_some_and(|p| (pos.0.x - p.x).powi(2) + (pos.0.y - p.y).powi(2) + (pos.0.z - p.z).powi(2) <= f64::from(w.radius).powi(2));
        let at = w.inside.binary_search(&id);
        let was = at.is_ok();
        if now && !was {
            w.inside.insert(at.unwrap_err(), id);
            ev.player_entered.push((e, id));
        } else if !now && was {
            w.inside.remove(at.unwrap());
            ev.player_left.push((e, id));
        }
    };
    for (old, new) in &on.seen {
        let Some(id) = new.or(*old).map(|p| p.id) else { continue };
        let mut near: Vec<Entity> = Vec::new();
        for p in [old, new].into_iter().flatten() {
            idx.for_radius(p.x, p.z, R, |c| near.push(c));
        }
        near.sort_unstable();
        near.dedup();
        for e in near {
            if let Ok((pos, mut w)) = q.get_mut(e) {
                eval(e, pos, &mut w, id, *new, &mut ev);
            }
        }
    }
    for &e in &moves.0 {
        if let Ok((pos, mut w)) = q.get_mut(e) {
            for p in on.list.iter().flatten() {
                eval(e, pos, &mut w, p.id, Some(*p), &mut ev);
            }
        }
    }
}
pub(crate) fn online_events(
    mut on: ResMut<Online>,
    mut pl: ResMut<Players>,
    idx: Res<Index>,
    heads: Query<&Pos, With<GoalSet>>,
    mut lazy: ResMut<LazyLook>,
    mut reqs: ResMut<Requests>,
) {
    on.seen.clear();
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
                    if let Ok(p) = heads.get(c)
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
    on.seen = events;
}
/// The path a goal asked for ended (or none was found): the goal that owns it ends (called by `cow::path_ended`).
pub fn path_ended(e: Entity, g: &GoalSet, reqs: &mut Requests) {
    if let Some(s) = g.mover()
        && g.module(s).ends_with_path
    {
        reqs.0.push(Req::End(e, s));
    }
}


/// Whether some entity holds a component of SparseSet storage: two lookups, no query.
pub(crate) fn sparse_any<C: Component>(world: &World) -> bool {
    world.component_id::<C>().and_then(|id| world.storages().sparse_sets.get(id)).is_some_and(|s| !s.is_empty())
}
/// The ticks of the modules that have one (`Module::tick`), in the order of the registry, for those some live animal carries: one
/// test per module and tick, no system and no run condition in the schedule.
fn ticks_list() -> &'static [(usize, fn(&mut World))] {
    static TICKS: std::sync::OnceLock<Vec<(usize, fn(&mut World))>> = std::sync::OnceLock::new();
    TICKS.get_or_init(|| crate::species::registry().modules.iter().enumerate().filter_map(|(i, m)| m.tick.map(|t| (i, t))).collect())
}
/// The rank of module `i` among those that have a tick.
fn tick_pos(i: usize) -> Option<usize> {
    ticks_list().binary_search_by_key(&i, |t| t.0).ok()
}
fn tick_modules(world: &mut World) {
    let ticks = ticks_list();
    // The bits of the modules carried (kept by the hooks): one word, no walk of the list. Past 128 ticks the rest is looked at one by one.
    let mut live = world.resource::<Carriers>().2;
    while live != 0 {
        let j = live.trailing_zeros() as usize;
        live &= live - 1;
        (ticks[j].1)(world);
    }
    for &(i, t) in ticks.iter().skip(128) {
        if world.resource::<Carriers>().0.get(i).is_some_and(|n| *n > 0) {
            t(world);
        }
    }
    if sparse_any::<crate::behaviours::homing_projectile::Homing>(world) {
        crate::behaviours::homing_projectile::tick(world);
    }
    // The touches: looked at only while some creature carries a `Toucher` (no system in the schedule).
    if sparse_any::<crate::touch::Toucher>(world) {
        tick_state(world, crate::touch::touch_events);
    }
}
/// The entities that match a filter, through a query state kept in the world (one per filter): building a `QueryState` each tick costs thousands of
/// instructions, reading a kept one a few hundred.
#[derive(Resource)]
pub(crate) struct Cached<F: bevy_ecs::query::QueryFilter + 'static>(bevy_ecs::query::QueryState<Entity, F>);
pub(crate) fn entities<F: bevy_ecs::query::QueryFilter + 'static>(w: &mut World) -> Vec<Entity> {
    if !w.contains_resource::<Cached<F>>() {
        let q = w.query_filtered::<Entity, F>();
        w.insert_resource(Cached::<F>(q));
    }
    w.resource_scope(|w, mut c: Mut<Cached<F>>| c.0.iter(w).collect())
}
/// The parameters of a system function kept as a tick (`SystemState`, one per function).
#[derive(Resource)]
struct TickState<F: 'static, P: bevy_ecs::system::SystemParam + 'static>(bevy_ecs::system::SystemState<P>, std::marker::PhantomData<fn() -> F>);
/// A bevy system function run as the tick of a module: the function as it was in the schedule (same queries, resources and commands,
/// applied at once), its parameters fetched through a `SystemState` kept in the world: some 220 instructions an empty call, against 800
/// by `World::run_system` (measured, `profile::tick_forms`).
pub fn tick_state<M, F>(world: &mut World, mut f: F)
where
    F: bevy_ecs::system::SystemParamFunction<M, In = (), Out = ()> + Copy,
    F::Param: 'static,
{
    if !world.contains_resource::<TickState<F, F::Param>>() {
        let state = bevy_ecs::system::SystemState::<F::Param>::new(world);
        world.insert_resource(TickState::<F, F::Param>(state, std::marker::PhantomData));
    }
    world.resource_scope(|world, mut ts: Mut<TickState<F, F::Param>>| {
        if let Ok(item) = ts.0.get_mut(world) {
            f.run((), item);
        }
        ts.0.apply(world);
    });
}

// ---- the selector: start and stop, with the locks -------------------------------------------------------------
/// The selector's first step of the tick (`GoalSelector.tick`: the goals that start do so BEFORE the running goals are ticked, so a goal acts at the
/// tick it starts): the deadlines that came up and the starts asked (`Req::Start`, `Req::Arm`); the other requests wait for `arbitrate_end`.
pub(crate) fn arbitrate(world: &mut World) {
    crate::farm::on_grown(world);
    let fired = std::mem::take(&mut world.resource_mut::<Fired>().0);
    let reqs = std::mem::take(&mut world.resource_mut::<Requests>().0);
    for (e, slot) in fired {
        timer_fired(world, e, slot);
    }
    let mut rest = Vec::new();
    for r in reqs {
        match r {
            Req::Start(..) => apply(world, r),
            other => rest.push(other),
        }
    }
    if !rest.is_empty() {
        let mut q = world.resource_mut::<Requests>();
        rest.append(&mut q.0);
        q.0 = rest;
    }
}
/// The selector's second step, after the running goals' systems: the module ticks, then the requests those systems and the events pushed (calls of a
/// goal's step, ends, starts).
pub(crate) fn arbitrate_end(world: &mut World) {
    tick_modules(world);
    // The column changes the slots just read.
    world.resource_mut::<CellMoves>().0.clear();
    let reqs = std::mem::take(&mut world.resource_mut::<Requests>().0);
    for r in reqs {
        apply(world, r);
    }
}
fn apply(world: &mut World, r: Req) {
    match r {
        // An animal entering the active zone: its modules arm their deadlines.
        Req::Arm(e) => {
            if world.resource::<LazyLook>().enabled
                && let Some(mut h) = world.get_mut::<Head>(e)
            {
                // A head that was not simulated starts from the body's heading.
                h.yaw = f32::NAN;
            }
            arm_timers(world, e);
        }
        Req::Call(e, f) => f(world, e),
        // The goal selector asks `canUse` only on the ticks where tick + id is even: an event that wakes a goal is
        // heard at the next such tick (the same as the next one at most).
        Req::Start(e, k) => {
            let now = crate::cow::now_of(world, e);
            if now > 1 && (now + phase_of(world, e)) % 2 == 1 {
                arm(world, e, k, 1);
            } else {
                start_goal(world, e, k);
            }
        }
        Req::End(e, k) => end_goal(world, e, k),
    }
}
/// The module and instance of a slot, if the animal still has the selector and the slot.
fn of(w: &World, e: Entity, slot: u8) -> Option<(&'static Module, Inst)> {
    let g = w.get::<GoalSet>(e)?;
    ((slot as usize) < g.len()).then(|| (g.module(slot), g.inst(slot)))
}
fn timer_fired(w: &mut World, e: Entity, slot: u8) {
    // The kernel's own stroll timer (`Due::Stroll`) fires the animal's stroll goal, wherever it is in the list.
    let slot = if slot == crate::behaviours::stroll::FIRED { slot_of(w, e, &crate::behaviours::stroll::MODULE).unwrap_or(u8::MAX) } else { slot };
    let Some((m, inst)) = of(w, e, slot) else { return };
    if (m.fired)(w, e, inst) {
        start_goal(w, e, slot);
    }
}
/// Starts the goal of this slot without asking `can_use` and without its `start` (the caller does what `start` does): the
/// goals that hold its flags are stopped. For the tests that impose what a `Mob` drew (an acquisition at a given tick).
pub fn force_start(w: &mut World, e: Entity, slot: u8) {
    let stopped = {
        let Some(mut g) = w.get_mut::<GoalSet>(e) else { return };
        if slot as usize >= g.len() || g.running >> slot & 1 == 1 {
            return;
        }
        g.start(slot)
    };
    for s in 0..MAX_ENTRIES as u8 {
        if stopped >> s & 1 == 1
            && let Some((sm, si)) = of(w, e, s)
        {
            (sm.stop)(w, e, si);
        }
    }
    changed(w, e, Change::Started);
}
/// The entity took damage: every module of its selector reacts through its `damaged` entry point (a default that does nothing).
pub fn dispatch_damaged(w: &mut World, e: Entity, d: &crate::behaviours::Damaged) {
    crate::brain::dispatch_damaged(w, e, d);
    let Some(g) = w.get::<GoalSet>(e) else { return };
    for slot in 0..g.len() as u8 {
        let Some((m, inst)) = of(w, e, slot) else { return };
        (m.damaged)(w, e, inst, d);
    }
}
/// A player uses an item on the entity: the first module of its selector that answers true handles it (`Mob.mobInteract`), then
/// its rules (shearing); false when nothing did.
pub fn interact(w: &mut World, e: Entity, i: &crate::behaviours::Interaction) -> bool {
    if let Some(g) = w.get::<GoalSet>(e) {
        for slot in 0..g.len() as u8 {
            let Some((m, inst)) = of(w, e, slot) else { break };
            if (m.interact)(w, e, inst, i) {
                return true;
            }
        }
    }
    let rules = w.get::<SpeciesId>(e).map_or(&[][..], |s| w.resource::<SpeciesTable>().0[s.0 as usize].rules);
    rules.iter().any(|r| (r.rule.interact)(w, e, r.params, i))
}
pub(crate) fn try_start(w: &mut World, e: Entity, slot: u8) {
    start_goal(w, e, slot);
}
fn start_goal(w: &mut World, e: Entity, slot: u8) {
    // One lookup of the selector: the module, whether it runs, whether its flags are free.
    let (free, m, inst, when) = {
        let Some(mut g) = w.get_mut::<GoalSet>(e) else { return };
        if slot as usize >= g.len() || g.running >> slot & 1 == 1 {
            return;
        }
        let free = g.replaceable(slot);
        if free {
            g.pending &= !(1 << slot);
        }
        (free, g.module(slot), g.inst(slot), g.entries[slot as usize].when)
    };
    if !free {
        match m.blocked {
            Some(f) => f(w, e, inst),
            None => w.get_mut::<GoalSet>(e).unwrap().pending |= 1 << slot,
        }
        return;
    }
    // The species' condition for this entry, before the module's own `canUse`; a refusal is a `blocked` (ask again later).
    if let Some(c) = when
        && !c.eval(w, e)
    {
        if let Some(f) = m.blocked {
            f(w, e, inst);
        }
        return;
    }
    if !(m.can_use)(w, e, inst) {
        return;
    }
    let stopped = w.get_mut::<GoalSet>(e).unwrap().start(slot);
    for s in 0..MAX_ENTRIES as u8 {
        if stopped >> s & 1 == 1 {
            let (sm, si) = of(w, e, s).unwrap();
            (sm.stop)(w, e, si);
        }
    }
    (m.start)(w, e, inst);
    changed(w, e, Change::Started);
}
/// A goal started or ended: the modules that follow the flags (the tempt goal's calm-down) are told, once each.
fn changed(w: &mut World, e: Entity, c: Change) {
    let Some(g) = w.get::<GoalSet>(e) else { return };
    if !g.watchers {
        return;
    }
    let list: Vec<(fn(&mut World, Entity, Change), ())> = g
        .entries
        .iter()
        .enumerate()
        .filter(|(i, en)| g.nth[*i] == 0 && en.module.after_change.is_some())
        .map(|(_, en)| (en.module.after_change.unwrap(), ()))
        .collect();
    for (f, ()) in list {
        f(w, e, c);
    }
}
/// The flags no goal of the entity may take (0: none): the goals that hold them end, the others wait; lifting the ban lets the waiting
/// ones start again (`Mob.goals.disabled`).
pub fn set_disabled_flags(w: &mut World, e: Entity, mask: u8) {
    let Some(mut g) = w.get_mut::<GoalSet>(e) else { return };
    if g.disabled == mask {
        return;
    }
    g.disabled = mask;
    let n = g.len() as u8;
    if mask != 0 {
        let held: Vec<u8> = (0..n).filter(|&s| g.slot_running(s) && g.module(s).flags & mask != 0).collect();
        for s in held {
            end_goal(w, e, s);
        }
    } else {
        let waiting: Vec<u8> = (0..n).filter(|&s| g.pending >> s & 1 == 1).collect();
        for s in waiting {
            start_goal(w, e, s);
        }
    }
}
pub(crate) fn end_goal(w: &mut World, e: Entity, slot: u8) {
    let Some(mut g) = w.get_mut::<GoalSet>(e) else { return };
    if slot as usize >= g.len() || !g.stop(slot) {
        return;
    }
    let (m, inst) = (g.module(slot), g.inst(slot));
    (m.stop)(w, e, inst);
    // The selector asks the goals in registration order: a goal that is still wanted comes back before a lower goal
    // that was waiting for its flag (the panic).
    if m.retry_on_end {
        start_goal(w, e, slot);
    }
    // A goal that was waiting for the flags it held tries again, in registration order.
    let g = w.get::<GoalSet>(e).unwrap();
    let waiting: Vec<u8> = (0..g.len() as u8).filter(|i| g.pending >> i & 1 == 1).collect();
    changed(w, e, Change::Ended);
    for p in waiting {
        start_goal(w, e, p);
    }
}

/// (name, animals with the module active) for the bench and the tests.
pub fn counts(world: &mut World) -> Vec<(&'static str, usize)> {
    macro_rules! n {
        ($t:ty) => {
            world.query_filtered::<(), With<$t>>().iter(world).count()
        };
    }
    vec![
        ("following", n!(Following)),
        ("moving", n!(Moving)),
        ("hunting", n!(crate::targeting::Target)),
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

// ---- the look control and the services of the goals ----------------------------------------------------------
fn rotate_towards(a: f32, b: f32, max: f32) -> f32 {
    a + wrap(b - a).clamp(-max, max)
}
fn wrap(a: f32) -> f32 {
    // `a % 360.0` is a library call: below 360 in magnitude it returns `a` itself.
    let mut a = if a.abs() < 360.0 { a } else { a % 360.0 };
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
            *h = Head { yaw: yaw.0, pitch: 0.0, body: yaw.0, stable: yaw.0, stable_time: 0, at: (pos.0.x, pos.0.z), angles: None, settled: None };
        }
        if look.cooldown > 0 && h.settled == Some((look.target, pos.0, look.y_speed, walking)) {
            look.cooldown -= 1;
            continue;
        }
        let before = (h.yaw, h.pitch, h.body, h.stable);
        let aimed = look.cooldown > 0;
        h.pitch = 0.0;
        if look.cooldown > 0 {
            look.cooldown -= 1;
            let (ty, tp) = match h.angles {
                Some((t, p, ty, tp)) if t == look.target && p == pos.0 => (ty, tp),
                _ => {
                    let (dx, dy, dz) = (look.target[0] - pos.0.x, look.target[1] - (pos.0.y + look.eye), look.target[2] - pos.0.z);
                    let sd = (dx * dx + dz * dz).sqrt();
                    let ty = (dz.abs() > 1.0e-5 || dx.abs() > 1.0e-5).then(|| trig::atan2_degrees(dz, dx) - 90.0);
                    let tp = (dy.abs() > 1.0e-5 || sd > 1.0e-5).then(|| -trig::atan2_degrees(dy, sd));
                    h.angles = Some((look.target, pos.0, ty, tp));
                    (ty, tp)
                }
            };
            if let Some(ty) = ty {
                h.yaw = rotate_towards(h.yaw, ty, look.y_speed);
            }
            if let Some(tp) = tp {
                h.pitch = rotate_towards(h.pitch, tp, look.x_max);
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
        h.settled = (aimed && !moved && h.stable_time > 20 && before == (h.yaw, h.pitch, h.body, h.stable)).then_some((look.target, pos.0, look.y_speed, walking));
        if look.cooldown == 0 && !moved && wrap(h.yaw - h.body).abs() < 0.01 && h.pitch == 0.0 {
            cmd.entity(e).remove::<Looking>();
        }
    }
}


/// Path searches asked, by goal: 0 stroll, 1 panic, 2 tempt, 3 breed, 4 follow parent, 5 melee (benches read them).
#[cfg(feature = "probes")]
pub static ASKS: [std::sync::atomic::AtomicU64; 32] = [const { std::sync::atomic::AtomicU64::new(0) }; 32];
/// The searches `pathfind` ran.
#[cfg(feature = "probes")]
pub static FINDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// The nodes of the paths those searches found (the mean length of a path is `PATH_NODES / paths found`).
#[cfg(feature = "probes")]
pub static PATH_NODES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(feature = "probes")]
pub static PATHS_FOUND: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub(crate) fn go_to(cmd: &mut Commands, e: Entity, x: f64, z: f64, speed: f64, goal: usize) {
    crate::pathsearch::probe!(ASKS[goal].fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    cmd.queue(move |w: &mut World| {
        let e = crate::mount::nav_owner(w, e);
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
/// A path to `at` (x, y, z: the row of the block is `floor(y)`) at `speed`, ending `reach` blocks from it: the way for a module to walk
/// somewhere; it writes nothing of its own in the walking state. The end of the path is the `path_ended` event.
pub fn go_to_at(w: &mut World, e: Entity, at: [f64; 3], speed: f64, reach: i32) {
    let e = crate::mount::nav_owner(w, e);
    if let Some(mut s) = w.get_mut::<Stroll>(e) {
        s.tx = at[0];
        s.ty = at[1].ifloor();
        s.tz = at[2];
    }
    if let Some(mut n) = w.get_mut::<Nav>(e) {
        n.speed = speed;
        n.reach = reach;
    }
    w.entity_mut(e).insert(NeedsPath);
}
/// The reach of the next paths of the entity (`go_to_at` sets it; a goal that ends puts the default 1 back).
pub fn set_reach(w: &mut World, e: Entity, reach: i32) {
    let e = crate::mount::nav_owner(w, e);
    if let Some(mut n) = w.get_mut::<Nav>(e) {
        n.reach = reach;
    }
}
/// `go_to_at` toward the position of the entity `target` now (follow: ask again as it moves; the module decides when). False
/// when the target has no position.
pub fn go_to_entity(w: &mut World, e: Entity, target: Entity, speed: f64, reach: i32) -> bool {
    let Some(p) = w.get::<Pos>(target).map(|p| p.0) else { return false };
    go_to_at(w, e, [p.x, p.y, p.z], speed, reach);
    true
}
/// A velocity written outside the physics wakes the body: always through these (`Moving` is the physics' wake marker).
pub fn set_velocity(w: &mut World, e: Entity, v: Vec3) {
    if let Some(mut vel) = w.get_mut::<Vel>(e) {
        vel.0 = v;
        w.entity_mut(e).insert(Moving);
    }
}
/// `Entity.push(x, y, z)`: adds to the velocity and wakes the body.
pub fn impulse(w: &mut World, e: Entity, dv: Vec3) {
    if let Some(mut vel) = w.get_mut::<Vel>(e) {
        vel.0 = Vec3::new(vel.0.x + dv.x, vel.0.y + dv.y, vel.0.z + dv.z);
        w.entity_mut(e).insert(Moving);
    }
}
/// The game rule `mobGriefing` of the world (a mob that eats grass, breaks doors or takes blocks asks it).
pub fn mob_griefing(w: &World) -> bool {
    w.get_resource::<Level>().is_some_and(|l| l.mw.0.mob_griefing)
}
/// The places an animal cannot live in (the dimension of `piglins_zombify`, a drowned out of water): a predicate the level
/// configures. Absent: no place is unsafe.
#[derive(Resource, Clone, Copy)]
pub struct UnsafePlace(pub fn(&World, Entity) -> bool);
/// Whether `e` stands in a place the configured `UnsafePlace` names.
pub fn in_unsafe_place(w: &World, e: Entity) -> bool {
    w.get_resource::<UnsafePlace>().is_some_and(|p| (p.0)(w, e))
}
pub(crate) fn stop_nav(w: &mut World, e: Entity) {
    let e = crate::mount::nav_owner(w, e);
    if let Some(mut n) = w.get_mut::<Nav>(e) {
        n.path = None;
        n.reach = 1;
    }
    if let Some(mut d) = w.get_mut::<Drive>(e) {
        d.zza = 0.0;
        d.xxa = 0.0;
        d.jump = false;
    }
    if let Some(mut m) = w.get_mut::<MoveTo>(e) {
        m.active = false;
    }
    w.entity_mut(e).remove::<(Following, NeedsPath)>();
}
/// `Mob.getLookControl().setLookAt(x, y, z, yMaxRotSpeed, xMaxRotAngle)`: the same with the vertical bound of this look.
pub fn aim_clamped(cmd: &mut Commands, e: Entity, look: &mut Look, looking: bool, at: [f64; 3], y_speed: Option<f32>, x_max: Option<f32>) {
    look.x_max = x_max.unwrap_or(look.x_default);
    aim(cmd, e, look, looking, at, y_speed);
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


pub(crate) type Aim<'a> = (Entity, &'a Pos, &'a mut Look, Has<Looking>, Has<Following>, Has<NeedsPath>);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::{self, breed, float, follow_parent, look_at_player, panic, random_look, stroll, tempt};
    use crate::behaviours::{eat_block, melee};

    /// The item that tempts the species of the process (`HORIZON_SPECIES`).
    fn item() -> u32 {
        last_tempt().items.iter().map(|n| item_id(n)).find(|&i| i != 0).unwrap()
    }
    fn last_tempt() -> &'static tempt::Params {
        let n = species().goals.entries.iter().filter(|e| std::ptr::eq(e.module, &tempt::MODULE)).count();
        species().goals.param::<tempt::Params>(&tempt::MODULE, n - 1).unwrap()
    }
    fn panic_speed() -> f64 {
        species().goals.param::<panic::Params>(&panic::MODULE, 0).unwrap().speed
    }
    fn look_range() -> f64 {
        species().goals.param::<look_at_player::Params>(&look_at_player::MODULE, 0).unwrap().range
    }
    /// Marks the animal's ask timers as armed (or not), so that none is armed by itself (or all are, by `arm_all`).
    fn set_timers(h: &mut Herd, e: Entity, armed: bool) {
        if let Some(mut g) = h.world.get_mut::<look_at_player::Glance>(e) {
            g.armed = armed;
        }
        if let Some(mut g) = h.world.get_mut::<look_at_player::Glance2>(e) {
            g.0.armed = armed;
        }
        if let Some(mut s) = h.world.get_mut::<Scan>(e) {
            s.armed = armed;
        }
        if let Some(mut g) = h.world.get_mut::<eat_block::Graze>(e) {
            g.armed = armed;
        }
    }
    fn herd() -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        Some(Herd::new_awake(0, 6.0, true, false, 3))
    }
    /// A cow that does nothing by itself: always active, no stroll and no look timer armed.
    fn cow(h: &mut Herd, x: f64, z: f64, seed: u64) -> Entity {
        let e = h.world.spawn(cow_bundle(x, z, seed)).id();
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.get_mut::<Stroll>(e).unwrap().scheduled = true;
        h.world.get_mut::<Vel>(e).unwrap().0.y = -0.08 * f64::from(0.98_f32);
        set_timers(h, e, true);
        e
    }
    fn player(h: &mut Herd, id: u32, x: f64, z: f64, item: u32) {
        h.world.resource_mut::<Online>().set(Player { id, x, y: 1.0, z, main: item, off: 0, ..Default::default() });
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
    fn running(h: &Herd, e: Entity, k: &'static Module) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(k)
    }

    /// The interruption rules of `GoalSelector`: a lower number replaces a higher one that holds a flag it needs.
    #[test]
    fn the_selector_interrupts_by_priority_and_flags() {
        let mut g = GoalSet::new(crate::species::config("minecraft:cow").entries);
        let s = |m: &'static Module| g_slot(m);
        fn g_slot(m: &'static Module) -> u8 {
            crate::species::config("minecraft:cow").entries.iter().position(|e| std::ptr::eq(e.module, m)).unwrap() as u8
        }
        let (stroll, random, tempt, panic, float, look, follow, breed) =
            (s(&stroll::MODULE), s(&random_look::MODULE), s(&tempt::MODULE), s(&panic::MODULE), s(&float::MODULE), s(&look_at_player::MODULE), s(&follow_parent::MODULE), s(&breed::MODULE));
        let mask = |slots: &[u8]| slots.iter().fold(0u32, |m, s| m | 1 << s);
        g.start(stroll);
        assert!(!g.replaceable(random), "random look (7) cannot take MOVE from the stroll (5)");
        assert!(g.replaceable(tempt) && g.replaceable(panic) && g.replaceable(float));
        g.start(look);
        assert_eq!(g.start(panic), mask(&[stroll]));
        assert!(!g.replaceable(tempt) && !g.replaceable(stroll) && !g.replaceable(breed));
        assert!(g.slot_running(look), "panic holds only MOVE");
        g.stop(panic);
        assert_eq!(g.start(tempt), mask(&[look]), "tempt (3) takes LOOK from look-at-player (6)");
        assert!(g.replaceable(follow) && g.start(follow) == 0, "no flags: never conflicts");
        assert!(!g.replaceable(look));
        g.stop(tempt);
        g.start(random);
        assert_eq!(g.start(stroll), mask(&[random]));
    }

    /// A struck cow flees at the speed modifier 2 to a spot within 5 blocks, during the walk only; the stroll it
    /// interrupted stops; a tempting player met during the panic waits for the flag and then tempts.
    #[test]
    fn a_struck_cow_panics_at_double_speed_and_blocks_the_lower_goals() {
    if crate::cow::skip_unless_small_body("a_struck_cow_panics_at_double_speed_and_blocks_the_lower_goals") {
        return;
    }
    if crate::cow::skip_unless_farm_goals("a_struck_cow_panics_at_double_speed_and_blocks_the_lower_goals") {
        return;
    }
        if crate::cow::skip_unless(&panic::MODULE, "a_struck_cow_panics_at_double_speed_and_blocks_the_lower_goals") {
            return;
        }
        let Some(mut h) = herd() else { return };
        let (e, s) = (cow(&mut h, 0.5, 0.5, 11), cow(&mut h, -10.5, -10.5, 12));
        player(&mut h, 1, 0.5, 6.5, 0);
        ticks(&mut h, 2);
        // The reference stroll: the same walk at modifier 1.
        h.world.resource_mut::<Fired>().0.push((s, stroll::FIRED));
        let mut stroll_steps = Vec::new();
        let mut prev = at(&h, s);
        for _ in 0..60 {
            h.tick();
            if has::<Following>(&h, s) {
                stroll_steps.push(dist(prev, at(&h, s)));
            }
            prev = at(&h, s);
        }
        assert!(running(&h, s, &stroll::MODULE) || stroll_steps.len() > 10);
        // The first cow walks (stroll), is struck, and panics.
        h.world.resource_mut::<Fired>().0.push((e, stroll::FIRED));
        ticks(&mut h, 3);
        assert!(running(&h, e, &stroll::MODULE));
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
                assert!(!running(&h, e, &stroll::MODULE), "panic took MOVE from the stroll");
                assert_eq!(h.world.get::<Nav>(e).unwrap().speed, panic_speed());
                let c = at(&h, e);
                player(&mut h, 2, c.x + 3.0, c.z, item());
            }
            assert!(!(running(&h, e, &panic::MODULE) && running(&h, e, &tempt::MODULE)), "panic and tempt share MOVE");
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
        // The recoil of the blow (0.4 of impulse, fading in about ten ticks) is not the walk: the walk is read after it.
        let fast = steps.iter().skip(12).cloned().fold(0.0, f64::max);
        let slow = stroll_steps.iter().cloned().fold(0.0, f64::max);
        let want = panic_speed().powi(2);
        assert!(fast / slow > want * 0.75 && fast / slow < want * 1.15, "{fast} / {slow}, expected about {want}");
        // Player 2 held wheat in range while it panicked: tempting began after the panic (and after the calm that the
        // interrupted tempt left), not during it.
        assert!(running(&h, e, &tempt::MODULE) || has::<Tempting>(&h, e));
        eprintln!("panic {started}..{ended}, step {fast:.3} vs stroll {slow:.3}");
    }

    /// Wheat within 10 blocks tempts, 10.5 does not; putting it away stops, with 100 ticks of calm.
    #[test]
    fn wheat_tempts_within_ten_blocks_and_stops_when_put_away() {
    if crate::cow::skip_unless_small_body("wheat_tempts_within_ten_blocks_and_stops_when_put_away") {
        return;
    }
    if crate::cow::skip_unless_farm_goals("wheat_tempts_within_ten_blocks_and_stops_when_put_away") {
        return;
    }
        if crate::cow::skip_unless(&tempt::MODULE, "wheat_tempts_within_ten_blocks_and_stops_when_put_away") {
            return;
        }
        let Some(mut h) = herd() else { return };
        let (near, far) = (cow(&mut h, -8.5, 0.5, 21), cow(&mut h, -10.5, 5.5, 22));
        ticks(&mut h, 2);
        player(&mut h, 1, 0.5, 0.5, item());
        ticks(&mut h, 3);
        assert!(has::<Tempting>(&h, near), "9 blocks away");
        assert!(!has::<Tempting>(&h, far), "11.2 blocks away");
        // The path is asked on the goal's ticks (every other one).
        ticks(&mut h, 3);
        assert_eq!(h.world.get::<Nav>(near).unwrap().speed, last_tempt().speed);
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
    if crate::cow::skip_unless_small_body("two_cows_in_love_make_a_baby_after_the_delay") {
        return;
    }
    if crate::cow::skip_unless_farm_goals("two_cows_in_love_make_a_baby_after_the_delay") {
        return;
    }
        if crate::cow::skip_unless(&breed::MODULE, "two_cows_in_love_make_a_baby_after_the_delay") {
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
        if crate::cow::skip_unless(&follow_parent::MODULE, "a_baby_follows_its_parent_beyond_three_blocks") {
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
        if crate::cow::skip_unless(&follow_parent::MODULE, "a_baby_follows_the_nearest_adult_that_walks_off") {
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
    if crate::cow::skip_unless_small_body("cows_glance_at_a_near_player_and_look_around") {
        return;
    }
    if crate::cow::skip_unless_farm_goals("cows_glance_at_a_near_player_and_look_around") {
        return;
    }
    if crate::cow::skip_unless_walker("cows_glance_at_a_near_player_and_look_around") {
        return;
    }
        let Some(mut h) = herd() else { return };
        // A ranged hunter holds the look flag as long as it has its target: the passive look is not what it does.
        if crate::cow::skip_if(&crate::behaviours::ranged_bow::MODULE, "cows_glance_at_a_near_player_and_look_around") {
            return;
        }
        // The control compares the rates of the look goals of every animal: the lazy look is off here.
        set_lazy_look(&mut h.world, false);
        let mut cows = Vec::new();
        for i in 0..15 {
            for j in 0..13 {
                let (x, z) = (-9.0 + 1.3 * f64::from(i), -8.0 + 1.3 * f64::from(j));
                cows.push((cow(&mut h, x, z, 100 + cows.len() as u64), x, z));
            }
        }
        for (e, _, _) in &cows {
            set_timers(&mut h, *e, false);
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
            if (x * x + z * z).sqrt() <= look_range() - 0.5 {
                near += 1;
                near_hit += usize::from(glanced[i]);
            } else if (x * x + z * z).sqrt() > look_range() + 0.5 {
                far_hit += usize::from(glanced[i]);
            }
            scanned += usize::from(seen_scan[i]);
        }
        let (rate, scan) = (near_hit as f64 / near as f64, scanned as f64 / cows.len() as f64);
        // 1 - 0.98^75 = 0.78 expected for a ask every 2 ticks over 150 ticks.
        // A species that hunts walks toward the player during the 150 ticks: a far one may glance once it is near.
        let hunts = species().goals.has(&melee::MODULE) || species().goals.has(&crate::behaviours::ranged_bow::MODULE);
        assert!((far_hit == 0 || hunts) && rate > 0.62 && rate < 0.92, "near {near_hit}/{near}, far {far_hit}");
        // A hunting species spends part of the 150 ticks in melee, which takes the flags of the look around.
        assert!(scan > if hunts { 0.5 } else { 0.62 } && scan < 0.92, "scan {scan}");
        let turned = cows.iter().filter(|c| h.world.get::<Head>(c.0).is_some_and(|hd| !hd.yaw.is_nan())).count();
        assert!(turned > 50, "{turned} heads moved");
        eprintln!("glance {rate:.2}, scan {scan:.2}");
    }

    /// A cow dropped in a pond floats: a jump 8 ticks in 10 while the water is deeper than 0.4, and it stays afloat.
    #[test]
    fn a_cow_in_a_pond_jumps_eight_ticks_in_ten() {
    if crate::cow::skip_unless_farm_goals("a_cow_in_a_pond_jumps_eight_ticks_in_ten") {
        return;
    }
    if crate::cow::skip_unless_walker("a_cow_in_a_pond_jumps_eight_ticks_in_ten") {
        return;
    }
        if crate::cow::skip_unless(&float::MODULE, "a_cow_in_a_pond_jumps_eight_ticks_in_ten") {
            return;
        }
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            return;
        }
        let mut h = Herd::with_hazards_awake(1, 100.0, true, false, 5, true);
        let e = h.world.spawn(cow_bundle(-39.0, -47.0, 51)).id();
        h.world.get_mut::<Pos>(e).unwrap().0.y = 3.0;
        h.world.get_mut::<Bounds>(e).unwrap().0 = box_at(Vec3::new(-39.0, 3.0, -47.0), crate::cow::species().width, crate::cow::species().height);
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

/// The player events of this tick, read outside the schedule (the benches that run a service alone).
pub fn online_events_for_bench(world: &mut World) {
    world.run_system_cached(online_events).unwrap();
}

pub use crate::goals_near::*;
