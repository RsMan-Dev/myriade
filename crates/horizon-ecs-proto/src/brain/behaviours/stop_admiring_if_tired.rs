//! `StopAdmiringIfTiredOfTryingToReachItem` (`PiglinAi` ADMIRE_ITEM, `create(200, 200)`): while the item is admired and in view with the
//! off hand empty, the attempts counter `TIME_TRYING_TO_REACH_ADMIRE_ITEM` goes up one per tick. Past `max_time`, the admiration ends,
//! the counter is cleared and the walk to admire is disabled for `ttl` ticks.
use crate::brain::sensors::nearest_wanted_item::NEAREST_WANTED_ITEM;
use crate::brain::*;
use crate::goals::Slot;
use crate::goals;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:stop_admiring_if_tired_of_trying_to_reach_item",
    mems: &[&ADMIRING_ITEM, &NEAREST_WANTED_ITEM, &TIME_TRYING_TO_REACH_ADMIRE_ITEM, &DISABLE_WALK_TO_ADMIRE_ITEM],
    requires: &[Cap::Brain],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

pub struct Params {
    /// Ticks of trying before the admiration is given up (`TIME_TRYING_TO_REACH_ADMIRE_ITEM` above it).
    pub max_time: i64,
    /// Ticks the walk to admire stays disabled after giving up.
    pub ttl: u64,
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    present(w, e, &ADMIRING_ITEM) && present(w, e, &NEAREST_WANTED_ITEM) && goals::equipped(w, e, Slot::Off) == 0
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    match int(w, e, &TIME_TRYING_TO_REACH_ADMIRE_ITEM) {
        None => set(w, e, &TIME_TRYING_TO_REACH_ADMIRE_ITEM, MemValue::Int(0), None),
        Some(t) if t > p.max_time => {
            erase(w, e, &ADMIRING_ITEM);
            erase(w, e, &TIME_TRYING_TO_REACH_ADMIRE_ITEM);
            set(w, e, &DISABLE_WALK_TO_ADMIRE_ITEM, MemValue::Bool(true), Some(p.ttl));
        }
        Some(t) => set(w, e, &TIME_TRYING_TO_REACH_ADMIRE_ITEM, MemValue::Int(t + 1), None),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_counter_gives_up_past_the_limit() {
        let give_up = |t: i64| t > 200;
        assert!(!give_up(200));
        assert!(give_up(201));
    }
}
