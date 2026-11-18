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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Float,
    Panic,
    Breed,
    Tempt,
    FollowParent,
    Stroll,
    LookAtPlayer,
    RandomLook,
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
/// The goals of the cow, in the order and with the priorities of `farm.rs`.
pub fn cow_slots() -> [(Kind, u8, u8); 8] {
    [
        (Kind::Float, 0, JUMP),
        (Kind::Panic, 1, MOVE),
        (Kind::Breed, 2, MOVE | LOOK),
        (Kind::Tempt, 3, MOVE | LOOK),
        (Kind::FollowParent, 4, 0),
        (Kind::Stroll, 5, MOVE),
        (Kind::LookAtPlayer, 6, LOOK),
        (Kind::RandomLook, 7, MOVE | LOOK),
    ]
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
/// `TemptGoal`: the item (0: none), speed, stop distance, range, the look speed, and its state.
#[derive(Component, Clone, Copy, Debug)]
pub struct Tempt {
    pub item: u32,
    pub speed: f64,
    pub stop: f64,
    pub range: f64,
    pub look_y: f32,
    pub calm: u64,
    pub calm_until: u64,
    pub player: u32,
    pub aim: (i32, i32),
}
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
/// The head and the body (`yBodyRot`); NaN until the head is first moved.
#[derive(Component, Clone, Copy, Debug)]
pub struct Head {
    pub yaw: f32,
    pub pitch: f32,
    pub body: f32,
}
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Fluid {
    pub depth: f64,
    pub lava: bool,
}
/// `Mob.getFluidJumpThreshold`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Buoyancy(pub f64);
/// The jump the goals ask for this tick (the physics consumes it).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Jump(pub bool);

// ---- run markers: a goal that runs is a marker its system visits ------------------------------------------
macro_rules! markers { ($($n:ident),*) => { $(#[derive(Component, Clone, Copy, Debug)] #[component(storage = "SparseSet")] pub struct $n;)* } }
markers!(Floating, Panicking, Tempting, Looking);
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

/// The data of a cow's goals (vanilla values of `Cow.registerGoals`, `Animal` and `Mob`).
pub fn cow_goals() -> impl Bundle {
    (
        GoalSet::new(&cow_slots()),
        Panic { speed: 2.0, h: 5, v: 4, window: 40, until: 0 },
        Tempt { item: WHEAT, speed: 1.25, stop: 2.5, range: 10.0, look_y: 95.0, calm: 100, calm_until: 0, player: 0, aim: (i32::MIN, 0) },
        BreedData { speed: 1.0, range: 8.0, delay: 60, love: 600, rest: 6000, baby_age: 24000 },
        FollowParent { speed: 1.25, recalc: 0 },
        Glance { range: 6.0, period: 50 },
        Scan { period: 50 },
        GoalTimers::default(),
        Look { target: [0.0; 3], cooldown: 0, y_default: 10.0, y_speed: 10.0, x_max: 40.0, max_head_y: 75.0, eye: 1.3 },
        Head { yaw: f32::NAN, pitch: 0.0, body: 0.0 },
        Fluid::default(),
        Buoyancy(0.4),
        Jump(false),
    )
}
pub const WHEAT: u32 = 1;

// ---- resources: the events, as data ------------------------------------------------------------------------
#[derive(Resource, Default)]
pub struct Fired(pub Vec<(Entity, Kind)>);
#[derive(Resource, Default)]
pub struct Entered(pub Vec<Entity>);
#[derive(Resource, Default)]
pub struct Hurts(pub Vec<Entity>);
pub enum Req {
    Arm(Entity),
    Start(Entity, Kind),
    End(Entity, Kind),
}
#[derive(Resource, Default)]
pub struct Requests(pub Vec<Req>);

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
    world.insert_resource(Online::default());
}
pub fn register(s: &mut Schedule) {
    s.add_systems(online_events.after(due).before(player_near));
    s.add_systems(
        (entered_events, hurt_events, mother_moved, tempt_run, breed_run, follow_parent_run, glance_run, scan_run, float_run, arbitrate, look_control)
            .chain()
            .after(online_events),
    );
}

// ---- events -------------------------------------------------------------------------------------------------
/// Damage that makes the cow panic (`DamageType.causes_panic`): call from the damage code.
pub fn hurt(world: &mut World, e: Entity) {
    world.resource_mut::<Hurts>().0.push(e);
}
/// A fluid appeared or vanished in the cow's box: `depth` blocks of water at its eyes' reach, or lava.
pub fn set_fluid(world: &mut World, e: Entity, depth: f64, lava: bool) {
    *world.get_mut::<Fluid>(e).unwrap() = Fluid { depth, lava };
    let k = Kind::Float;
    let req = if floats(world, e) { Req::Start(e, k) } else { Req::End(e, k) };
    world.resource_mut::<Requests>().0.push(req);
}
fn floats(w: &World, e: Entity) -> bool {
    let (f, b) = (w.get::<Fluid>(e).unwrap(), w.get::<Buoyancy>(e).unwrap());
    f.depth > b.0 || f.lava
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
    let b = *world.get::<BreedData>(e).unwrap();
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
/// Makes `parent` the mother of `baby`: its moves wake the baby's follow-parent goal.
pub fn adopt(world: &mut World, parent: Entity, baby: Entity) {
    match world.get_mut::<Mother>(parent) {
        Some(mut m) => m.0.push(baby),
        None => {
            world.entity_mut(parent).insert(Mother(vec![baby]));
        }
    }
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
    world.resource_mut::<Wheel>().push((now + ticks).min(now + WHEEL_MAX), e, Due::Grow);
}

fn with_rng<T>(w: &mut World, e: Entity, f: impl FnOnce(&mut Rng) -> T) -> T {
    f(&mut w.get_mut::<Rng>(e).unwrap())
}
fn arm(w: &mut World, e: Entity, k: Kind, wait: i32) {
    let now = w.resource::<Clock>().0;
    w.resource_mut::<Wheel>().push((now + wait.max(1) as u64).min(now + WHEEL_MAX), e, Due::Goal(k));
}
/// Arms the look timers of a cow that has none (1 chance in `period` per ask, one ask every 2 ticks).
fn arm_timers(w: &mut World, e: Entity) {
    let Some(mut t) = w.get::<GoalTimers>(e).copied() else { return };
    let (g, s) = (*w.get::<Glance>(e).unwrap(), *w.get::<Scan>(e).unwrap());
    if !t.look {
        let wait = with_rng(w, e, |r| 2 * geometric(r, g.period));
        arm(w, e, Kind::LookAtPlayer, wait);
        t.look = true;
    }
    if !t.random {
        let wait = with_rng(w, e, |r| 2 * geometric(r, s.period));
        arm(w, e, Kind::RandomLook, wait);
        t.random = true;
    }
    *w.get_mut::<GoalTimers>(e).unwrap() = t;
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
fn hurt_events(clock: Res<Clock>, mut h: ResMut<Hurts>, mut q: Query<&mut Panic>, mut reqs: ResMut<Requests>) {
    for e in std::mem::take(&mut h.0) {
        if let Ok(mut p) = q.get_mut(e) {
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
    cows: Query<(&Pos, &Tempt)>,
    mut reqs: ResMut<Requests>,
) {
    if on.events.is_empty() {
        return;
    }
    let events = std::mem::take(&mut on.events);
    pl.list = on.list.iter().flatten().map(|p| (p.x, p.z)).collect();
    pl.dirty = true;
    let d2 = |p: &Player, c: Vec3| (p.x - c.x).powi(2) + (p.y - c.y).powi(2) + (p.z - c.z).powi(2);
    for (old, new) in events {
        let Some(n) = new else { continue };
        idx.for_radius(n.x, n.z, SCAN, |c| {
            let Ok((pos, t)) = cows.get(c) else { return };
            let reach = t.range.max(2.0).powi(2);
            let was = old.is_some_and(|o| o.holds(t.item) && d2(&o, pos.0) <= reach);
            if clock.0 >= t.calm_until && n.holds(t.item) && d2(&n, pos.0) <= reach && !was {
                reqs.0.push(Req::Start(c, Kind::Tempt));
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
/// The path a goal asked for ended (or none was found): the goal that owns it ends (called by `cow::path_ended`).
pub fn path_ended(e: Entity, g: &GoalSet, reqs: &mut Requests) {
    if let Some(k @ (Kind::Stroll | Kind::Panic)) = g.mover() {
        reqs.0.push(Req::End(e, k));
    }
}

fn go_to(cmd: &mut Commands, e: Entity, x: f64, z: f64, speed: f64) {
    cmd.queue(move |w: &mut World| {
        if let Some(mut s) = w.get_mut::<Stroll>(e) {
            s.tx = x;
            s.tz = z;
        }
        if let Some(mut n) = w.get_mut::<Nav>(e) {
            n.speed = speed;
        }
        w.entity_mut(e).insert(NeedsPath);
    });
}
fn stop_nav(w: &mut World, e: Entity) {
    if let Some(mut n) = w.get_mut::<Nav>(e) {
        n.path = None;
    }
    if let Some(mut d) = w.get_mut::<Drive>(e) {
        d.zza = 0.0;
    }
    if let Some(mut m) = w.get_mut::<MoveTo>(e) {
        m.active = false;
    }
    w.entity_mut(e).remove::<(Following, NeedsPath)>();
}
fn aim(cmd: &mut Commands, e: Entity, look: &mut Look, looking: bool, at: [f64; 3], y_speed: Option<f32>) {
    look.target = at;
    look.cooldown = 2;
    look.y_speed = y_speed.unwrap_or(look.y_default);
    if !looking {
        cmd.entity(e).insert(Looking);
    }
}
fn d2(a: Vec3, b: Vec3) -> f64 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)
}

// ---- running goals: the only things that tick -----------------------------------------------------------------
type Aim<'a> = (Entity, &'a Pos, &'a mut Look, Has<Looking>, Has<Following>, Has<NeedsPath>);
/// `TemptGoal.canContinue` and `tick`: look at the player, stop near it, walk to it when its block changed.
fn tempt_run(
    clock: Res<Clock>,
    on: Res<Online>,
    mut q: Query<(Aim, &mut Tempt), With<Tempting>>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    for ((e, pos, mut look, looking, following, asked), mut t) in &mut q {
        let max_d = t.range.max(2.0).powi(2);
        let p = on.get(t.player).filter(|p| p.holds(t.item) && d2(Vec3::new(p.x, p.y, p.z), pos.0) <= max_d);
        let Some(p) = p else {
            reqs.0.push(Req::End(e, Kind::Tempt));
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
    mut q: Query<(Aim, &mut Breeding, &BreedData, Option<&Love>)>,
    partners: Query<(&Pos, Option<&Love>, Has<Panicking>)>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    let now = clock.0;
    for ((e, pos, mut look, looking, following, asked), mut b, data, love) in &mut q {
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
        let elapsed = now - b.since;
        if elapsed >= data.delay && d < 9.0 {
            let partner = b.partner;
            cmd.queue(move |w: &mut World| birth(w, e, partner));
        } else if elapsed >= 2 * data.delay {
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
    let baby = w.spawn(cow_bundle(pos.x, pos.z, seed)).id();
    w.get_mut::<ActiveUntil>(baby).unwrap().0 = active;
    make_baby(w, baby, data.baby_age);
    adopt(w, a, baby);
    if active > now {
        arm_stroll_now(w, baby);
        arm_timers(w, baby);
    }
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
/// `LookControl.tick` and `BodyRotationControl`: the head turns to the target (or back to the body), a head that is
/// turned too far drags the body, a walking body is the entity's yaw. Leaves when head and body agree.
#[allow(clippy::type_complexity)]
fn look_control(mut q: Query<(Entity, &Pos, &Yaw, &mut Look, &mut Head, Has<Following>), With<Looking>>, mut cmd: Commands) {
    for (e, pos, yaw, mut look, mut h, walking) in &mut q {
        if h.yaw.is_nan() {
            *h = Head { yaw: yaw.0, pitch: 0.0, body: yaw.0 };
        }
        if look.cooldown > 0 {
            look.cooldown -= 1;
            let (dx, dy, dz) = (look.target[0] - pos.0.x, look.target[1] - (pos.0.y + look.eye), look.target[2] - pos.0.z);
            let ty = trig::atan2_degrees(dz, dx) - 90.0;
            let tp = -trig::atan2_degrees(dy, (dx * dx + dz * dz).sqrt());
            h.yaw = rotate_towards(h.yaw, ty, look.y_speed);
            h.pitch = rotate_towards(h.pitch, tp, look.x_max);
        } else {
            h.yaw = rotate_towards(h.yaw, h.body, 10.0);
            h.pitch = rotate_towards(h.pitch, 0.0, 10.0);
        }
        if walking {
            h.body = yaw.0;
            h.yaw = rotate_if_necessary(h.yaw, h.body, look.max_head_y);
        } else {
            h.body = rotate_if_necessary(h.body, h.yaw, look.max_head_y);
        }
        if look.cooldown == 0 && wrap(h.yaw - h.body).abs() < 0.01 && h.pitch.abs() < 0.01 {
            cmd.entity(e).remove::<Looking>();
        }
    }
}

// ---- the selector: start and stop, with the locks -------------------------------------------------------------
enum Pick {
    Yes,
    Spot(f64, f64),
    Partner(Entity),
    Player(u32),
    Parent(Entity),
}
fn arbitrate(world: &mut World) {
    let fired = std::mem::take(&mut world.resource_mut::<Fired>().0);
    let reqs = std::mem::take(&mut world.resource_mut::<Requests>().0);
    for (e, k) in fired {
        timer_fired(world, e, k);
    }
    for r in reqs {
        match r {
            // A cow entering the active zone: its look timers are armed.
            Req::Arm(e) => arm_timers(world, e),
            Req::Start(e, k) => start_goal(world, e, k),
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
            if now >= active {
                return;
            }
        }
        Kind::Breed if w.get::<Love>(e).is_none_or(|l| l.0 <= now) => return,
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
}
fn rearm_timer(w: &mut World, e: Entity, k: Kind) {
    let g = *w.get::<Glance>(e).unwrap();
    let s = *w.get::<Scan>(e).unwrap();
    let mut t = *w.get::<GoalTimers>(e).unwrap();
    match k {
        Kind::LookAtPlayer => t.look = false,
        _ => t.random = false,
    }
    let _ = (g, s);
    *w.get_mut::<GoalTimers>(e).unwrap() = t;
    arm_timers(w, e);
}
fn end_goal(w: &mut World, e: Entity, k: Kind) {
    let Some(mut g) = w.get_mut::<GoalSet>(e) else { return };
    if !g.stop(k) {
        return;
    }
    stop_effects(w, e, k);
    // A goal that was waiting for the flags it held tries again, in registration order.
    let g = w.get::<GoalSet>(e).unwrap();
    let waiting: Vec<Kind> = g.slots.iter().enumerate().filter(|(i, _)| g.pending >> i & 1 == 1).map(|s| s.1.0).collect();
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
            if now > p.until {
                return None;
            }
            let lim = w.resource::<Level>().half - 2;
            // DefaultRandomPos: 10 tries; on the slab only the standing level is a stable destination.
            let spot = with_rng(w, e, |r| {
                (0..10).find_map(|_| {
                    let (dx, dy, dz) = (r.next_int(2 * p.h + 1) - p.h, r.next_int(2 * p.v + 1) - p.v, r.next_int(2 * p.h + 1) - p.h);
                    let (x, z) = ((pos.x + f64::from(dx)).floor() as i32, (pos.z + f64::from(dz)).floor() as i32);
                    (dy == 0 && x.abs() < lim && z.abs() < lim).then(|| (f64::from(x) + 0.5, f64::from(z) + 0.5))
                })
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
                let lovers = w.get::<Love>(c).is_some_and(|l| l.0 > now) && w.get::<GrowAt>(c).is_none() && w.get::<Panicking>(c).is_none();
                if c != e && lovers && d <= b.range * b.range && best.is_none_or(|x| d < x.1) {
                    best = Some((c, d));
                }
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
        Kind::Tempt => {
            let t = *w.get::<Tempt>(e)?;
            if now < t.calm_until {
                return None;
            }
            let mut best: Option<(u32, f64)> = None;
            let reach = t.range.max(2.0);
            w.resource::<Online>().for_near(pos.x, pos.z, reach, |p| {
                let d = d2(pos, Vec3::new(p.x, p.y, p.z));
                if p.holds(t.item) && d <= reach * reach && best.is_none_or(|b| d < b.1) {
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
                if c != e && w.get::<GrowAt>(c).is_none() && dx <= 8.0 && dy <= 4.0 && dz <= 8.0 {
                    let d = d2(pos, cp.0);
                    if best.is_none_or(|b| d <= b.1) {
                        best = Some((c, d));
                    }
                }
            }
            best.filter(|b| b.1 >= 9.0).map(|b| Pick::Parent(b.0))
        }
        Kind::Stroll => {
            let level = *w.resource::<Level>();
            let (mut s, mut r) = (*w.get::<Stroll>(e)?, Rng(w.get::<Rng>(e)?.0));
            let ok = pick_spot(&level, &Pos(pos), &mut s, &mut r);
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
    }
}
fn start_effects(w: &mut World, e: Entity, k: Kind, pick: Pick) {
    let now = w.resource::<Clock>().0;
    match (k, pick) {
        (Kind::Float, _) => {
            w.entity_mut(e).insert(Floating);
        }
        (Kind::Panic, Pick::Spot(x, z)) => {
            let speed = w.get::<Panic>(e).unwrap().speed;
            w.entity_mut(e).insert(Panicking);
            w.get_mut::<Nav>(e).unwrap().speed = speed;
            let mut s = w.get_mut::<Stroll>(e).unwrap();
            (s.tx, s.tz) = (x, z);
            w.entity_mut(e).insert(NeedsPath);
        }
        (Kind::Breed, Pick::Partner(c)) => {
            w.entity_mut(e).insert(Breeding { partner: c, since: now, aim: (i32::MIN, 0) });
            w.resource_mut::<Requests>().0.push(Req::Start(c, Kind::Breed));
        }
        (Kind::Tempt, Pick::Player(id)) => {
            let mut t = w.get_mut::<Tempt>(e).unwrap();
            t.player = id;
            t.aim = (i32::MIN, 0);
            w.entity_mut(e).insert(Tempting);
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
        }
        Kind::Breed => {
            w.entity_mut(e).remove::<Breeding>();
        }
        Kind::Tempt => {
            w.entity_mut(e).remove::<Tempting>();
            stop_nav(w, e);
            // `calmDown = reducedTickDelay(100)`: no new temptation for 100 ticks, then it is looked at again.
            let calm = w.get::<Tempt>(e).unwrap().calm;
            w.get_mut::<Tempt>(e).unwrap().calm_until = now + calm;
            arm(w, e, Kind::Tempt, calm as i32);
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
