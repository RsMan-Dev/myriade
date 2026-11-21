//! `BreakDoorGoal`: crack the wooden door in the way for 240 ticks, then remove it. Two entries of one code: `MODULE` (a zombie: no flag, hard only,
//! only a creature that carries the entry can) and `RAID` (`Vindicator.VindicatorBreakDoorGoal`: holds the move flag, normal and hard, only in an
//! active raid, a 1-in-10 draw to start). The door is found by the contact with a wall (`door_interact`); `mobGriefing` and the door's rules are
//! asked before the start. Not done: the crack animation of the block (`DestroyProgress`), the sounds (the 1-in-20 draw of the door sound is taken),
//! the arm swing, `Zombie.setCanBreakDoors`' random draw at spawn (the entry says which species can).
use super::door_interact::{self as door, BreakSeeker, DoorAt, RaidBreakSeeker};
use super::*;
use crate::raids::{difficulty, has_active_raid};
use crate::rules::break_door::{self as rules, BREAK_TIME, RAID_START_ODDS, SOUND_ODDS};
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub raid: bool,
}
impl Params {
    pub const ZOMBIE: Self = Self { raid: false };
    pub const VINDICATOR: Self = Self { raid: true };
}
/// The goal's state: its slot, the ticks of cracking.
#[derive(Component, Clone, Copy, Debug)]
pub struct BreakDoor {
    pub p: Params,
    pub slot: u8,
    time: i32,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Breaking;

pub static MODULE: Module = Module { name: "break_door", flags: 0, equip, can_use, start, stop, tick: Some(tick), requires: &[Cap::Locomotion], ..Module::DEFAULT };
pub static RAID: Module = Module { name: "raid_break_door", flags: MOVE, equip: equip_raid, can_use: can_use_raid, start, stop, tick: Some(tick_raid), requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert((BreakDoor { p: *params::<Params>(p), slot: inst.slot, time: 0 }, BreakSeeker { slot: inst.slot }));
}
fn equip_raid(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert((BreakDoor { p: *params::<Params>(p), slot: inst.slot, time: 0 }, RaidBreakSeeker { slot: inst.slot }));
}
/// `DOOR_BREAKING_PREDICATE`.
fn difficulty_ok(w: &World, raid: bool) -> bool {
    let d = difficulty(w);
    if raid { d >= 2 } else { d >= 3 }
}
fn can_break(w: &mut World, e: Entity, raid: bool) -> bool {
    door::can_use(w, e) && mob_griefing(w) && difficulty_ok(w, raid) && !door::is_open(w, e)
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    can_break(w, e, false)
}
fn can_use_raid(w: &mut World, e: Entity, _: Inst) -> bool {
    has_active_raid(w, e) && with_rng(w, e, |r| rules::hit(r.next_int(RAID_START_ODDS))) && can_break(w, e, true)
}
fn start(w: &mut World, e: Entity, _: Inst) {
    if let Some(mut b) = w.get_mut::<BreakDoor>(e) {
        b.time = 0;
    }
    w.entity_mut(e).insert(Breaking);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Breaking>();
}
fn tick(w: &mut World) {
    door::tick::<BreakSeeker>(w);
    tick_state(w, run::<false>);
}
fn tick_raid(w: &mut World) {
    door::tick::<RaidBreakSeeker>(w);
    tick_state(w, run::<true>);
}
/// `canContinue` and `tick`.
/// One instance per module (`RAID`: the raid's goal, the vindicator's): each runs the creatures of its own, so that two carried modules do not step a creature twice.
fn run<const RAID: bool>(q: Query<(Entity, &BreakDoor), With<Breaking>>, mut cmd: Commands) {
    for (e, b) in &q {
        if b.p.raid != RAID {
            continue;
        }
        let (slot, raid) = (b.slot, b.p.raid);
        cmd.queue(move |w: &mut World| {
            let at = w.get::<DoorAt>(e).copied().unwrap_or_default();
            let pos = w.get::<Pos>(e).map(|p| p.0);
            let time = w.get::<BreakDoor>(e).map_or(0, |b| b.time);
            let keep = rules::keeps_breaking(
                raid,
                has_active_raid(w, e),
                time,
                door::is_open(w, e),
                pos.is_some_and(|p| door::closer_to_center(at.pos, p, 2.0)),
                difficulty_ok(w, raid),
            );
            if !keep {
                w.resource_mut::<Requests>().0.push(Req::End(e, slot));
                return;
            }
            // The door's sound is a 1-in-20 draw, kept so the stream of draws is the `Mob`'s.
            let _ = with_rng(w, e, |r| r.next_int(SOUND_ODDS));
            let time = time + 1;
            w.get_mut::<BreakDoor>(e).unwrap().time = time;
            if time == BREAK_TIME && difficulty_ok(w, raid) {
                w.resource_mut::<BlockChanges>().0.push((at.pos[0], at.pos[1], at.pos[2], "minecraft:air"));
            }
        });
    }
}
