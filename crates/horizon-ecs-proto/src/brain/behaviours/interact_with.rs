//! `InteractWith.of(kind, range, memory, speed, close)` (`PiglinBruteAi`, idle): with no walk target and a visible mob of the named kind
//! within `range` blocks, the nearest becomes the interaction target, is looked at and walked to.
use crate::behaviours::play_tag::INTERACTION_TARGET;
use crate::brain::behaviours::look_at_entity::species_id;
use crate::brain::*;
use crate::goals::d2;
use crate::targeting::{info, Who};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:interact_with",
    mems: &[&WALK_TARGET, &LOOK_TARGET, &VISIBLE_MOBS, &INTERACTION_TARGET],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

pub struct Params {
    pub species: &'static str,
    pub range: f64,
    pub speed: f32,
    pub close: i32,
}

/// The nearest visible mob of the kind within the range of the entity.
fn near(w: &mut World, e: Entity, p: &Params) -> Option<Who> {
    let id = species_id(w, p.species)?;
    let at = w.get::<Pos>(e)?.0;
    find_closest(w, e, |w, who| info(w, who).is_some_and(|i| i.species == id && d2(at, i.pos) <= p.range * p.range))
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    if present(w, e, &WALK_TARGET) || !present(w, e, &VISIBLE_MOBS) {
        return false;
    }
    near(w, e, c.get::<Params>()).is_some()
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    let Some(who) = near(w, e, p) else { return };
    set(w, e, &INTERACTION_TARGET, MemValue::Who(who), None);
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(who, true)), None);
    set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Entity(who, false), speed: p.speed, close: p.close }), None);
}
