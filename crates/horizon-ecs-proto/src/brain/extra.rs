//! Behaviours an entity carries besides its species' plan: added to one entity after the plan was frozen (a mod's).
//! They take their rank in `Brain.tick`'s order (priority, the activity's slot, then insertion: after the plan's own of
//! the same rank). An entity without any does not carry the component and its step is the plan's alone.
use super::*;

/// What an added behaviour runs; `u16` is its index in the entity's [`Extra`].
#[derive(Clone, Copy)]
pub struct ExtraFns {
    pub can_start: fn(&mut World, Entity, u16) -> bool,
    pub start: fn(&mut World, Entity, u16),
    /// `false` ends it (`canStillUse`).
    pub tick: fn(&mut World, Entity, u16) -> bool,
    pub stop: fn(&mut World, Entity, u16),
}
pub struct ExtraEntry {
    act: u8,
    needs_l: Vec<(u16, bool)>,
    duration: Option<(i32, i32)>,
    fns: ExtraFns,
    /// How many of the plan's top-level behaviours go before it.
    rank: u16,
    running: bool,
    end: u64,
}
#[derive(Component, Default)]
pub struct Extra {
    entries: Vec<ExtraEntry>,
    /// The entries in the order they are asked.
    order: Vec<u16>,
}
impl Extra {
    pub fn running(&self, k: u16) -> bool {
        self.entries.get(k as usize).is_some_and(|x| x.running)
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
#[derive(Clone, Copy)]
enum Id {
    Plan(u16),
    Extra(u16),
}

/// Adds a behaviour to `e`'s brain and returns its index; `None` when `e` has no brain or the activity is not one of its
/// plan. `needs` naming a memory the plan does not hold is never present.
pub fn add_extra(w: &mut World, e: Entity, activity: &str, priority: u8, needs: &[(&'static MemKey, Need)], duration: Option<(i32, i32)>, fns: ExtraFns) -> Option<u16> {
    let mut b = w.get_mut::<Brain>(e)?;
    let plan = b.plan;
    let act = plan.acts.iter().position(|a| a.name == activity)? as u8;
    b.extra = true;
    let key = (priority, plan.acts[act as usize].slot);
    let rank = plan.order.iter().filter(|&&i| (plan.insts[i as usize].prio, plan.acts[plan.insts[i as usize].act as usize].slot) <= key).count() as u16;
    let needs_l = needs.iter().map(|(k, n)| (plan.local(k).map_or(NONE, |l| l as u16), *n == Need::Present)).collect();
    if w.get::<Extra>(e).is_none() {
        w.entity_mut(e).insert(Extra::default());
    }
    let mut x = w.get_mut::<Extra>(e)?;
    let k = x.entries.len() as u16;
    x.entries.push(ExtraEntry { act, needs_l, duration, fns, rank, running: false, end: 0 });
    // Stable: equal ranks keep the order they were added in.
    let at = x.order.iter().position(|&o| x.entries[o as usize].rank > rank).unwrap_or(x.order.len());
    x.order.insert(at, k);
    wake(w, e);
    Some(k)
}
/// Whether the species of `e` declares the memory `k`: its brain plan holds a slot for it (`MemoryStatus.REGISTERED`).
pub fn declares(w: &World, e: Entity, k: &MemKey) -> bool {
    w.get::<Brain>(e).is_some_and(|b| b.plan.local(k).is_some())
}
/// Removes every added behaviour of `e` (the ones running are stopped first).
pub fn clear_extra(w: &mut World, e: Entity) {
    let n = w.get::<Extra>(e).map_or(0, |x| x.entries.len()) as u16;
    for k in 0..n {
        stop(w, e, k);
    }
    w.entity_mut(e).remove::<Extra>();
    if let Some(mut b) = w.get_mut::<Brain>(e) {
        b.extra = false;
    }
}

fn merged(w: &World, e: Entity, plan: &Plan) -> Vec<Id> {
    let Some(x) = w.get::<Extra>(e) else { return plan.order.iter().map(|&i| Id::Plan(i)).collect() };
    let mut out = Vec::with_capacity(plan.order.len() + x.order.len());
    let mut next = x.order.iter().peekable();
    for (n, &i) in plan.order.iter().enumerate() {
        while let Some(&&k) = next.peek().filter(|&&&k| x.entries[k as usize].rank as usize <= n) {
            out.push(Id::Extra(k));
            next.next();
        }
        out.push(Id::Plan(i));
    }
    out.extend(next.map(|&k| Id::Extra(k)));
    out
}
/// The step's start loop then its tick loop over the plan's behaviours and the added ones, in one order.
pub(super) fn start_and_tick(w: &mut World, e: Entity, plan: &'static Plan) {
    let seq = merged(w, e, plan);
    for &id in &seq {
        match id {
            Id::Plan(i) => {
                if w.get::<Brain>(e).is_some_and(|b| b.ready(i)) {
                    try_start(w, e, plan, i);
                }
            }
            Id::Extra(k) => try_start_extra(w, e, k),
        }
    }
    for &id in &seq {
        match id {
            Id::Plan(i) => {
                if running(w, e, i) {
                    tick_or_stop(w, e, plan, i);
                }
            }
            Id::Extra(k) => tick_extra(w, e, k),
        }
    }
}
/// An added behaviour that runs keeps the brain stepped every tick.
pub(super) fn any_running(w: &World, e: Entity) -> bool {
    w.get::<Extra>(e).is_some_and(|x| x.entries.iter().any(|y| y.running))
}
fn try_start_extra(w: &mut World, e: Entity, k: u16) {
    let Some((fns, duration)) = (|| {
        let b = w.get::<Brain>(e)?;
        let y = w.get::<Extra>(e)?.entries.get(k as usize)?;
        ((y.act == 0 || b.activity == y.act) && !y.running && b.holds(&y.needs_l)).then_some((y.fns, y.duration))
    })() else {
        return;
    };
    if !(fns.can_start)(w, e, k) {
        return;
    }
    let end = match duration {
        Some((min, max)) => now_of(w, e) + (min + with_rng(w, e, |r| r.next_int(max + 1 - min))) as u64,
        None => 0,
    };
    if let Some(y) = w.get_mut::<Extra>(e).as_mut().and_then(|x| x.entries.get_mut(k as usize)) {
        y.running = true;
        y.end = end;
    }
    (fns.start)(w, e, k);
}
fn tick_extra(w: &mut World, e: Entity, k: u16) {
    let Some((fns, timed, end)) = w.get::<Extra>(e).and_then(|x| x.entries.get(k as usize)).filter(|y| y.running).map(|y| (y.fns, y.duration.is_some(), y.end)) else { return };
    if (timed && now_of(w, e) > end) || !(fns.tick)(w, e, k) {
        stop(w, e, k);
    }
}
fn stop(w: &mut World, e: Entity, k: u16) {
    let Some(mut x) = w.get_mut::<Extra>(e) else { return };
    let Some(y) = x.entries.get_mut(k as usize).filter(|y| y.running) else { return };
    y.running = false;
    let fns = y.fns;
    (fns.stop)(w, e, k);
    wake(w, e);
}
