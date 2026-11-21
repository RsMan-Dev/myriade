//! The brain capability (`Cap::Brain`): memories with a time to live, sensors that scan on a cadence, activities, and
//! behaviours that start under conditions of memory, run, and stop (`Brain.tick` of the `Mob`, written once). Everything is
//! configuration: a species lists its sensors, activities and behaviours (`BrainDef`); a sensor or a behaviour is a static
//! module with entry points, like a goal; memories are NAMED keys (`MemKey`) whose ids are assigned at the registry's freeze.
//!
//! Nothing is polled: a sensor is a deadline of the wheel (its period), a memory expires by a deadline, and the brain of an
//! entity is stepped (`brain_step`) only when something woke it: a sensor scanned, a memory changed, a behaviour runs or is
//! polled. A brain that has nothing to do costs its sensors' scans. The brain needs the `Goals` capability (the random
//! stream, the look and the head) and `Locomotion` for the behaviours that walk.
use crate::IFloor as _;
use crate::behaviours::Cap;
use crate::cow::*;
use crate::goals::*;
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use std::any::Any;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use std::sync::Arc;

pub mod behaviours;
pub mod sensors;

/// Bench probe (`probes` feature, env `BRAINPROF`): instructions per sensor scan and per behaviour entry point inside `brain_step`.
/// The table is keyed by the addresses of the two names (open addressing, no hashing library); the row `step empty` is an empty scope: the
/// cost of measuring one, to subtract per call.
#[cfg(feature = "probes")]
pub mod bprobe {
    use std::cell::RefCell;
    type Row = ((usize, usize), &'static str, &'static str, u64, u64);
    const N: usize = 4096;
    thread_local! {
        static C: crate::profile::Counter = crate::profile::Counter::new();
        static T: RefCell<Vec<Option<Row>>> = RefCell::new(vec![None; N]);
    }
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    pub struct Scope(Option<(u64, &'static str, &'static str)>);
    pub fn scope(kind: &'static str, name: &'static str) -> Scope {
        if !*ON.get_or_init(|| std::env::var_os("BRAINPROF").is_some()) {
            return Scope(None);
        }
        Scope(Some((C.with(|c| c.read()), kind, name)))
    }
    impl Drop for Scope {
        fn drop(&mut self) {
            if let Some((a, kind, name)) = self.0.take() {
                let d = C.with(|c| c.read()) - a;
                let key = (kind.as_ptr() as usize, name.as_ptr() as usize);
                T.with(|t| {
                    let mut t = t.borrow_mut();
                    let mut h = (key.0.wrapping_mul(31) ^ key.1) >> 3;
                    loop {
                        let slot = &mut t[h % N];
                        match slot {
                            Some(r) if r.0 == key => {
                                r.3 += 1;
                                r.4 += d;
                                return;
                            }
                            None => {
                                *slot = Some((key, kind, name, 1, d));
                                return;
                            }
                            _ => h += 1,
                        }
                    }
                });
            }
        }
    }
    /// (label, calls, instructions) of this thread, heaviest first.
    pub fn dump() -> Vec<(String, u64, u64)> {
        let mut v: Vec<_> = T.with(|t| t.borrow().iter().flatten().map(|r| (format!("{} {}", r.1, r.2), r.3, r.4)).collect());
        v.sort_by_key(|x| std::cmp::Reverse(x.2));
        v
    }
    pub fn reset() {
        T.with(|t| t.borrow_mut().iter_mut().for_each(|x| *x = None));
    }
}

// ---- memories ---------------------------------------------------------------------------------------------------------
/// What a memory holds (the forms are few; `Opaque` is a value a mod owns and types).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Who,
    Whos,
    Block,
    Blocks,
    Walk,
    Look,
    Int,
    Bool,
    Unit,
    Opaque,
    /// A list of entities that is present even when empty (`NearestVisibleLivingEntities`).
    Seen,
}
/// A position a behaviour looks or walks at: an entity (at its eyes or feet), a block, a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tracker {
    Entity(Who, bool),
    Block([i32; 3]),
    Point([f64; 3]),
}
impl Tracker {
    pub fn position(&self, w: &World) -> Option<[f64; 3]> {
        match *self {
            Tracker::Entity(who, eye) => crate::targeting::info(w, who).map(|i| [i.pos.x, if eye { i.eye } else { i.pos.y }, i.pos.z]),
            Tracker::Block(p) => Some([f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5, f64::from(p[2]) + 0.5]),
            Tracker::Point(v) => Some(v),
        }
    }
    pub fn block(&self, w: &World) -> Option<[i32; 3]> {
        match *self {
            Tracker::Entity(who, _) => crate::targeting::info(w, who).map(|i| [i.pos.x.ifloor(), i.pos.y.ifloor(), i.pos.z.ifloor()]),
            Tracker::Block(p) => Some(p),
            Tracker::Point(v) => Some([v[0].ifloor(), v[1].ifloor(), v[2].ifloor()]),
        }
    }
}
/// `WalkTarget`: where to walk, how fast, how close is close enough (manhattan, in blocks).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkTarget {
    pub target: Tracker,
    pub speed: f32,
    pub close: i32,
}
#[derive(Clone, Debug)]
pub enum MemValue {
    Who(Who),
    Whos(Vec<Who>),
    Block([i32; 3]),
    Blocks(Vec<[i32; 3]>),
    Walk(WalkTarget),
    Look(Tracker),
    Int(i64),
    Bool(bool),
    Unit,
    Opaque(Arc<dyn Any + Send + Sync>),
}
static NEXT_MEM: AtomicU32 = AtomicU32::new(0);
/// A memory's name (namespaced) and shape; the registry gives it an id when a plan that uses it is built.
pub struct MemKey {
    pub name: &'static str,
    pub shape: Shape,
    id: AtomicU32,
}
impl MemKey {
    pub const fn new(name: &'static str, shape: Shape) -> Self {
        Self { name, shape, id: AtomicU32::new(u32::MAX) }
    }
    pub fn id(&self) -> usize {
        self.id.load(Relaxed) as usize
    }
    fn assign(&self) {
        if self.id.load(Relaxed) == u32::MAX {
            self.id.store(NEXT_MEM.fetch_add(1, Relaxed), Relaxed);
        }
    }
}
macro_rules! keys { ($($id:ident = $name:literal, $shape:ident;)*) => { $(pub static $id: MemKey = MemKey::new($name, Shape::$shape);)* } }
keys! {
    MOBS = "minecraft:mobs", Whos;
    VISIBLE_MOBS = "minecraft:visible_mobs", Seen;
    HURT_BY = "minecraft:hurt_by", Bool;
    HURT_BY_ENTITY = "minecraft:hurt_by_entity", Who;
    IS_PANICKING = "minecraft:is_panicking", Bool;
    WALK_TARGET = "minecraft:walk_target", Walk;
    LOOK_TARGET = "minecraft:look_target", Look;
    PATH = "minecraft:path", Unit;
    CANT_REACH_WALK_TARGET_SINCE = "minecraft:cant_reach_walk_target_since", Int;
    TRANSPORT_ITEMS_COOLDOWN_TICKS = "minecraft:transport_items_cooldown_ticks", Int;
    NEAREST_PLAYERS = "minecraft:nearest_players", Whos;
    NEAREST_VISIBLE_PLAYER = "minecraft:nearest_visible_player", Who;
    NEAREST_VISIBLE_ADULT = "minecraft:nearest_visible_adult", Who;
    NEAREST_VISIBLE_TARGETABLE_PLAYERS = "minecraft:nearest_visible_targetable_players", Whos;
    NEAREST_VISIBLE_TARGETABLE_PLAYER = "minecraft:nearest_visible_targetable_player", Who;
    TEMPTING_PLAYER = "minecraft:tempting_player", Who;
    TEMPTATION_COOLDOWN_TICKS = "minecraft:temptation_cooldown_ticks", Int;
    BREED_TARGET = "minecraft:breed_target", Who;
    ATTACK_TARGET = "minecraft:attack_target", Who;
    ATTACK_COOLING_DOWN = "minecraft:attack_cooling_down", Bool;
}

