//! Celebration after a hunt that ended with the prey killed (`celebrate`, `mobs/piglin.rs:1874-1905`): the animal stands and
//! makes `sound` for `duration` ticks. `start_party` is called by the hunt's result; nothing is polled, the end is a deadline.
use super::*;
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub sound: &'static str,
    pub duration: u64,
}
impl Params {
    pub const DEFAULT: Self = Self { sound: "minecraft:entity.piglin.celebrate", duration: 300 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Party {
    pub p: &'static Params,
    pub slot: u8,
}
/// The celebration runs until `until`.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Celebrating {
    pub until: u64,
}

pub static MODULE: Module = Module {
    name: "celebrate",
    flags: MOVE,
    equip,
    can_use,
    start,
    stop,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Party { p: params::<Params>(p), slot: inst.slot });
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Celebrating>(e).is_some()
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let at = w.get::<Pos>(e).unwrap().0;
    let sound = w.get::<Party>(e).unwrap().p.sound;
    w.resource_mut::<Events>().host.sound(sound, [at.x, at.y, at.z], 1.0, 1.0);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Celebrating>();
}
/// `celebrate` as the goal's own: the party is started on a hunt's result, and the goal ends when its time is up.
pub fn start_party(w: &mut World, e: Entity) {
    let Some(p) = w.get::<Party>(e).copied() else { return };
    let until = now_of(w, e) + p.p.duration;
    w.entity_mut(e).insert(Celebrating { until });
    after(w, e, p.p.duration as i32, time_up);
    w.resource_mut::<Requests>().0.push(Req::Start(e, p.slot));
}
/// The deadline of a party: the end is asked at its tick, unless the party was restarted (a later deadline is armed) or is over.
fn time_up(w: &mut World, e: Entity) {
    let (Some(c), Some(p)) = (w.get::<Celebrating>(e).copied(), w.get::<Party>(e).copied()) else { return };
    if now_of(w, e) >= c.until {
        w.resource_mut::<Requests>().0.push(Req::End(e, p.slot));
    }
}
