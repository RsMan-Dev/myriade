//! `GoToWantedItem` as the piglin's ADMIRE_ITEM uses it (`PiglinAi.initAdmireItemActivity`: `isNotHoldingLovedItemInOffHand`, speed 1.0,
//! range 9): with no walk target and the off hand not holding a loved item, the wanted item in range is walked to and looked at. Within
//! reach it is picked up: a loved item goes to the off hand (`holdInOffhand`), any other to the main hand.
use crate::brain::sensors::nearest_wanted_item::NEAREST_WANTED_ITEM;
use crate::brain::*;
use crate::cow::Pos;
use crate::equipment;
use crate::goals::{self, set_main_hand, Slot};
use crate::targeting::Who;
use crate::{item, items};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:go_to_wanted_item",
    mems: &[&WALK_TARGET, &LOOK_TARGET, &NEAREST_WANTED_ITEM],
    wakes_on_item: 4.0,
    can_start,
    start,
    tick,
    ..Behaviour::DEFAULT
};

pub struct Params {
    pub speed: f32,
    /// The wanted item must be within this many blocks (9 for the piglin).
    pub range: f64,
}

/// `isNotHoldingLovedItemInOffHand`: the off hand is empty, or holds something that is not a loved item.
pub fn off_hand_free(off: u32, loved: bool) -> bool {
    off == 0 || !loved
}

/// The wanted item the sensor remembers, while it is on the ground, pickable and within `range`.
fn wanted_near(w: &World, e: Entity, range: f64) -> Option<Entity> {
    let Some(MemValue::Who(Who::Entity(it))) = get(w, e, &NEAREST_WANTED_ITEM) else { return None };
    let me = w.get::<Pos>(e)?.0;
    let at = w.get::<Pos>(*it)?.0;
    (item::pickable(w, *it) && at.distance_squared(me) <= range * range).then_some(*it)
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let loved = match get(w, e, &NEAREST_WANTED_ITEM) {
        Some(MemValue::Who(Who::Entity(it))) => super::start_admiring_item_if_seen::is_loved(w, *it),
        _ => false,
    };
    walk_target(w, e).is_none() && off_hand_free(goals::equipped(w, e, Slot::Off), loved) && wanted_near(w, e, c.get::<Params>().range).is_some()
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let Some(it) = wanted_near(w, e, c.get::<Params>().range) else { return };
    let Some(at) = w.get::<Pos>(it).map(|p| p.0) else { return };
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Point([at.x, at.y + 0.2125, at.z])), None);
    let walk = WalkTarget { target: Tracker::Point([at.x, at.y, at.z]), speed: c.get::<Params>().speed, close: 0 };
    set(w, e, &WALK_TARGET, MemValue::Walk(walk), None);
}

/// Walking until the path is done; then the item within reach (1.5 blocks, squared 2.25) is picked up and the behaviour ends.
fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    if walk_target(w, e).is_some() || !nav_done(w, e) {
        return true;
    }
    erase(w, e, &WALK_TARGET);
    let Some(me) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let reached = wanted_near(w, e, c.get::<Params>().range).filter(|&it| w.get::<Pos>(it).is_some_and(|q| q.0.distance_squared(me) <= 2.25));
    if let Some(stack) = reached.and_then(|it| item::pick_up(w, e, it, u32::MAX)) {
        if items::has_tag("minecraft:piglin_loved", items::item_id(&stack.item)) {
            equipment::set_slot(w, e, Slot::Off, items::item_id(&stack.item));
        } else {
            set_main_hand(w, e, Some(stack));
        }
    }
    false
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_loved_item_in_the_off_hand_stops_the_walk() {
        assert!(super::off_hand_free(0, true));
        assert!(!super::off_hand_free(3, true));
    }
}