// ---- the modules and the configuration --------------------------------------------------------------------------------
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    Present,
    Absent,
}
pub type Needs = &'static [(&'static MemKey, Need)];
/// A sensor: a scan the brain makes every `period` ticks (the period is configuration).
pub struct Sensor {
    pub name: &'static str,
    /// The memories it writes.
    pub mems: &'static [&'static MemKey],
    /// A scan: the entry's `params` (its `Params`, with a `DEFAULT`: the range, the foods of a species) are in the `Ctx`.
    pub scan: fn(&mut World, Entity, &Ctx),
    /// Once at creation (the sensor's own state components from its parameters); the frame calls it, nobody else.
    pub equip: fn(&mut World, Entity, &Ctx),
    pub requires: &'static [Cap],
}
impl Sensor {
    /// A sensor is written `Sensor { name, mems, scan, ..Sensor::DEFAULT }`: a field added later touches no sensor.
    pub const DEFAULT: Sensor = Sensor { name: "", mems: &[], scan: no_scan, equip: nothing, requires: &[] };
}
fn no_scan(_: &mut World, _: Entity, _: &Ctx) {}
/// What a behaviour is given: its place in the plan and its parameters.
#[derive(Clone, Copy)]
pub struct Ctx {
    pub id: u16,
    pub nth: u8,
    pub params: &'static (dyn Any + Sync),
}
impl Ctx {
    /// This entry's parameters (its module's `Params`).
    pub fn get<T: 'static>(&self) -> &'static T {
        let p: &'static dyn Any = self.params;
        p.downcast_ref::<T>().expect("the parameters of another behaviour")
    }
}
/// A composite: its kids are tried in order (or shuffled, weighted), the first that starts wins unless `try_all`.
#[derive(Clone, Copy)]
pub struct Gate {
    pub shuffle: bool,
    pub try_all: bool,
}
/// A behaviour, written `Behaviour { name, <entry points>, ..Behaviour::DEFAULT }`.
pub struct Behaviour {
    pub name: &'static str,
    /// The memories it reads or writes (so that they have an id in the species' plan).
    pub mems: &'static [&'static MemKey],
    pub requires: &'static [Cap],
    /// Starts and stops in the same tick (`OneShot`).
    pub one_shot: bool,
    /// Nothing to do while it runs (no `tick` work): the brain is woken at its end, not every tick.
    pub passive: bool,
    /// Woken by an item dropped within this many blocks (0 = never): the brain sleeps, nothing polls for items.
    pub wakes_on_item: f64,
    /// While it runs and this says true, the brain is not stepped every tick: it is woken by its deadline, a memory write, or whatever the
    /// behaviour arranged itself (the end of the walk, a crossing).
    pub sleeps: fn(&World, Entity, &Ctx) -> bool,
    pub gate: Option<Gate>,
    pub equip: fn(&mut World, Entity, &Ctx),
    pub can_start: fn(&mut World, Entity, &Ctx) -> bool,
    pub start: fn(&mut World, Entity, &Ctx),
    /// Every tick while it runs, after the timeout check; false = stop.
    pub tick: fn(&mut World, Entity, &Ctx) -> bool,
    pub stop: fn(&mut World, Entity, &Ctx),
    /// The entity took damage (running or not): a reaction of the behaviour (`Animal.actuallyHurt` ends love), like a goal's `damaged`.
    pub damaged: fn(&mut World, Entity, &Ctx, &crate::behaviours::Damaged),
}
fn no_damage(_: &mut World, _: Entity, _: &Ctx, _: &crate::behaviours::Damaged) {}
fn yes(_: &mut World, _: Entity, _: &Ctx) -> bool {
    true
}
fn no(_: &World, _: Entity, _: &Ctx) -> bool {
    false
}
fn nothing(_: &mut World, _: Entity, _: &Ctx) {}
impl Behaviour {
    pub const DEFAULT: Behaviour = Behaviour {
        name: "",
        mems: &[],
        requires: &[],
        one_shot: false,
        passive: false,
        wakes_on_item: 0.0,
        gate: None,
        sleeps: no,
        equip: nothing,
        can_start: yes,
        start: nothing,
        tick: yes,
        stop: nothing,
        damaged: no_damage,
    };
}
#[derive(Clone, Copy)]
pub struct SensorEntry {
    pub sensor: &'static Sensor,
    pub period: i32,
    /// The sensor's typed parameters (its module's `Params`), `&()` when it has none.
    pub params: &'static (dyn Any + Sync),
}
impl SensorEntry {
    pub const fn new(sensor: &'static Sensor, period: i32) -> Self {
        Self { sensor, period, params: &() }
    }
    /// The same entry with its parameters: `SensorEntry::new(&tempting::SENSOR, 10).with(&tempting::Params { .. })`.
    pub const fn with(self, params: &'static (dyn Any + Sync)) -> Self {
        Self { params, ..self }
    }
}
#[derive(Clone, Copy)]
pub struct Kid {
    pub behaviour: &'static Behaviour,
    pub weight: i32,
    pub params: &'static (dyn Any + Sync),
    pub needs: Needs,
    pub duration: Option<(i32, i32)>,
}
#[derive(Clone, Copy)]
pub struct BEntry {
    pub behaviour: &'static Behaviour,
    pub activity: &'static str,
    pub priority: u8,
    pub params: &'static (dyn Any + Sync),
    /// The memories it needs to be started, and `Behavior`'s duration (min, max): drawn at the start, timed out after it.
    pub needs: Needs,
    pub duration: Option<(i32, i32)>,
    pub kids: &'static [Kid],
}
/// An activity: its name, its place in vanilla's hash map (the order of equal priorities), what it requires.
#[derive(Clone, Copy)]
pub struct ActivityDef {
    pub name: &'static str,
    pub slot: u8,
    pub requires: Needs,
}
/// A memory a brain starts with: `base + nextInt(range)` of the entity's random (the golem's transport cooldown).
#[derive(Clone, Copy)]
pub struct Initial {
    pub key: &'static MemKey,
    pub base: i64,
    pub range: i32,
}
/// A species' brain: sensors, activities (the first is `core`, always active), the order the activities are tried in to
/// pick the active one (the first whose requirements hold), and the behaviours.
#[derive(Clone, Copy)]
pub struct BrainDef {
    pub sensors: &'static [SensorEntry],
    pub activities: &'static [ActivityDef],
    pub pick: &'static [&'static str],
    pub initial: &'static [Initial],
    pub behaviours: &'static [BEntry],
}
impl BrainDef {
    pub const EMPTY: BrainDef = BrainDef { sensors: &[], activities: &[], pick: &[], initial: &[], behaviours: &[] };
    pub fn is_empty(&self) -> bool {
        self.behaviours.is_empty() && self.sensors.is_empty()
    }
}

