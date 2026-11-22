//! `StartHuntingHoglin` (`PiglinAi` IDLE, `triggerIf` adult): an adult piglin with no anger and no recent hunt starts hunting the huntable
//! hoglin in view, unless a nearby adult piglin has hunted recently. The hoglin becomes the anger target for 600 ticks
//! (`setAngerTarget`), and the hunt is on cooldown for 30 to 120 seconds, for it and the nearby adult piglins (`dontKillAnyMoreHoglinsForAWhile`).
use crate::brain::*;
use crate::targeting::{self, Who};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:start_hunting_hoglin",
    mems: &[&NEAREST_VISIBLE_HUNTABLE_HOGLIN, &ANGRY_AT, &HUNTED_RECENTLY, &NEAREST_VISIBLE_ADULT_PIGLINS],
    requires: &[Cap::Brain, Cap::Targets],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

/// `TIME_BETWEEN_HUNTS`, `UniformInt(30 s, 120 s)` in ticks.
pub const HUNT_COOLDOWN: (i32, i32) = (600, 2400);

/// The anger on the hoglin lasts this many ticks (`setAngerTarget`).
const ANGER_TICKS: u64 = 600;

fn hunted_recently(w: &World, who: Who) -> bool {
    matches!(who, Who::Entity(a) if present(w, a, &HUNTED_RECENTLY))
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    let baby = targeting::info(w, Who::Entity(e)).is_some_and(|i| i.baby);
    let adults_hunting = match get(w, e, &NEAREST_VISIBLE_ADULT_PIGLINS) {
        Some(MemValue::Whos(list)) => list.iter().any(|&a| hunted_recently(w, a)),
        _ => false,
    };
    let huntable = present(w, e, &NEAREST_VISIBLE_HUNTABLE_HOGLIN);
    huntable && !baby && !adults_hunting && !present(w, e, &ANGRY_AT) && !present(w, e, &HUNTED_RECENTLY)
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    let hoglin = match get(w, e, &NEAREST_VISIBLE_HUNTABLE_HOGLIN) {
        Some(MemValue::Who(who)) => *who,
        _ => return,
    };
    set(w, e, &ANGRY_AT, MemValue::Who(hoglin), Some(ANGER_TICKS));
    let adults = match get(w, e, &NEAREST_VISIBLE_ADULT_PIGLINS) {
        Some(MemValue::Whos(list)) => list.clone(),
        _ => Vec::new(),
    };
    // `broadcastAngerTarget`: the nearby adult piglins that are not angry yet take the hoglin as their target too.
    for &who in &adults {
        if let Who::Entity(a) = who {
            if a != e && !present(w, a, &ANGRY_AT) {
                set(w, a, &ANGRY_AT, MemValue::Who(hoglin), Some(ANGER_TICKS));
            }
        }
    }
    for who in std::iter::once(Who::Entity(e)).chain(adults) {
        if let Who::Entity(a) = who {
            let ticks = cooldown(w, a);
            set(w, a, &HUNTED_RECENTLY, MemValue::Bool(true), Some(ticks));
        }
    }
}

/// A draw of `TIME_BETWEEN_HUNTS` in ticks.
fn cooldown(w: &mut World, e: Entity) -> u64 {
    let (lo, hi) = HUNT_COOLDOWN;
    (lo + with_rng(w, e, |r| r.next_int(hi - lo + 1))) as u64
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_cooldown_is_30_to_120_seconds() {
        assert_eq!(super::HUNT_COOLDOWN, (600, 2400));
    }
}
