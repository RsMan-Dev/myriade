//! `StopAdmiringIfItemTooFarAway` (`PiglinAi` ADMIRE_ITEM, `create(9)`): with the off hand empty, the admiration ends when the wanted item
//! is gone or not closer than 9 blocks (`closerThan`).
use crate::brain::sensors::nearest_wanted_item::NEAREST_WANTED_ITEM;
use crate::brain::*;
use crate::cow::Pos;
use crate::goals::Slot;
use crate::goals;
use crate::targeting::Who;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:stop_admiring_if_item_too_far_away",
    mems: &[&ADMIRING_ITEM, &NEAREST_WANTED_ITEM],
    requires: &[Cap::Brain],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

pub struct Params {
    /// The item must be closer than this many blocks to keep the admiration (`closerThan(piglin, 9)`).
    pub distance: f64,
}

/// `closerThan`: the squared distance is below the squared limit.
pub fn is_closer(dist_sqr: f64, distance: f64) -> bool {
    dist_sqr < distance * distance
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    if !present(w, e, &ADMIRING_ITEM) || goals::equipped(w, e, Slot::Off) != 0 {
        return false;
    }
    let Some(me) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    match get(w, e, &NEAREST_WANTED_ITEM) {
        Some(MemValue::Who(Who::Entity(it))) => {
            let near = w.get::<Pos>(*it).is_some_and(|q| is_closer(q.0.distance_squared(me), c.get::<Params>().distance));
            !near
        }
        _ => true,
    }
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    erase(w, e, &ADMIRING_ITEM);
}

#[cfg(test)]
mod tests {
    #[test]
    fn nine_blocks_is_too_far() {
        assert!(super::is_closer(8.0 * 8.0, 9.0));
        assert!(!super::is_closer(9.0 * 9.0, 9.0));
    }
}