// ---- the plan: the configuration resolved once ------------------------------------------------------------------------
pub struct BInst {
    pub b: &'static Behaviour,
    pub params: &'static (dyn Any + Sync),
    pub act: u8,
    pub prio: u8,
    pub parent: u16,
    pub weight: i32,
    pub kids: Vec<u16>,
    pub nth: u8,
    pub needs: Needs,
    /// `needs` resolved to the brain's slots: (slot, must be present); `NONE` = a memory this plan does not hold (never present).
    needs_l: Vec<(u16, bool)>,
    pub duration: Option<(i32, i32)>,
}
pub struct Plan {
    pub mems: Vec<&'static MemKey>,
    local: Vec<u16>,
    pub sensors: Vec<SensorEntry>,
    pub insts: Vec<BInst>,
    /// The top-level behaviours in `Brain.tick`'s order: priority, then the activity's slot, then insertion.
    pub order: Vec<u16>,
    pub acts: Vec<ActivityDef>,
    pick: Vec<u8>,
    /// `acts[i].requires` resolved like `BInst::needs_l`.
    act_req: Vec<Vec<(u16, bool)>>,
    initial: Vec<Initial>,
    /// The widest `wakes_on_item` of its behaviours.
    item_wake: f64,
}
const NONE: u16 = u16::MAX;
impl Plan {
    /// Resolves a species' brain: the ids of its memories, the flat table of its behaviours (the kids of a composite after
    /// the top level), the order.
    pub fn build(def: &BrainDef, caps: &[Cap]) -> Result<Plan, String> {
        let mut mems: Vec<&'static MemKey> = Vec::new();
        let mut note = |k: &'static MemKey| {
            k.assign();
            if !mems.iter().any(|m| std::ptr::eq(*m, k)) {
                mems.push(k);
            }
        };
        for s in def.sensors {
            s.sensor.mems.iter().for_each(|k| note(k));
            if let Some(c) = s.sensor.requires.iter().find(|c| !caps.contains(c)) {
                return Err(format!("sensor `{}` requires the capability {c:?}", s.sensor.name));
            }
        }
        for a in def.activities {
            a.requires.iter().for_each(|(k, _)| note(k));
        }
        for i in def.initial {
            note(i.key);
        }
        let act_of = |n: &str| def.activities.iter().position(|a| a.name == n).map(|i| i as u8);
        let mut insts: Vec<BInst> = Vec::new();
        let mut tops = Vec::new();
        for (i, e) in def.behaviours.iter().enumerate() {
            let b = e.behaviour;
            b.mems.iter().for_each(|k| note(k));
            e.needs.iter().for_each(|(k, _)| note(k));
            if let Some(c) = b.requires.iter().find(|c| !caps.contains(c)) {
                return Err(format!("behaviour `{}` requires the capability {c:?}", b.name));
            }
            let act = act_of(e.activity).ok_or_else(|| format!("behaviour `{}`: unknown activity `{}`", b.name, e.activity))?;
            let nth = def.behaviours[..i].iter().filter(|o| std::ptr::eq(o.behaviour, b)).count() as u8;
            tops.push(insts.len() as u16);
            insts.push(BInst { b, params: e.params, act, prio: e.priority, parent: NONE, weight: 1, kids: Vec::new(), nth, needs: e.needs, needs_l: Vec::new(), duration: e.duration });
        }
        for (t, e) in def.behaviours.iter().enumerate() {
            let parent = tops[t];
            for (k, kid) in e.kids.iter().enumerate() {
                kid.behaviour.mems.iter().for_each(|m| note(m));
                kid.needs.iter().for_each(|(m, _)| note(m));
                let id = insts.len() as u16;
                insts.push(BInst { b: kid.behaviour, params: kid.params, act: insts[parent as usize].act, prio: 0, parent, weight: kid.weight, kids: Vec::new(), nth: k as u8, needs: kid.needs, needs_l: Vec::new(), duration: kid.duration });
                insts[parent as usize].kids.push(id);
            }
        }
        let mut order = tops;
        order.sort_by_key(|&i| (insts[i as usize].prio, def.activities[insts[i as usize].act as usize].slot));
        let mut local = Vec::new();
        for (l, k) in mems.iter().enumerate() {
            if local.len() <= k.id() {
                local.resize(k.id() + 1, NONE);
            }
            local[k.id()] = l as u16;
        }
        let pick = def.pick.iter().filter_map(|n| act_of(n)).collect();
        let resolve = |needs: Needs| -> Vec<(u16, bool)> {
            needs.iter().map(|(k, n)| (local.get(k.id()).copied().unwrap_or(NONE), *n == Need::Present)).collect()
        };
        for i in insts.iter_mut() {
            i.needs_l = resolve(i.needs);
        }
        let act_req = def.activities.iter().map(|a| resolve(a.requires)).collect();
        let item_wake = insts.iter().map(|i| i.b.wakes_on_item).fold(0.0, f64::max);
        Ok(Plan { item_wake, act_req, mems, local, sensors: def.sensors.to_vec(), insts, order, acts: def.activities.to_vec(), pick, initial: def.initial.to_vec() })
    }
    fn local(&self, k: &MemKey) -> Option<usize> {
        self.local.get(k.id()).copied().filter(|&l| l != NONE).map(usize::from)
    }
}

