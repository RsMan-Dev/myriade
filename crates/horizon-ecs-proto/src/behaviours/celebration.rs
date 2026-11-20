//! `Raider.RaiderCelebration`: after a lost village (the raid is over and lost, the raider still belongs to it) a raider with no target celebrates:
//! it stops (it holds the move flag), a 1-in-50 draw each tick is its cheer (the sound: not done, the draw is taken), a 1-in-25 draw makes it
//! jump unless it rides something. Woken when the raid changes phase (`raids::wake_raiders`), not every tick.
use super::*;
use crate::raids::{raid_of, Celebrating, Raider};
use crate::targeting::Target;
use crate::{cow::*, goals::*};

#[derive(Component, Clone, Copy, Debug)]
pub struct Celebrate {
    pub slot: u8,
}
pub static MODULE: Module = Module { name: "celebration", flags: MOVE, equip, can_use, start, stop, tick: Some(tick), requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, inst: Inst, _: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Celebrate { slot: inst.slot });
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Health>(e).is_some_and(|h| h.0 > 0.0) && w.get::<Target>(e).is_none() && w.get::<Raider>(e).is_some_and(|r| r.raid.is_some()) && raid_of(w, e).is_some_and(|r| r.is_loss())
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Celebrating);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Celebrating>();
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, run);
}
fn run(mut q: Query<(&mut Rng, Option<&mut Jump>, Has<crate::mount::Riding>), With<Celebrating>>) {
    for (mut rng, jump, riding) in &mut q {
        let _cheer = rng.next_int(50) == 0;
        if !riding
            && rng.next_int(25) == 0
            && let Some(mut j) = jump
        {
            j.0 = true;
        }
    }
}
