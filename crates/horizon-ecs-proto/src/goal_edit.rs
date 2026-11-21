//! Goals a mod adds, removes or resets on one entity (`GoalSelector.addGoal`, `removeAllGoals`, `registerGoals` again).
//! The selector stays what it is: a `GoalSet` over a list of `Entry`. An entity a mod never touched keeps the list of its
//! species and pays nothing. The first edit gives the entity a sparse `GoalEdit` (the species list, for the reset) and
//! swaps its `GoalSet` for one over an interned list: a goal is added in a new slot at the end and removed by turning its
//! slot into a `REMOVED` entry, so no slot a module's state or a wheel deadline refers to ever moves.
use crate::behaviours::{Cond, Entry, Inst, Module};
use crate::goals::*;
use bevy_ecs::prelude::*;
use std::{any::Any, collections::HashMap, sync::Mutex};

/// On an entity whose goal list a mod changed: the list its species gave it.
#[derive(Component, Clone, Copy)]
#[component(storage = "SparseSet")]
pub struct GoalEdit {
    base: &'static [Entry],
}

fn never(_: &mut World, _: Entity, _: Inst) -> bool {
    false
}
/// The entry of a removed goal: it holds its slot, takes no flag and never starts.
pub static REMOVED: Module = Module { name: "removed", can_use: never, fired: never, ..Module::DEFAULT };
static NOTHING: () = ();

type Key = Vec<(usize, u8, usize, usize)>;
/// One allocation per distinct list: entries are compared by module, priority, parameters and condition (the parameters
/// themselves are interned by the caller, so equal ones are the same reference).
fn intern(list: Vec<Entry>) -> &'static [Entry] {
    static LISTS: Mutex<Option<HashMap<Key, &'static [Entry]>>> = Mutex::new(None);
    let key: Key = list
        .iter()
        .map(|e| (e.module as *const Module as usize, e.priority, e.params as *const (dyn Any + Sync) as *const () as usize, e.when.map_or(0, |c| c as *const Cond as usize)))
        .collect();
    let mut lists = LISTS.lock().unwrap();
    let interned: &'static [Entry] = *lists.get_or_insert_with(Default::default).entry(key).or_insert_with(|| Box::leak(list.into_boxed_slice()));
    interned
}

fn removed(m: &'static Module) -> bool {
    std::ptr::eq(m, &REMOVED)
}
/// Whether `m` is a target goal (the target selector's) and not a goal.
pub fn is_target(m: &Module) -> bool {
    m.flags & TARGET != 0
}
/// The capabilities `m` supposes that the entity shows as components (the rest is not recorded per entity).
fn supported(w: &World, e: Entity, m: &'static Module) -> bool {
    use crate::behaviours::Cap;
    m.requires.iter().all(|c| match c {
        Cap::Head => w.get::<Head>(e).is_some(),
        _ => true,
    })
}

/// Puts `list` (the slots of the current one, then more) in the place of the entity's selector.
fn install(w: &mut World, e: Entity, list: Vec<Entry>) {
    let Some(g) = w.get::<GoalSet>(e) else { return };
    let new = g.rebuilt(intern(list));
    if w.get::<GoalEdit>(e).is_none() {
        let base = g.entries();
        w.entity_mut(e).insert(GoalEdit { base });
    }
    w.entity_mut(e).insert(new);
}

/// Stops the goal of `slot` if it runs, with no retry of its own; true when it ran.
fn stop_slot(w: &mut World, e: Entity, slot: u8) -> bool {
    let Some(mut g) = w.get_mut::<GoalSet>(e) else { return false };
    let was = g.stop(slot);
    g.unqueue(slot);
    let (m, inst) = (g.module(slot), g.inst(slot));
    if was {
        (m.stop)(w, e, inst);
    }
    was
}