// ---- the component ----------------------------------------------------------------------------------------------------
struct Slot {
    v: Option<MemValue>,
    token: u32,
    /// The own time at which it expires.
    until: u64,
}
#[derive(Clone, Default)]
struct Run {
    running: bool,
    /// The deadline wake of a sleeping behaviour is armed.
    armed: bool,
    end: u64,
    /// The kids of a composite, in the order they are tried.
    order: Vec<u16>,
}
/// The brain of an entity.
#[derive(Component)]
pub struct Brain {
    plan: &'static Plan,
    slots: Vec<Slot>,
    run: Vec<Run>,
    pub activity: u8,
    awake: bool,
    armed: Vec<bool>,
    /// `Visible`'s line-of-sight cache, emptied when `visible_mobs` is written.
    sight: Vec<(Who, bool)>,
}
impl Brain {
    fn holds(&self, needs: &[(u16, bool)]) -> bool {
        needs.iter().all(|&(l, want)| (l != NONE && self.slots[l as usize].v.is_some()) == want)
    }
    /// `active && !running && needs` of a top-level behaviour (the step's start loop).
    fn ready(&self, id: u16) -> bool {
        let i = &self.plan.insts[id as usize];
        (i.act == 0 || self.activity == i.act) && !self.run[id as usize].running && self.holds(&i.needs_l)
    }
    fn kid_ready(&self, id: u16) -> bool {
        !self.run[id as usize].running && self.holds(&self.plan.insts[id as usize].needs_l)
    }
}
/// A walk toward a fixed block, the brain asleep: it wakes when the walk ends or the entity comes within `close` (`MoveToTargetSink`).
#[derive(Component)]
pub struct SinkWatch {
    pub target: [i32; 3],
    pub close: i32,
}
fn wake_walks(
    mut ended: (RemovedComponents<Following>, RemovedComponents<NeedsPath>),
    mut q: Query<&mut Brain>,
    moved: Query<(Entity, &Pos, &SinkWatch), Changed<Pos>>,
    mut awake: ResMut<BrainAwake>,
) {
    for e in ended.0.read().chain(ended.1.read()) {
        if let Ok(mut b) = q.get_mut(e) && !b.awake {
            b.awake = true;
            awake.0.push(e);
        }
    }
    for (e, p, s) in &moved {
        let d = (p.0.x.ifloor() - s.target[0]).abs() + (p.0.y.ifloor() - s.target[1]).abs() + (p.0.z.ifloor() - s.target[2]).abs();
        if d <= s.close && let Ok(mut b) = q.get_mut(e) && !b.awake {
            b.awake = true;
            awake.0.push(e);
        }
    }
}
/// The entities to step this tick.
#[derive(Resource, Default)]
pub struct BrainAwake(pub Vec<Entity>);
/// The last damage an entity took (`LivingEntity.getLastDamageSource`): when (its own time), whether it makes it panic, by whom.
#[derive(Component, Clone, Copy, Debug)]
pub struct LastDamage {
    pub at: u64,
    pub panic: bool,
    pub by: Option<Who>,
}

