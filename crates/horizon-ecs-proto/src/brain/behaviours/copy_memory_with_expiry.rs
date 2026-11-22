//! `CopyMemoryWithExpiry.create(from, to, duration)` (javap `ai/behavior/CopyMemoryWithExpiry`): a one-shot that copies the value of
//! `from` into `to` for a time drawn from `duration` (a range in ticks, its `UniformInt`). The piglin's `babyAvoidNemesis` and
//! `avoidZombified` make a `NEAREST_VISIBLE_*` memory into an `AVOID_TARGET` this way; the copy expires by its deadline.
use crate::brain::*;

/// `mems` holds the piglin's copy target (`AVOID_TARGET`): a memory a behaviour writes must be in the plan, and the species lists it.
pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:copy_memory_with_expiry", mems: &[&super::walk_away::AVOID_TARGET], one_shot: true, can_start, start, ..Behaviour::DEFAULT };

/// `from` and `to` are the memories; `min` and `spread` give the time (`min` to `min + spread` ticks, as `UniformInt`);
/// `when` is the predicate of `CopyMemoryWithExpiry.create` (`triggerIf` in vanilla).
pub struct Params {
    pub from: &'static MemKey,
    pub to: &'static MemKey,
    pub min: i32,
    pub spread: i32,
    pub when: fn(&World, Entity) -> bool,
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    (p.when)(w, e) && present(w, e, p.from)
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    let Some(value) = get(w, e, p.from).cloned() else { return };
    let ticks = p.min + with_rng(w, e, |r| r.next_int(p.spread.max(1))) as i32;
    set(w, e, p.to, value, Some(ticks.max(0) as u64));
}
