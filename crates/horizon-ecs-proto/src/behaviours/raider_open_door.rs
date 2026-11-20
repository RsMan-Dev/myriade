//! `AbstractIllager.RaiderOpenDoorGoal`: during a raid, open the wooden door on the path (`door_interact`). It never continues: it ends the tick
//! after it started, and closing the door again is its `stop`. No move flag. Woken by the contact with a wall.
use super::door_interact::{self as door, OpenSeeker};
use super::*;
use crate::raids::has_active_raid;
use crate::goals::*;

pub static MODULE: Module = Module { name: "raider_open_door", flags: 0, equip, can_use, start, stop, tick: Some(tick), requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, inst: Inst, _: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(OpenSeeker { slot: inst.slot });
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    door::can_use(w, e) && has_active_raid(w, e)
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    door::set_open(w, e, true);
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    door::set_open(w, e, false);
}
fn tick(w: &mut World) {
    door::tick::<OpenSeeker>(w);
}