pub fn insert_resources(w: &mut World) {
    w.insert_resource(BrainAwake::default());
}
/// A dropped item wakes the brains that want items within their range (the box test of the behaviour's own range).
fn wake_on_item(ev: Res<Events>, items: Query<&Pos, With<crate::item::Item>>, idx: Res<Index>, mut q: Query<(&mut Brain, &Pos)>, mut awake: ResMut<BrainAwake>) {
    const MAX: f64 = 32.0;
    for i in ev.item_dropped.iter() {
        let Ok(at) = items.get(i) else { continue };
        idx.for_radius(at.0.x, at.0.z, MAX, |c| {
            let Ok((mut b, me)) = q.get_mut(c) else { return };
            let r = b.plan.item_wake;
            if r > 0.0 && !b.awake && (me.0.x - at.0.x).abs() <= r && (me.0.y - at.0.y).abs() <= r && (me.0.z - at.0.z).abs() <= r {
                b.awake = true;
                awake.0.push(c);
            }
        });
    }
}
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems((wake_walks, wake_on_item, brain_step).chain().after(arbitrate).before(pathfind));
}
/// The components of the capability (the plan is the species' resolved brain).
pub fn bundle(plan: &'static Plan) -> Brain {
    let slots = plan.mems.iter().map(|_| Slot { v: None, token: 0, until: u64::MAX }).collect();
    Brain { plan, slots, run: vec![Run::default(); plan.insts.len()], activity: plan.pick.last().copied().unwrap_or(0), awake: false, armed: vec![false; plan.sensors.len()], sight: Vec::new() }
}
/// Creation: the starting memories, the behaviours' own state, the sensors' first scans (a random phase each, as `Mob`).
pub fn start(w: &mut World, e: Entity) {
    let Some(plan) = w.get::<Brain>(e).map(|b| b.plan) else { return };
    for i in &plan.initial {
        let v = i.base + i64::from(with_rng(w, e, |r| r.next_int(i.range)));
        set(w, e, i.key, MemValue::Int(v), Some(v as u64));
    }
    for (id, inst) in plan.insts.iter().enumerate() {
        (inst.b.equip)(w, e, &Ctx { id: id as u16, nth: inst.nth, params: inst.params });
    }
    for i in 0..plan.sensors.len() {
        (plan.sensors[i].sensor.equip)(w, e, &sensor_ctx(plan, i));
    }
    arm(w, e);
    wake(w, e);
}
/// `Brain.setActivity` by name: the activity holds until `update_activity` picks the next one. Costs nothing unless called.
pub fn set_activity(w: &mut World, e: Entity, name: &str) {
    let Some(mut b) = w.get_mut::<Brain>(e) else { return };
    if let Some(a) = b.plan.acts.iter().position(|a| a.name == name) {
        b.activity = a as u8;
    }
}
/// Starts the instances of behaviour `b` again on `e` (the host changed what they read) and wakes the brain.
pub fn restart(w: &mut World, e: Entity, b: &'static Behaviour) {
    let Some(plan) = w.get::<Brain>(e).map(|x| x.plan) else { return };
    for (id, inst) in plan.insts.iter().enumerate().filter(|(_, i)| std::ptr::eq(i.b, b)) {
        (b.start)(w, e, &Ctx { id: id as u16, nth: inst.nth, params: inst.params });
    }
    wake(w, e);
}
/// What a sensor is given: its place in the plan (`nth` among the entries of the same sensor) and its parameters.
fn sensor_ctx(plan: &Plan, i: usize) -> Ctx {
    let nth = plan.sensors[..i].iter().filter(|s| std::ptr::eq(s.sensor, plan.sensors[i].sensor)).count() as u8;
    Ctx { id: i as u16, nth, params: plan.sensors[i].params }
}
/// Arms the sensors that have no deadline (creation, the entity enters the active zone).
pub fn arm(w: &mut World, e: Entity) {
    let Some(b) = w.get::<Brain>(e) else { return };
    let (plan, armed) = (b.plan, b.armed.clone());
    for (i, s) in plan.sensors.iter().enumerate() {
        if !armed[i] {
            let first = with_rng(w, e, |r| r.next_int(s.period)).max(1) as u64;
            let now = now_of(w, e);
            arm_at(w, e, now + first, Due::Sensor(i as u8));
            w.get_mut::<Brain>(e).unwrap().armed[i] = true;
        }
    }
}
/// A deadline of a sensor: scan, and arm the next (nothing out of the active zone: the next entry arms again).
pub fn sensor_due(w: &mut World, e: Entity, i: u8) {
    let now = now_of(w, e);
    let Some(b) = w.get::<Brain>(e) else { return };
    let (plan, i) = (b.plan, i as usize);
    if w.get::<ActiveUntil>(e).is_some_and(|a| now >= a.0) {
        w.get_mut::<Brain>(e).unwrap().armed[i] = false;
        return;
    }
    expire(w, e, now);
    {
        bp!("scan", plan.sensors[i].sensor.name);
        (plan.sensors[i].sensor.scan)(w, e, &sensor_ctx(plan, i));
    }
    arm_at(w, e, now + plan.sensors[i].period as u64, Due::Sensor(i as u8));
    wake(w, e);
}
/// The memories whose time is up are forgotten (`Brain.forgetOutdatedMemories`, before the sensors).
fn expire(w: &mut World, e: Entity, now: u64) {
    let mut changed = false;
    if let Some(mut b) = w.get_mut::<Brain>(e) {
        for s in &mut b.slots {
            if s.v.is_some() && s.until <= now {
                s.v = None;
                s.token += 1;
                changed = true;
            }
        }
    }
    if changed {
        wake(w, e);
    }
}
/// A deadline of a memory: forgotten, if it was not written again.
pub fn forget_due(w: &mut World, e: Entity, local: u16, token: u32) {
    let now = now_of(w, e);
    let Some(mut b) = w.get_mut::<Brain>(e) else { return };
    if let Some(s) = b.slots.get_mut(local as usize)
        && s.token == token
        && s.v.is_some()
        && s.until <= now
    {
        s.v = None;
        s.token += 1;
        drop(b);
        wake(w, e);
    }
}
/// The entity took damage: each behaviour of its plan reacts through its `damaged` entry point (a default that does nothing).
/// Only on a damage, never per tick.
pub fn dispatch_damaged(w: &mut World, e: Entity, d: &crate::behaviours::Damaged) {
    let Some(plan) = w.get::<Brain>(e).map(|b| b.plan) else { return };
    for (id, inst) in plan.insts.iter().enumerate() {
        (inst.b.damaged)(w, e, &Ctx { id: id as u16, nth: inst.nth, params: inst.params }, d);
    }
}
/// Whether the timed behaviour of this `Ctx` is past its time (`Timed.timed_out`): read in its `stop` to tell the end of its time from an
/// interruption.
pub fn timed_out(w: &World, e: Entity, c: &Ctx) -> bool {
    w.get::<Brain>(e).is_some_and(|b| b.run[c.id as usize].end < now_of(w, e))
}
/// The brain has something to do at the next step.
/// Queues a brain that is marked awake but was left out of the list (it was asleep with the chunk).
pub fn requeue(w: &mut World, e: Entity) {
    if w.get::<Brain>(e).is_some_and(|b| b.awake) {
        w.resource_mut::<BrainAwake>().0.push(e);
    }
}
pub fn wake(w: &mut World, e: Entity) {
    let Some(mut b) = w.get_mut::<Brain>(e) else { return };
    if !b.awake {
        b.awake = true;
        w.resource_mut::<BrainAwake>().0.push(e);
    }
}

