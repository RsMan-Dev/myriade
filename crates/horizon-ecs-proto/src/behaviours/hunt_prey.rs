//! Target goal of a hunter that chases a prey species (`HuntHoglin`, `mobs/piglin.rs:1375-1462`): an adult with no target and
//! past its cooldown takes the nearest prey within `range` as its target, and every hunter of its kind within `group` blocks
//! gets the same cooldown (the delay is common to the group). The cooldown is drawn in `cooldown` ticks. Woken by the cooldown
//! being due when the selector asks; nothing polls.
use super::*;
use crate::{cow::*, goals::*, targeting::{self, Target, Who}};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The species hunted, by name.
    pub prey: &'static [&'static str],
    pub range: f64,
    /// Blocks within which the hunters share the cooldown.
    pub group: f64,
    /// `unseenMemoryTicks` of the target taken.
    pub memory: u16,
    /// Ticks between two hunts (drawn from the animal's random source).
    pub cooldown: (u32, u32),
}
impl Params {
    pub const DEFAULT: Self = Self { prey: &[], range: 16.0, group: 8.0, memory: 60, cooldown: (600, 2400) };
}
/// The hunter's cooldown: the hunt is allowed at `until`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Hunt {
    pub p: Params,
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
    let p = *params::<Params>(p);
    let until = draw(w, e, p.cooldown);
    w.entity_mut(e).insert(Hunt { p, until });
}
/// `canUse`: an adult past its cooldown, with no target, and a prey in range.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let h = *w.get::<Hunt>(e).unwrap();
    if w.get::<GrowAt>(e).is_some() || w.get::<targeting::Target>(e).is_some() || now_of(w, e) < h.until {
        return false;
    }
    prey_near(w, e, &h.p).is_some()
}
/// The nearest prey within `range` (by name), the first found wins a tie.
fn prey_near(w: &mut World, e: Entity, p: &Params) -> Option<Entity> {
    let at = w.get::<Pos>(e)?.0;
    let mut best: Option<(f64, Entity)> = None;
    let mut q = w.query::<(Entity, &Pos, &SpeciesId)>();
    for (other, pos, id) in q.iter(w) {
        if other == e || !p.prey.contains(&crate::species::registry().species[usize::from(id.0)].name) {
            continue;
        }
        let d = pos.0.distance_squared(at);
        if d <= p.range * p.range && best.is_none_or(|(b, _)| d < b) {
            best = Some((d, other));
        }
    }
    best.map(|(_, o)| o)
}
/// `start`: the prey is the target; the hunter and the hunters of its kind near it wait the same cooldown.
fn start(w: &mut World, e: Entity, inst: Inst) {
    let h = *w.get::<Hunt>(e).unwrap();
    let Some(prey) = prey_near(w, e, &h.p) else { return };
    targeting::set_target(w, e, Target::new(Who::Entity(prey), h.p.memory, inst.slot));
    let until = draw(w, e, h.p.cooldown);
    let at = w.get::<Pos>(e).unwrap().0;
    let kind = w.get::<SpeciesId>(e).unwrap().0;
    let mut group = Vec::new();
    let mut q = w.query::<(Entity, &Pos, &SpeciesId)>();
    for (other, pos, id) in q.iter(w) {
        if id.0 == kind && pos.0.distance_squared(at) <= h.p.group * h.p.group {
            group.push(other);
        }
    }
    for other in group {
        if let Some(mut hunt) = w.get_mut::<Hunt>(other) {
            hunt.until = until;
        }
    }
    w.get_mut::<Hunt>(e).unwrap().until = until;
}
