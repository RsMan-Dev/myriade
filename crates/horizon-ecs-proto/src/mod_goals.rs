//! Goals and behaviours a native mod adds to an entity, run by the ECS. A goal is one entry of the entity's `GoalSet`, whose module
//! is `FAMILY[flags | interruptable]`: 32 static modules, one per flag set, so a species without mod goals pays nothing. The entry's
//! parameter is the goal's rank in the entity's `ModGoals`. A behaviour is an entry of the entity's `ModBehaviors`, merged into the
//! brain's order by the brain (`brain::step`). The mod's own calls go through `ModGoalHost`, which the game installs in `ModGoalCalls`.
use crate::behaviours::{self, Entry, Inst, Module};
use crate::brain::{add_extra, clear_extra, ExtraFns, MemKey, Need};
use crate::bridge::ids;
use crate::cow::{Clock, Nav, Rng};
use crate::goal_edit;
use crate::goals::{self, GoalSet, Jump, Look, Looking, MAX_ENTRIES};
use crate::targeting::{self, Target, Who};
use bevy_ecs::{prelude::*, system::Commands, world::CommandQueue};
use horizon_core::random::LegacyRandom;
use std::any::Any;

/// A mod goal or behaviour of an entity: the mod and part that registered it, whether it runs every tick, and the goal, lent out while it runs.
pub struct ModEntry {
    pub mod_id: &'static str,
    pub part: &'static str,
    pub every_tick: bool,
    pub goal: Box<dyn Any + Send + Sync>,
}
/// The mod goals of an entity by rank; a slot is None while its goal is lent out for a call. Absent on an entity without one.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct ModGoals(pub Vec<Option<ModEntry>>);

/// A mod behaviour of an entity, at the same index as its `brain::Extra` entry: the mod and part that registered it, and the behaviour,
/// lent out while it runs. Its activity, priority, duration and running state are the brain's (`brain::add_extra`).
pub struct ModBehaviour {
    pub mod_id: &'static str,
    pub part: &'static str,
    pub entry: Option<ModEntry>,
}
/// The mod behaviours of an entity. Absent on an entity without one, so the brain's step pays nothing for it.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct ModBehaviors(pub Vec<ModBehaviour>);

/// The random source of an entity's mod goals and behaviours (`Mob.random`): seeded from the entity's own generator (`cow::Rng`,
/// read without advancing it) when the first goal or behaviour is added, so the draws do not follow the network ids.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct GoalRandom(pub LegacyRandom);

/// Gives `e` its `GoalRandom` if it has none yet.
fn seed_random(w: &mut World, e: Entity) {
    if w.get::<GoalRandom>(e).is_some() {
        return;
    }
    let Some(mut own) = w.get::<Rng>(e).copied() else { return };
    let seed = (u64::from(own.next_u32()) << 32 | u64::from(own.next_u32())) as i64;
    w.entity_mut(e).insert(GoalRandom(LegacyRandom::new(seed)));
}

