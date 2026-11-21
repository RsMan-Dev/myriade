//! `SocializeAtBell` (`villager_brain.rs:2367`, `:4089`): a villager within 4 blocks of its meeting point (the bell's block), with a visible
//! mob and no interaction target, passes a draw of `nextInt(100) == 0`; then, if a villager of its own species is visible, the nearest one
//! within 32 squared becomes the interaction target and the look target, and the villager walks to it (speed 0.3, reach 1). One shot: it
//! starts even when no villager is within 32 squared, as the Mob does.
use crate::behaviours::play_tag::INTERACTION_TARGET;
use crate::brain::*;
use crate::cow::{Pos, SpeciesId};
use crate::goals::{d2, with_rng};
use crate::rules::social_socialize_bell as rules;
use crate::targeting::{info, Who};

pub struct Params {
    /// The memory of the meeting point (a block).
    pub bell: &'static MemKey,
}

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_socialize_at_bell",
    mems: &[&INTERACTION_TARGET, &LOOK_TARGET, &WALK_TARGET, &VISIBLE_MOBS],
    one_shot: true,
    can_start,
    ..Behaviour::DEFAULT
};

/// `closerToCenterThan(bell, 4)`: the centre of the block is within 4 blocks.
fn at_bell(w: &World, e: Entity, bell: &MemKey) -> bool {
    let Some(MemValue::Block(b)) = get(w, e, bell) else { return false };
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    rules::at_bell(*b, pos)
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    if present(w, e, &INTERACTION_TARGET) || !present(w, e, &VISIBLE_MOBS) || !at_bell(w, e, p.bell) {
        return false;
    }
    if with_rng(w, e, |r| r.next_int(100)) != 0 {
        return false;
    }
    let Some(own) = w.get::<SpeciesId>(e).map(|s| s.0) else { return false };
    let is_villager = |w: &World, who: Who| info(w, who).is_some_and(|i| i.species == own);
    if find_closest(w, e, is_villager).is_none() {
        return false;
    }
    let Some(pos) = w.get::<Pos>(e).map(|q| q.0) else { return false };
    if let Some(who) = find_closest(w, e, |w, who| is_villager(w, who) && info(w, who).is_some_and(|i| rules::in_reach(d2(pos, i.pos)))) {
        set(w, e, &INTERACTION_TARGET, MemValue::Who(who), None);
        set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(who, true)), None);
        set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Entity(who, false), speed: rules::SPEED, close: rules::CLOSE }), None);
    }
    true
}
