//! `StartCelebratingIfTargetDead` (`PiglinAi` CELEBRATE and CORE, `create(300, wantsToDance)`): the attack target is dead; a hoglin may get
//! a dance (`DANCING`, 300 ticks, one draw in ten), the celebration spot is the piglin's block (`CELEBRATE_LOCATION`, 300 ticks). A dead
//! player is not handled here (`forgive_dead_player` clears its anger).
use crate::brain::*;
use crate::rules::celebration::hit;
use crate::species::HOGLIN;
use crate::targeting::{self, Who};
use super::look_at_entity::species_id;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:start_celebrating_if_target_dead",
    mems: &[&ATTACK_TARGET, &ANGRY_AT, &CELEBRATE_LOCATION, &DANCING],
    requires: &[Cap::Brain, Cap::Targets],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

pub struct Params {
    /// Ticks the celebration spot and the dance last.
    pub duration: u64,
}

fn target(w: &World, e: Entity) -> Option<Who> {
    match get(w, e, &ATTACK_TARGET) {
        Some(MemValue::Who(who)) => Some(*who),
        _ => None,
    }
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    let Some(who) = target(w, e) else { return false };
    // `exempt` on a creature means dead (`Info`); a creative or spectator player is not dead, and a dead player is forgiven by `forgive_dead_player`.
    targeting::info(w, who).is_some_and(|i| !i.player && i.exempt) && !present(w, e, &CELEBRATE_LOCATION)
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    let Some(who) = target(w, e) else { return };
    let hoglin = targeting::info(w, who).is_some_and(|i| i.species == species_id(w, HOGLIN).unwrap_or(u8::MAX));
    if hoglin && hit(with_rng(w, e, |r| r.next_int(10))) {
        set(w, e, &DANCING, MemValue::Bool(true), Some(p.duration));
    }
    let Some(at) = w.get::<crate::cow::Pos>(e).map(|q| q.0) else { return };
    set(w, e, &CELEBRATE_LOCATION, MemValue::Block([at.x.floor() as i32, at.y.floor() as i32, at.z.floor() as i32]), Some(p.duration));
}

#[cfg(test)]
mod tests {
    #[test]
    fn one_draw_in_ten_dances() {
        assert!(super::hit(0));
        assert!(!super::hit(9));
    }
}