/// The game's side of a mod's calls: it builds the context on the world and runs the mod under its panic guard.
pub trait ModGoalHost {
    fn can_use(&self, w: &mut World, e: Entity, goal: &mut ModEntry) -> bool;
    fn start(&self, w: &mut World, e: Entity, goal: &mut ModEntry);
    fn tick(&self, w: &mut World, e: Entity, goal: &mut ModEntry);
    fn stop(&self, w: &mut World, e: Entity, goal: &mut ModEntry);
    /// `Behavior.checkExtraStartConditions`.
    fn behaviour_check(&self, w: &mut World, e: Entity, behaviour: &mut ModEntry) -> bool;
    /// `Behavior.start`.
    fn behaviour_start(&self, w: &mut World, e: Entity, behaviour: &mut ModEntry);
    /// `Behavior.canStillUse`.
    fn behaviour_can_still_use(&self, w: &mut World, e: Entity, behaviour: &mut ModEntry) -> bool;
    /// `Behavior.tick`.
    fn behaviour_tick(&self, w: &mut World, e: Entity, behaviour: &mut ModEntry);
    /// `Behavior.stop`.
    fn behaviour_stop(&self, w: &mut World, e: Entity, behaviour: &mut ModEntry);
}
/// The host lent for one step, its lifetime erased: `EcsWorld::step_with` takes it out of the world before it returns.
#[derive(Clone, Copy)]
pub struct LentHost(&'static dyn ModGoalHost, std::thread::ThreadId);
// SAFETY: it is only read by the brain and goal steps, which take the whole world (`&mut World`) and so run on the thread
// that runs the schedule, the one that lent it (`get` checks it in debug builds).
unsafe impl Send for LentHost {}
unsafe impl Sync for LentHost {}
impl LentHost {
    pub(crate) fn lend(host: &dyn ModGoalHost) -> LentHost {
        // SAFETY: the only holder is `ModGoalCalls`, emptied by `step_with` before `host`'s borrow ends, panics included.
        LentHost(unsafe { std::mem::transmute::<&dyn ModGoalHost, &'static dyn ModGoalHost>(host) }, std::thread::current().id())
    }
    fn get(self) -> &'static dyn ModGoalHost {
        debug_assert_eq!(self.1, std::thread::current().id());
        self.0
    }
}
/// The host of the current step: set before the schedule runs and emptied after it.
#[derive(Resource, Default)]
pub struct ModGoalCalls(pub Option<LentHost>);

/// A goal a mod registered for an entity: its priority (lower runs first), its flags (`TARGET` for a target goal), whether it is
/// interruptable and whether it runs every tick.
pub struct NewGoal {
    pub priority: u8,
    pub flags: u8,
    pub interruptable: bool,
    pub every_tick: bool,
    pub mod_id: &'static str,
    pub part: &'static str,
    pub goal: Box<dyn Any + Send + Sync>,
}
/// A behaviour a mod registered for an entity (`BehaviorRegistrar::add`), with the window and entry memories it was given.
pub struct NewBehaviour {
    pub activity: &'static str,
    pub priority: i32,
    pub duration: Option<(i32, i32)>,
    pub needs: Vec<(&'static MemKey, Need)>,
    pub mod_id: &'static str,
    pub part: &'static str,
    pub behaviour: Box<dyn Any + Send + Sync>,
}

static SLOTS: [u8; MAX_ENTRIES] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31];
macro_rules! family {
    ($($k:literal)*) => { [$(module::<$k>()),*] };
}
/// The modules of mod goals: `flags` (bits 0..3) and `interruptable` (bit 4) of the goals they hold.
pub static FAMILY: [Module; 32] = family!(0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31);

const fn module<const K: u8>() -> Module {
    Module {
        name: "mod_goal",
        flags: K & 15,
        interruptable: K >> 4 == 1,
        can_use: can_use_goal,
        start: start_goal,
        stop: stop_goal,
        arm: arm_goal,
        tick: Some(tick::<K> as fn(&mut World)),
        ..Module::DEFAULT
    }
}

/// The rank of the goal of `slot` in its entity's `ModGoals`.
fn rank(w: &World, e: Entity, slot: u8) -> Option<usize> {
    let g = w.get::<GoalSet>(e)?;
    Some(*behaviours::params::<u8>(g.entries()[slot as usize].params) as usize)
}
/// Runs `f` on the goal of `slot` with the host.
fn call<R>(w: &mut World, e: Entity, slot: u8, f: impl FnOnce(&dyn ModGoalHost, &mut World, Entity, &mut ModEntry) -> R) -> Option<R> {
    let i = rank(w, e, slot)?;
    let host = w.get_resource::<ModGoalCalls>()?.0?;
    let lent = w.get_mut::<ModGoals>(e).and_then(|mut m| m.0.get_mut(i).and_then(Option::take));
    let mut entry = lent?;
    let out = f(host.get(), w, e, &mut entry);
    if let Some(mut m) = w.get_mut::<ModGoals>(e)
        && let Some(slot) = m.0.get_mut(i)
    {
        *slot = Some(entry);
    }
    Some(out)
}
/// Runs `f` on the entry of the behaviour of rank `i` in `ModBehaviors`, lent out for the call (a call made from inside it finds none).
fn call_behaviour<R>(w: &mut World, e: Entity, i: usize, f: impl FnOnce(&dyn ModGoalHost, &mut World, Entity, &mut ModEntry) -> R) -> Option<R> {
    let host = w.get_resource::<ModGoalCalls>()?.0?;
    let lent = w.get_mut::<ModBehaviors>(e).and_then(|mut b| b.0.get_mut(i).and_then(|x| x.entry.take()));
    let mut entry = lent?;
    let out = f(host.get(), w, e, &mut entry);
    if let Some(mut b) = w.get_mut::<ModBehaviors>(e)
        && let Some(x) = b.0.get_mut(i)
    {
        x.entry = Some(entry);
    }
    Some(out)
}
/// Asks the selector again in two ticks (the cadence of a goal that is not wanted now); a dormant entity is asked when it wakes.
fn rearm(w: &mut World, e: Entity, slot: u8) {
    if w.get::<crate::zone::Dormant>(e).is_none() {
        goals::arm(w, e, slot, 2);
    }
}
fn arm_goal(w: &mut World, e: Entity, inst: Inst) {
    goals::arm(w, e, inst.slot, 1);
}
fn can_use_goal(w: &mut World, e: Entity, inst: Inst) -> bool {
    let ok = call(w, e, inst.slot, |h, w, e, g| h.can_use(w, e, g)).unwrap_or(false);
    if !ok {
        rearm(w, e, inst.slot);
    }
    ok
}
fn start_goal(w: &mut World, e: Entity, inst: Inst) {
    call(w, e, inst.slot, |h, w, e, g| h.start(w, e, g));
}
fn stop_goal(w: &mut World, e: Entity, inst: Inst) {
    call(w, e, inst.slot, |h, w, e, g| h.stop(w, e, g));
    rearm(w, e, inst.slot);
}
/// Whether the selector runs its full pass on this tick for `e` (`Mob.aiStep`: the id-based half of the ticks; the first two always).
fn full_pass(w: &World, e: Entity) -> bool {
    let age = w.get_resource::<Clock>().map_or(0, |c| c.of(e) as i64);
    let id_based = age + ids::id_of(w, e).map_or(0, i64::from);
    !(id_based % 2 != 0 && age > 1)
}
/// The running mod goals of the family's module `K` tick on the entities that carry a `ModGoals`. On the off half of the ticks only
/// those that run every tick do (`GoalSelector.tickRunningGoals(false)`).
fn tick<const K: u8>(w: &mut World) {
    let own = &FAMILY[K as usize];
    for e in goals::entities::<With<ModGoals>>(w) {
        let full = full_pass(w, e);
        let running: Vec<u8> = match w.get::<GoalSet>(e) {
            Some(g) => (0..g.len() as u8).filter(|&s| g.slot_running(s) && std::ptr::eq(g.module(s), own)).collect(),
            None => continue,
        };
        for s in running {
            let every = rank(w, e, s).and_then(|i| w.get::<ModGoals>(e)?.0.get(i)?.as_ref().map(|g| g.every_tick)).unwrap_or(false);
            if full || every {
                call(w, e, s, |h, w, e, g| h.tick(w, e, g));
            }
        }
    }
}

/// Adds a mod goal to `e`'s selector. False for an entity with no selector, a full one, or when the goal's module needs a component
/// the entity lacks.
pub fn add_goal(w: &mut World, e: Entity, goal: NewGoal) -> bool {
    let i = w.get::<ModGoals>(e).map_or(0, |m| m.0.len());
    if i >= SLOTS.len() {
        return false;
    }
    let module = &FAMILY[((goal.flags & 15) | (u8::from(goal.interruptable) << 4)) as usize];
    if !goal_edit::add(w, e, Entry::new(module, goal.priority, &SLOTS[i])) {
        return false;
    }
    seed_random(w, e);
    let entry = Some(ModEntry { mod_id: goal.mod_id, part: goal.part, every_tick: goal.every_tick, goal: goal.goal });
    match w.get_mut::<ModGoals>(e) {
        Some(mut m) => m.0.push(entry),
        None => {
            w.entity_mut(e).insert(ModGoals(vec![entry]));
        }
    }
    true
}
/// Adds a mod behaviour to `e` as a brain extra (`brain::add_extra`), at the index it gets there. False when `e` has no brain or its
/// activity is not in the plan.
pub fn add_behaviour(w: &mut World, e: Entity, b: NewBehaviour) -> bool {
    let fns = ExtraFns { can_start: behaviour_check, start: behaviour_start, tick: behaviour_step, stop: behaviour_stop };
    let priority = u8::try_from(b.priority.clamp(0, 255)).unwrap_or(0);
    let Some(k) = add_extra(w, e, b.activity, priority, &b.needs, b.duration, fns) else { return false };
    seed_random(w, e);
    let one = ModBehaviour { mod_id: b.mod_id, part: b.part, entry: Some(ModEntry { mod_id: b.mod_id, part: b.part, every_tick: false, goal: b.behaviour }) };
    match w.get_mut::<ModBehaviors>(e) {
        Some(mut list) => list.0.push(one),
        None => {
            w.entity_mut(e).insert(ModBehaviors(vec![one]));
        }
    }
    debug_assert_eq!(w.get::<ModBehaviors>(e).map(|l| l.0.len()), Some(k as usize + 1));
    true
}
/// Removes the mod behaviours of `e` from its brain (detach, reload).
pub fn clear_behaviours(w: &mut World, e: Entity) {
    clear_extra(w, e);
    w.entity_mut(e).remove::<ModBehaviors>();
}
/// `Behavior.checkExtraStartConditions` of the behaviour at index `i`; false when it is absent.
fn behaviour_check(w: &mut World, e: Entity, i: u16) -> bool {
    call_behaviour(w, e, i as usize, |h, w, e, b| h.behaviour_check(w, e, b)).unwrap_or(false)
}
/// `Behavior.start` of the behaviour at index `i`.
fn behaviour_start(w: &mut World, e: Entity, i: u16) {
    call_behaviour(w, e, i as usize, |h, w, e, b| h.behaviour_start(w, e, b));
}
/// One step of the behaviour at index `i` (`Behavior.canStillUse`, then `Behavior.tick`); false ends it, as `canStillUse` does.
fn behaviour_step(w: &mut World, e: Entity, i: u16) -> bool {
    let go_on = call_behaviour(w, e, i as usize, |h, w, e, b| h.behaviour_can_still_use(w, e, b)).unwrap_or(false);
    if go_on {
        call_behaviour(w, e, i as usize, |h, w, e, b| h.behaviour_tick(w, e, b));
    }
    go_on
}
/// `Behavior.stop` of the behaviour at index `i`.
fn behaviour_stop(w: &mut World, e: Entity, i: u16) {
    call_behaviour(w, e, i as usize, |h, w, e, b| h.behaviour_stop(w, e, b));
}

/// `GoalContext::move_to`: a walk to `at` at `speed`; false when the entity cannot walk.
pub fn move_to(w: &mut World, e: Entity, at: [f64; 3], speed: f64) -> bool {
    let Some(reach) = w.get::<Nav>(e).map(|n| n.reach) else { return false };
    goals::go_to_at(w, e, at, speed, reach);
    true
}
/// `GoalContext::move_to_path`: follows the nodes as given; false outside a step, without navigation or with no node left.
pub fn move_to_path(w: &mut World, e: Entity, nodes: &[[f64; 3]], speed: f64) -> bool {
    let Some(mw) = w.get_resource::<crate::cow::Level>().map(|l| l.mw.0) else { return false };
    crate::nav_supplied::follow(w, mw, e, nodes, speed)
}
/// `GoalContext::stop_navigation`.
pub fn stop_navigation(w: &mut World, e: Entity) {
    goals::stop_nav(w, e);
}
/// `GoalContext::look_at`: the head turns toward `at`.
pub fn look_at(w: &mut World, e: Entity, at: [f64; 3]) {
    let Some(mut look) = w.get::<Look>(e).copied() else { return };
    let looking = w.get::<Looking>(e).is_some();
    let mut queue = CommandQueue::default();
    goals::aim_clamped(&mut Commands::new(&mut queue, w), e, &mut look, looking, at, None, None);
    if let Some(mut l) = w.get_mut::<Look>(e) {
        *l = look;
    }
    queue.apply(w);
}
/// `GoalContext::set_target` on an entity whose target is `who` (None clears it). The target-change hook is emitted by `targeting::set_target`.
pub fn set_target(w: &mut World, e: Entity, who: Option<Who>) -> bool {
    match who {
        // Memory 0, as the other goals set their targets (`reinforcements`): the mod gives no memory to set.
        Some(who) => targeting::set_target(w, e, Target::new(who, 0, u8::MAX)),
        None => targeting::clear_target(w, e),
    }
    true
}
/// `GoalContext::jump`: the jump is asked for the entity's next tick.
pub fn jump(w: &mut World, e: Entity) {
    if let Some(mut j) = w.get_mut::<Jump>(e) {
        j.0 = true;
    }
}
/// The entity of a network id, for a goal's target.
pub fn entity_of_id(w: &World, id: u32) -> Option<Entity> {
    w.get_resource::<ids::NetIds>()?.entity(id)
}
/// The network id of `e`, for a goal's own handle.
pub fn id_of(w: &World, e: Entity) -> Option<u32> {
    ids::id_of(w, e)
}
/// The living target a network id names: an entity of the level, or the online player with that id (a player's id is its `Who::Player`).
pub fn living_who(w: &World, id: u32) -> Option<Who> {
    if let Some(e) = entity_of_id(w, id) {
        return Some(Who::Entity(e));
    }
    w.get_resource::<goals::Online>()?.get(id).map(|_| Who::Player(id))
}
/// The `WALK_TARGET` memory as a behaviour reads it: its position, speed and close range.
pub fn walk_target(w: &World, e: Entity) -> Option<([f64; 3], f64, i32)> {
    let t = brain::walk_target(w, e)?;
    Some((t.target.position(w)?, f64::from(t.speed), t.close))
}
/// The `LOOK_TARGET` memory's position.
pub fn look_target(w: &World, e: Entity) -> Option<[f64; 3]> {
    brain::look_target(w, e)?.position(w)
}
/// The `ATTACK_TARGET` memory: the target it holds.
pub fn attack_target(w: &World, e: Entity) -> Option<Who> {
    match brain::get(w, e, &brain::ATTACK_TARGET) {
        Some(brain::MemValue::Who(who)) => Some(*who),
        _ => None,
    }
}
/// `Brain.setMemory` of `ATTACK_TARGET`: posed for `who` or erased for `None`. False when the brain does not keep the memory.
pub fn set_brain_target(w: &mut World, e: Entity, who: Option<Who>) -> bool {
    let Some(who) = who else {
        brain::erase(w, e, &brain::ATTACK_TARGET);
        return w.get::<brain::Brain>(e).is_some();
    };
    brain::set(w, e, &brain::ATTACK_TARGET, brain::MemValue::Who(who), None);
    matches!(brain::get(w, e, &brain::ATTACK_TARGET), Some(brain::MemValue::Who(_)))
}
/// `Brain.setMemory` of `WALK_TARGET` for the block `at`, at `speed`, close within `close` blocks.
pub fn set_walk_target(w: &mut World, e: Entity, at: [i32; 3], speed: f64, close: i32) {
    let walk = brain::WalkTarget { target: brain::Tracker::Block(at), speed: speed as f32, close };
    brain::set(w, e, &brain::WALK_TARGET, brain::MemValue::Walk(walk), None);
}
