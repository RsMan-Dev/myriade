//! `TradeWithVillager` (`villager_brain.rs:1623`, `:2909`): while its interaction target is a villager of its own species that it still
//! sees, a villager looks at it, and within 5 blocks it meets it (`VillagerMeet`: the gossips go round by `gossip::meet`). The duration
//! is the entry's (60 ticks).
use crate::behaviours::gossip;
use crate::behaviours::play_tag::INTERACTION_TARGET;
use crate::brain::*;
use crate::cow::{Pos, SpeciesId};
use crate::goals::d2;
use crate::targeting::{info, Who};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_trade_with_villager",
    mems: &[&INTERACTION_TARGET, &LOOK_TARGET, &VISIBLE_MOBS],
    can_start,
    start,
    tick,
    ..Behaviour::DEFAULT
};

/// The interaction target, when it is a villager of the own species that is still seen.
fn partner(w: &mut World, e: Entity) -> Option<Entity> {
    let other = match get(w, e, &INTERACTION_TARGET) {
        Some(MemValue::Who(Who::Entity(o))) => *o,
        _ => return None,
    };
    let own = w.get::<SpeciesId>(e).map(|s| s.0);
    let who = Who::Entity(other);
    (info(w, who).is_some_and(|i| Some(i.species) == own) && visible_contains(w, e, who)).then_some(other)
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    partner(w, e).is_some()
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    gaze(w, e);
}

fn tick(w: &mut World, e: Entity, _: &Ctx) -> bool {
    let Some(other) = partner(w, e) else { return false };
    gaze(w, e);
    let (Some(me), Some(at)) = (w.get::<Pos>(e).map(|p| p.0), info(w, Who::Entity(other)).map(|i| i.pos)) else { return false };
    if d2(me, at) <= 5.0 {
        gossip::meet(w, e, other);
    }
    true
}

fn gaze(w: &mut World, e: Entity) {
    let who = match get(w, e, &INTERACTION_TARGET) {
        Some(MemValue::Who(who)) => *who,
        _ => return,
    };
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(who, true)), None);
}