// ---- memory access -----------------------------------------------------------------------------------------------------
pub fn get<'a>(w: &'a World, e: Entity, k: &MemKey) -> Option<&'a MemValue> {
    let b = w.get::<Brain>(e)?;
    b.slots.get(b.plan.local(k)?)?.v.as_ref()
}
pub fn present(w: &World, e: Entity, k: &MemKey) -> bool {
    get(w, e, k).is_some()
}
pub fn walk_target(w: &World, e: Entity) -> Option<WalkTarget> {
    match get(w, e, &WALK_TARGET) {
        Some(MemValue::Walk(t)) => Some(*t),
        _ => None,
    }
}
pub fn look_target(w: &World, e: Entity) -> Option<Tracker> {
    match get(w, e, &LOOK_TARGET) {
        Some(MemValue::Look(t)) => Some(*t),
        _ => None,
    }
}
pub fn int(w: &World, e: Entity, k: &MemKey) -> Option<i64> {
    match get(w, e, k) {
        Some(MemValue::Int(v)) => Some(*v),
        _ => None,
    }
}
/// `Brain.setMemory` / `setMemoryWithExpiry`: present for this tick and `ttl` more.
pub fn set(w: &mut World, e: Entity, k: &MemKey, v: MemValue, ttl: Option<u64>) {
    let now = now_of(w, e);
    let Some(mut b) = w.get_mut::<Brain>(e) else { return };
    let Some(i) = b.plan.local(k) else { return };
    if k.shape == Shape::Whos && matches!(&v, MemValue::Whos(l) if l.is_empty()) {
        drop(b);
        return erase(w, e, k);
    }
    let s = &mut b.slots[i];
    // Written again with the same value (a schedule's slice, a re-sensed flag): only the expiry moves, nothing to re-evaluate.
    let same = matches!((&s.v, &v), (Some(MemValue::Unit), MemValue::Unit))
        || matches!((&s.v, &v), (Some(MemValue::Bool(a)), MemValue::Bool(c)) if a == c)
        || matches!((&s.v, &v), (Some(MemValue::Int(a)), MemValue::Int(c)) if a == c);
    s.v = Some(v);
    s.token += 1;
    s.until = ttl.map_or(u64::MAX, |t| now + t + 1);
    let token = s.token;
    drop(b);
    if let Some(t) = ttl {
        arm_at(w, e, now + t + 1, Due::Forget(i as u16, token));
    }
    if !same {
        wake(w, e);
    }
}
pub fn erase(w: &mut World, e: Entity, k: &MemKey) {
    let Some(mut b) = w.get_mut::<Brain>(e) else { return };
    let Some(i) = b.plan.local(k) else { return };
    if b.slots[i].v.is_some() {
        b.slots[i].v = None;
        b.slots[i].token += 1;
        drop(b);
        wake(w, e);
    }
}

