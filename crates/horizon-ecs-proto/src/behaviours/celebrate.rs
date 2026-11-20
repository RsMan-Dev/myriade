//! Celebration after a hunt that ended with the prey killed (`celebrate`, `mobs/piglin.rs:1874-1905`): the animal stands and
//! makes `sound` for `duration` ticks. `start_party` is called by the hunt's result; nothing is polled, the end is one tick
//! check on the animals that celebrate.
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
    pub p: Params,
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
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Party { p: *params::<Params>(p), slot: inst.slot });
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
    w.resource_mut::<Requests>().0.push(Req::Start(e, p.slot));
}
fn tick(w: &mut World) {
    let due: Vec<(Entity, u8)> = {
        let mut q = w.query::<(Entity, &Celebrating, &Party)>();
        q.iter(w).filter(|(e, c, _)| now_of(w, *e) >= c.until).map(|(e, _, p)| (e, p.slot)).collect()
    };
    for (e, slot) in due {
        w.resource_mut::<Requests>().0.push(Req::End(e, slot));
    }
}