/// Adds `entry` after the entity's goals: its state is created and its deadlines armed (a sleeping entity arms on waking).
/// False for an entity with no selector, one whose module needs a component it lacks, and a full selector.
pub fn add(w: &mut World, e: Entity, entry: Entry) -> bool {
    let Some(g) = w.get::<GoalSet>(e) else { return false };
    if g.len() >= MAX_ENTRIES || !supported(w, e, entry.module) {
        return false;
    }
    let mut list = g.entries().to_vec();
    let slot = list.len() as u8;
    list.push(entry);
    install(w, e, list);
    let inst = w.get::<GoalSet>(e).unwrap().inst(slot);
    (entry.module.equip)(w, e, inst, entry.params);
    if w.get::<crate::zone::Dormant>(e).is_none() {
        (entry.module.arm)(w, e, inst);
    }
    true
}

/// Removes every goal of the selector (`target`: the target selector) whose module `pred` accepts, a running one stopped
/// first. How many went; None for an entity with no selector.
pub fn remove_where(w: &mut World, e: Entity, target: bool, pred: &dyn Fn(&'static Module) -> bool) -> Option<usize> {
    let g = w.get::<GoalSet>(e)?;
    let hits: Vec<u8> = (0..g.len() as u8)
        .filter(|&s| {
            let m = g.module(s);
            !removed(m) && is_target(m) == target && pred(m)
        })
        .collect();
    if hits.is_empty() {
        return Some(0);
    }
    let mut stopped = false;
    for &s in &hits {
        stopped |= stop_slot(w, e, s);
    }
    let mut list = w.get::<GoalSet>(e)?.entries().to_vec();
    for &s in &hits {
        list[s as usize] = Entry::new(&REMOVED, list[s as usize].priority, &NOTHING);
    }
    install(w, e, list);
    if stopped {
        release_waiting(w, e);
    }
    Some(hits.len())
}

/// `Mob.registerGoals` again: every running goal stops, the species' goals are back in their slots (re-armed where one had
/// been removed), what a mod added is removed. The flags a rider disables stay. False for an entity with no selector.
pub fn reset(w: &mut World, e: Entity) -> bool {
    let Some(g) = w.get::<GoalSet>(e) else { return false };
    let was_running: Vec<u8> = (0..g.len() as u8).filter(|&s| g.slot_running(s)).collect();
    for &s in &was_running {
        stop_slot(w, e, s);
    }
    if let Some(edit) = w.get::<GoalEdit>(e).copied() {
        let old = w.get::<GoalSet>(e).unwrap().entries();
        let mut list = edit.base.to_vec();
        list.extend(old[edit.base.len()..].iter().map(|en| Entry::new(&REMOVED, en.priority, &NOTHING)));
        let back: Vec<u8> = (0..edit.base.len()).filter(|&i| removed(old[i].module)).map(|i| i as u8).collect();
        let whole = list.len() == edit.base.len();
        install(w, e, list);
        if whole {
            w.entity_mut(e).remove::<GoalEdit>();
        }
        if w.get::<crate::zone::Dormant>(e).is_none() {
            for s in back {
                let g = w.get::<GoalSet>(e).unwrap();
                let (m, inst) = (g.module(s), g.inst(s));
                (m.arm)(w, e, inst);
            }
        }
    }
    // A new mob's goals are asked from its first tick: the ones that ran are asked again.
    for s in was_running {
        w.resource_mut::<Requests>().0.push(Req::Start(e, s));
    }
    true
}

/// The goals of the selector (`target`: the target selector) in registration order: priority, module, whether it runs.
/// None for an entity with no selector.
pub fn list(w: &World, e: Entity, target: bool) -> Option<Vec<(u8, &'static Module, bool)>> {
    let g = w.get::<GoalSet>(e)?;
    Some(
        g.entries()
            .iter()
            .enumerate()
            .filter(|(_, en)| !removed(en.module) && is_target(en.module) == target)
            .map(|(i, en)| (en.priority, en.module, g.slot_running(i as u8)))
            .collect(),
    )
}
