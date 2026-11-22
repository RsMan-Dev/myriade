//! `RememberIfHoglinWasKilled` (`PiglinAi` IDLE, after `StartHuntingHoglin`): the attack target is a hoglin that is dead; the hunt is then on
//! cooldown for `HUNTED_RECENTLY` (the same draw as the hunt start, 30 to 120 seconds). Ends at once, returning true.
use crate::brain::*;
use crate::species::HOGLIN;
use crate::targeting::{self, Who};
use super::look_at_entity::species_id;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:remember_if_hoglin_was_killed",
    mems: &[&ATTACK_TARGET, &HUNTED_RECENTLY],
    requires: &[Cap::Brain, Cap::Targets],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

fn killed_hoglin(w: &World, e: Entity) -> Option<Who> {
    match get(w, e, &ATTACK_TARGET) {
        Some(MemValue::Who(who @ Who::Entity(_))) if targeting::info(w, *who).is_some_and(|i| !i.player && i.species == species_id(w, HOGLIN).unwrap_or(u8::MAX) && i.exempt) => Some(*who),
        _ => None,
    }
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    killed_hoglin(w, e).is_some()
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    let (lo, hi) = super::start_hunting_hoglin::HUNT_COOLDOWN;
    let ticks = (lo + with_rng(w, e, |r| r.next_int(hi - lo + 1))) as u64;
    set(w, e, &HUNTED_RECENTLY, MemValue::Bool(true), Some(ticks));
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_hunt_cooldown_is_thirty_to_120_seconds() {
        assert_eq!(super::super::start_hunting_hoglin::HUNT_COOLDOWN, (600, 2400));
    }
}