// ---- the step ---------------------------------------------------------------------------------------------------------
/// `Brain.tick` for the entities that were woken: start what can start (in order), tick what runs, pick the activity.
pub fn brain_step(world: &mut World) {
    let list = std::mem::take(&mut world.resource_mut::<BrainAwake>().0);
    for e in list {
        // A creature asleep (its chunk does not tick) is not stepped: its `awake` flag stays up and `zone::wake` queues it again.
        if world.get::<crate::zone::Dormant>(e).is_some() {
            continue;
        }
        step(world, e);
    }
}
/// `Brain.isActive(activity)` by name: what a behaviour's `canStillUse` asks (the jar stops nothing when the activity changes).
pub fn is_active(w: &World, e: Entity, name: &str) -> bool {
    w.get::<Brain>(e).is_some_and(|b| b.plan.acts[b.activity as usize].name == name || b.plan.acts[0].name == name)
}
fn step(w: &mut World, e: Entity) {
    let Some(mut b) = w.get_mut::<Brain>(e) else { return };
    b.awake = false;
    let plan = b.plan;
    drop(b);
    bp!("step", "total");
    {
        bp!("step", "empty");
    }
    {
        bp!("step", "start loop");
        // One read of the brain finds the next behaviour that can be asked; the walk goes on after it (a start changes the flags).
        let mut i = 0;
        while i < plan.order.len() {
            let Some(k) = w.get::<Brain>(e).and_then(|b| plan.order[i..].iter().position(|&id| b.ready(id))) else { break };
            try_start(w, e, plan, plan.order[i + k]);
            i += k + 1;
        }
    }
    let mut any;
    {
        bp!("step", "tick loop");
        let mut i = 0;
        while i < plan.order.len() {
            let Some(k) = w.get::<Brain>(e).and_then(|b| plan.order[i..].iter().position(|&id| b.run[id as usize].running)) else { break };
            tick_or_stop(w, e, plan, plan.order[i + k]);
            i += k + 1;
        }
    }
    {
        bp!("step", "update_activity");
        update_activity(w, e, plan);
    }
    any = false;
    // The running flags are read in place (one lookup of the brain); the deadlines to arm are applied after the walk.
    let mut arm: Option<(usize, u64)> = None;
    let mut more: Vec<(usize, u64)> = Vec::new();
    if let Some(b) = w.get::<Brain>(e) {
        for (i, x) in plan.insts.iter().enumerate() {
            if x.b.gate.is_some() || x.b.passive || !b.run[i].running {
                continue;
            }
            bp!("sleeps", x.b.name);
            if !(x.b.sleeps)(w, e, &ctx(plan, i as u16)) {
                any = true;
                break;
            }
            if x.duration.is_some() && !b.run[i].armed {
                let t = (i, b.run[i].end);
                if arm.is_none() { arm = Some(t) } else { more.push(t) }
            }
        }
    }
    for (i, end) in arm.into_iter().chain(more) {
        w.get_mut::<Brain>(e).unwrap().run[i].armed = true;
        arm_at(w, e, end + 1, Due::BrainWake);
    }
    if any {
        wake(w, e);
    }
}
fn running(w: &World, e: Entity, id: u16) -> bool {
    w.get::<Brain>(e).is_some_and(|b| b.run[id as usize].running)
}
fn set_running(w: &mut World, e: Entity, id: u16, on: bool) {
    w.get_mut::<Brain>(e).unwrap().run[id as usize].running = on;
}
fn ctx(plan: &Plan, id: u16) -> Ctx {
    let i = &plan.insts[id as usize];
    Ctx { id, nth: i.nth, params: i.params }
}
/// `Behavior.tryStart`: the extra condition, the duration, the start; a composite tries its kids.
fn try_start(w: &mut World, e: Entity, plan: &'static Plan, id: u16) -> bool {
    let inst = &plan.insts[id as usize];
    let c = ctx(plan, id);
    let cs = { bp!("can_start", inst.b.name); (inst.b.can_start)(w, e, &c) };
    if !cs {
        return false;
    }
    if let Some((min, max)) = inst.duration {
        let d = min + with_rng(w, e, |r| r.next_int(max + 1 - min));
        let now = now_of(w, e);
        w.get_mut::<Brain>(e).unwrap().run[id as usize].end = now + d as u64;
        if inst.b.passive {
            arm_at(w, e, now + d as u64 + 1, Due::BrainWake);
        }
    }
    set_running(w, e, id, true);
    w.get_mut::<Brain>(e).unwrap().run[id as usize].armed = false;
    if let Some(g) = inst.b.gate {
        let mut order = inst.kids.clone();
        if g.shuffle {
            let forced = w.get_mut::<ForcedShuffle>(e).and_then(|mut q| q.0.pop_front());
            match forced {
                Some(p) => order = p.into_iter().filter_map(|k| inst.kids.get(k).copied()).collect(),
                None => {
                    let mut keyed: Vec<(f64, u16)> = order.iter().map(|&k| (-with_rng(w, e, |r| r.next_f64()).powf(1.0 / f64::from(plan.insts[k as usize].weight)), k)).collect();
                    keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
                    order = keyed.into_iter().map(|x| x.1).collect();
                }
            }
        }
        w.get_mut::<Brain>(e).unwrap().run[id as usize].order = order.clone();
        for k in order {
            if w.get::<Brain>(e).is_some_and(|b| b.kid_ready(k)) && try_start(w, e, plan, k) && !g.try_all {
                break;
            }
        }
        return true;
    }
    {
        bp!("start", inst.b.name);
        (inst.b.start)(w, e, &c);
    }
    if inst.b.one_shot {
        set_running(w, e, id, false);
    }
    true
}
/// `BehaviorControl.tickOrStop`.
fn tick_or_stop(w: &mut World, e: Entity, plan: &'static Plan, id: u16) {
    let inst = &plan.insts[id as usize];
    if inst.b.gate.is_some() {
        let mut i = 0;
        while let Some(k) = w.get::<Brain>(e).unwrap().run[id as usize].order.get(i).copied() {
            i += 1;
            if running(w, e, k) {
                tick_or_stop(w, e, plan, k);
            }
        }
        if !inst.kids.iter().any(|&k| running(w, e, k)) {
            do_stop(w, e, plan, id);
        }
        return;
    }
    let c = ctx(plan, id);
    let (end, timed) = (w.get::<Brain>(e).unwrap().run[id as usize].end, inst.duration.is_some());
    let alive = { bp!("tick", inst.b.name); !(timed && now_of(w, e) > end) && (inst.b.tick)(w, e, &c) };
    if !alive {
        do_stop(w, e, plan, id);
    }
}
fn do_stop(w: &mut World, e: Entity, plan: &'static Plan, id: u16) {
    let inst = &plan.insts[id as usize];
    set_running(w, e, id, false);
    for &k in &inst.kids {
        if running(w, e, k) {
            do_stop(w, e, plan, k);
        }
    }
    if inst.b.gate.is_none() {
        bp!("stop", inst.b.name);
        (inst.b.stop)(w, e, &ctx(plan, id));
    }
    wake(w, e);
}
/// `updateActivity`: the first activity of the brain's list whose requirements hold.
fn update_activity(w: &mut World, e: Entity, plan: &'static Plan) {
    let mut b = w.get_mut::<Brain>(e).unwrap();
    if let Some(&a) = plan.pick.iter().find(|&&a| b.holds(&plan.act_req[a as usize])) {
        if b.activity != a {
            b.activity = a;
        }
    }
}

