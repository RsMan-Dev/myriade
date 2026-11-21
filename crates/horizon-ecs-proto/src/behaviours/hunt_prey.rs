//! Target goal of a hunter that chases a prey species (`HuntHoglin`, `mobs/piglin.rs:1375-1462`): an adult with no target, past its
//! cooldown, with no adult hunter of its kind in range still cooling down, takes the nearest prey within `range` that it sees as its
//! target. Then every adult hunter of its kind within `range` draws its own cooldown, and takes the prey as target when it has none
//! (`HuntHoglin.start`, the `HuntHoglin` event). The cooldown is drawn in `cooldown` ticks. Woken by the cooldown being due when the
//! selector asks; nothing polls.
use super::*;
use crate::{cow::*, goals::*, targeting::{self, Target, Who}};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The species hunted, by name.
    pub prey: &'static [&'static str],
    /// The reach of the hunt: the prey, and the hunters that share the cooldown and the target.
    pub range: f64,
    /// `unseenMemoryTicks` of the target taken.
    pub memory: u16,
    /// Ticks between two hunts (drawn from the animal's random source).
    pub cooldown: (u32, u32),
}
impl Params {
    pub const DEFAULT: Self = Self { prey: &[], range: 16.0, memory: 60, cooldown: (600, 2400) };
}
/// The hunter's cooldown: the hunt is allowed at `until`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Hunt {
    pub p: &'static Params,
    pub until: u64,
}

pub static MODULE: Module = Module {
    name: "hunt_prey",
    flags: TARGET,
    equip,
    can_use,
    start,
    requires: &[Cap::Body, Cap::Goals, Cap::Targets],
    ..Module::DEFAULT
};
fn draw(w: &mut World, e: Entity, (lo, hi): (u32, u32)) -> u64 {
    let r = w.get_mut::<Rng>(e).unwrap().next_u32();
    now_of(w, e) + u64::from(lo + r % (hi - lo + 1))
}
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    let until = draw(w, e, p.cooldown);
    w.entity_mut(e).insert(Hunt { p, until });
}
/// The hunter that may not hunt (`AbstractPiglin.canHunt`): the saved `CannotHunt` flag. Sparse: absent means it may hunt.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct CannotHunt;

/// `canUse`: an adult past its cooldown, with no target, not angry, not avoiding a repellent, allowed to hunt, with no adult hunter of
/// its kind in range still cooling down, and a prey in sight.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let h = *w.get::<Hunt>(e).unwrap();
    let now = now_of(w, e);
    if w.get::<GrowAt>(e).is_some() || w.get::<targeting::Target>(e).is_some() || now < h.until || w.get::<CannotHunt>(e).is_some() {
        return false;
    }
    if crate::behaviours::neutral_anger::is_angry(w, e) || w.get::<super::walk_away::Avoid>(e).is_some_and(|a| now < a.until) {
        return false;
    }
    if near_hunters(w, e, h.p.range).iter().any(|&(_, until)| now < until) {
        return false;
    }
    prey_near(w, e, &h.p).is_some()
}
/// The adult hunters of the kind of `e` within `range` (not `e`), with the time their cooldown ends.
fn near_hunters(w: &mut World, e: Entity, range: f64) -> Vec<(Entity, u64)> {
    let Some(at) = w.get::<Pos>(e).map(|p| p.0) else { return Vec::new() };
    let kind = w.get::<SpeciesId>(e).map(|k| k.0);
    let mut q = w.query::<(Entity, &Pos, &SpeciesId, &Hunt)>();
    let found: Vec<(Entity, u64)> = q
        .iter(w)
        .filter(|(other, pos, id, _)| *other != e && Some(id.0) == kind && pos.0.distance_squared(at) <= range * range)
        .map(|(other, _, _, h)| (other, h.until))
        .collect();
    found.into_iter().filter(|(other, _)| w.get::<GrowAt>(*other).is_none()).collect()
}
/// The nearest prey within `range` (by name) that `e` sees, the first found wins a tie.
fn prey_near(w: &mut World, e: Entity, p: &Params) -> Option<Entity> {
    let at = w.get::<Pos>(e)?.0;
    let mut q = w.query::<(Entity, &Pos, &SpeciesId)>();
    let near: Vec<(f64, Entity)> = q
        .iter(w)
        .filter(|(other, _, id)| *other != e && p.prey.contains(&crate::species::registry().species[usize::from(id.0)].name))
        .map(|(other, pos, _)| (pos.0.distance_squared(at), other))
        .filter(|&(d, _)| d <= p.range * p.range)
        .collect();
    let world: &World = w;
    near.into_iter()
        .filter(|&(_, other)| targeting::sees(world, e, Who::Entity(other)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, other)| other)
}
/// `start`: the prey is the target; the hunter draws its cooldown, and the hunters of its kind near it draw theirs and take the prey
/// as target when they have none.
fn start(w: &mut World, e: Entity, inst: Inst) {
    let h = *w.get::<Hunt>(e).unwrap();
    let Some(prey) = prey_near(w, e, &h.p) else { return };
    targeting::set_target(w, e, Target::new(Who::Entity(prey), h.p.memory, inst.slot));
    let until = draw(w, e, h.p.cooldown);
    w.get_mut::<Hunt>(e).unwrap().until = until;
    for (other, _) in near_hunters(w, e, h.p.range) {
        let until = draw(w, e, h.p.cooldown);
        if let Some(mut hunt) = w.get_mut::<Hunt>(other) {
            hunt.until = until;
        }
        if w.get::<targeting::Target>(other).is_none() {
            targeting::set_target(w, other, Target::new(Who::Entity(prey), h.p.memory, inst.slot));
        }
    }
}
