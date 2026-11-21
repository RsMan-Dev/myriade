//! `CalmDown` (`villager_brain.rs:2402`, the first behaviour of the panic activity): the hurt memories are forgotten when nothing
//! keeps the villager scared (no hurt, no nearest hostile, the hurter farther than `range_sqr`); then the schedule is resynced.
use crate::behaviours::schedule;
use crate::brain::*;
use crate::brain::sensors::village_hostile::NEAREST_HOSTILE;
use crate::cow::*;
use crate::goals::d2;
use crate::rules::calm_down::keeps_scared;
use crate::targeting::info;

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:calm_down", mems: &[&HURT_BY, &HURT_BY_ENTITY, &NEAREST_HOSTILE], one_shot: true, can_start, ..Behaviour::DEFAULT };

pub struct Params {
    /// Squared distance (blocks squared) within which the hurter keeps the villager scared.
    pub range_sqr: f64,
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let range = c.get::<Params>().range_sqr;
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let hurter = match get(w, e, &HURT_BY_ENTITY) {
        Some(MemValue::Who(who)) => info(w, *who).map(|i| d2(pos, i.pos)),
        _ => None,
    };
    if !keeps_scared(present(w, e, &HURT_BY), present(w, e, &NEAREST_HOSTILE), hurter, range) {
        erase(w, e, &HURT_BY);
        erase(w, e, &HURT_BY_ENTITY);
        restart(w, e, &schedule::BEHAVIOUR);
    }
    true
}
