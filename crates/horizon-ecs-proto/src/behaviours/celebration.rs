//! `Raider.RaiderCelebration`: after a lost village (the raid is over and lost, the raider still belongs to it) a raider with no target celebrates:
//! it stops (it holds the move flag), a 1-in-50 draw each tick is its cheer (the sound: not done, the draw is taken), a 1-in-25 draw makes it
//! jump unless it rides something. Woken when the raid changes phase (`raids::wake_raiders`), not every tick.
use super::*;
use crate::raids::{raid_of, Celebrating, Raider};
use crate::targeting::Target;
use crate::rules::celebration::{can_celebrate, hit, CHEER_ODDS, JUMP_ODDS};
use crate::{cow::*, goals::*};

/// The jumps drawn (a test inserts it to count them; the jump itself is the physics' affair).
#[derive(Resource, Default)]
pub struct Jumps(pub u64);
#[derive(Component, Clone, Copy, Debug)]
pub struct Celebrate {
    pub slot: u8,
}
pub static MODULE: Module = Module { name: "celebration", flags: MOVE, equip, can_use, start, stop, tick: Some(tick), requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, inst: Inst, _: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Celebrate { slot: inst.slot });
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let alive = w.get::<Health>(e).is_some_and(|h| h.0 > 0.0);
    let in_raid = w.get::<Raider>(e).is_some_and(|r| r.raid.is_some());
    let lost = raid_of(w, e).is_some_and(|r| r.is_loss());
    can_celebrate(alive, w.get::<Target>(e).is_some(), in_raid, lost)
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Celebrating);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Celebrating>();
}
/// The goal's tick is every other game tick (`reducedTickDelay`): the cheer is one draw in 50 of them, the jump one in 25.
fn tick(w: &mut World) {
    let es: Vec<Entity> = w.query_filtered::<Entity, With<Celebrating>>().iter(w).collect();
    for e in es {
        if (now_of(w, e) + phase_of(w, e)) % 2 != 0 {
            continue;
        }
        let _cheer = with_rng(w, e, |r| hit(r.next_int(CHEER_ODDS)));
        if !w.entity(e).contains::<crate::mount::Riding>() && with_rng(w, e, |r| hit(r.next_int(JUMP_ODDS))) {
            if let Some(mut j) = w.get_mut::<Jump>(e) {
                j.0 = true;
            }
            if let Some(mut n) = w.get_resource_mut::<Jumps>() {
                n.0 += 1;
            }
        }
    }
}