// ---- the services the modules use -------------------------------------------------------------------------------------
/// `NearestVisibleLivingEntities.findClosest`: the first of `visible_mobs` that passes the filter and the sight test.
pub fn find_closest(w: &mut World, e: Entity, filter: impl Fn(&World, Who) -> bool) -> Option<Who> {
    // The list is read in place (no copy), once per candidate: the entries the filter refuses cost one test each, not one lookup of the memory.
    let mut i = 0;
    loop {
        let who = match get(w, e, &VISIBLE_MOBS) {
            Some(MemValue::Whos(l)) => {
                let Some(k) = l.iter().skip(i).position(|&x| filter(w, x)) else { return None };
                i += k;
                l[i]
            }
            _ => return None,
        };
        if visible_test(w, e, who) {
            return Some(who);
        }
        i += 1;
    }
}
/// Every entry of `visible_mobs` that passes the filter and the sight test, in the list's order.
pub fn find_all(w: &mut World, e: Entity, filter: impl Fn(&World, Who) -> bool) -> Vec<Who> {
    let list = match get(w, e, &VISIBLE_MOBS) {
        Some(MemValue::Whos(l)) => l.clone(),
        _ => return Vec::new(),
    };
    list.into_iter().filter(|&who| filter(w, who) && visible_test(w, e, who)).collect()
}
/// `NearestVisibleLivingEntities.contains`.
pub fn visible_contains(w: &mut World, e: Entity, who: Who) -> bool {
    let listed = matches!(get(w, e, &VISIBLE_MOBS), Some(MemValue::Whos(l)) if l.contains(&who));
    listed && visible_test(w, e, who)
}
/// `Sensor.isEntityTargetable`, cached per `visible_mobs` value.
fn visible_test(w: &mut World, e: Entity, who: Who) -> bool {
    if let Some(&(_, r)) = w.get::<Brain>(e).and_then(|b| b.sight.iter().find(|(k, _)| *k == who)) {
        return r;
    }
    let r = targetable(w, e, who);
    w.get_mut::<Brain>(e).unwrap().sight.push((who, r));
    r
}
pub(crate) fn clear_sight(w: &mut World, e: Entity) {
    if let Some(mut b) = w.get_mut::<Brain>(e) {
        b.sight.clear();
    }
}
/// `TargetingConditions.forNonCombat()` within the follow range, in sight.
fn targetable(w: &World, e: Entity, who: Who) -> bool {
    let Some(i) = crate::targeting::info(w, who) else { return false };
    let (Some(pos), Some(eye), Some(range)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<crate::targeting::Eye>(e), w.get::<crate::targeting::FollowRange>(e)) else { return false };
    let vis = match who {
        Who::Player(id) => w.resource::<crate::hostile::PlayerState>().visibility(id),
        Who::Entity(_) => 1.0,
    };
    if i.exempt && i.player {
        return false;
    }
    let reach = crate::targeting::sight_range(range.0, vis);
    if d2(pos, i.pos) > reach * reach {
        return false;
    }
    let mw = w.resource::<Level>().mw.0;
    mw.line_of_sight_for(Some(pos.y), horizon_core::geometry::Vec3::new(pos.x, pos.y + eye.0, pos.z), horizon_core::geometry::Vec3::new(i.pos.x, i.eye, i.pos.z))
}
/// `PathNavigation.moveTo`: a path to the block (exact: reach 0), at this speed modifier; the walk is `pathfind`'s.
pub fn walk_to(w: &mut World, e: Entity, at: [f64; 3], speed: f64) {
    if let Some(mut s) = w.get_mut::<Stroll>(e) {
        (s.tx, s.ty, s.tz) = (at[0], at[1].ifloor(), at[2]);
    }
    if let Some(mut n) = w.get_mut::<Nav>(e) {
        n.speed = speed;
        n.reach = 0;
    }
    w.entity_mut(e).insert(NeedsPath);
}
/// `nav.is_done()`: no path to follow and none being made.
pub fn nav_done(w: &World, e: Entity) -> bool {
    w.get::<Following>(e).is_none() && w.get::<NeedsPath>(e).is_none()
}
/// `LookControl.setLookAt`: the head turns toward the point for the next ticks.
pub fn look_at(w: &mut World, e: Entity, at: [f64; 3]) {
    let looking = w.get::<Looking>(e).is_some();
    if let Some(mut l) = w.get_mut::<Look>(e) {
        l.target = at;
        l.cooldown = 2;
        l.y_speed = l.y_default;
    }
    if !looking {
        w.entity_mut(e).insert(Looking);
    }
}
/// A spot of the species' own walk value (`spots.rs`), as the goals pick it.
pub fn pick(w: &mut World, e: Entity, p: crate::spots::Pick) -> Option<[f64; 3]> {
    crate::spots::pick_for(w, e, p)
}

/// The probe's view: the active activity, the behaviours that run, the memories that are present.
pub fn describe(w: &World, e: Entity) -> Option<String> {
    let b = w.get::<Brain>(e)?;
    let running: Vec<&str> = b.plan.insts.iter().enumerate().filter(|(i, _)| b.run[*i].running).map(|(_, x)| x.b.name).collect();
    let mems: Vec<&str> = b.plan.mems.iter().enumerate().filter(|(i, _)| b.slots[*i].v.is_some()).map(|(_, k)| k.name).collect();
    Some(format!("activity {} running {:?} memories {:?}", b.plan.acts[b.activity as usize].name, running, mems))
}
