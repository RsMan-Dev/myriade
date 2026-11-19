//! Ambient sounds (`PiglinAi` idle sounds, `mobs/piglin.rs:478-512`): every `min..max` ticks of its own time the animal plays one
//! of `sounds`, drawn from its random source. The goal runs for an instant: start plays, the next try is armed, nothing polls.
use super::*;
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub sounds: &'static [&'static str],
    /// Ticks between two sounds: drawn in `min..=max`.
    pub min: u32,
    pub max: u32,
}
impl Params {
    pub const DEFAULT: Self = Self { sounds: &[], min: 200, max: 400 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Ambient(pub Params);

pub static MODULE: Module = Module {
    name: "ambient_sound",
    flags: 0,
    equip,
    arm,
    can_use,
    start,
    stop,
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Ambient(*params::<Params>(p)));
}
fn delay(w: &mut World, e: Entity) -> i32 {
    let p = w.get::<Ambient>(e).unwrap().0;
    let r = w.get_mut::<Rng>(e).unwrap().next_u32();
    (p.min + r % (p.max - p.min + 1)) as i32
}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    let d = delay(w, e);
    arm_at(w, e, now_of(w, e) + d as u64, Due::Goal(inst.slot));
}
fn can_use(_: &mut World, _: Entity, _: Inst) -> bool {
    true
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    let p = w.get::<Ambient>(e).unwrap().0;
    if !p.sounds.is_empty() {
        let pick = p.sounds[(w.get_mut::<Rng>(e).unwrap().next_u32() as usize) % p.sounds.len()];
        let at = w.get::<Pos>(e).unwrap().0;
        w.resource_mut::<Events>().host.sound(pick, [at.x, at.y, at.z], 1.0, 1.0);
    }
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    arm(w, e, inst);
}
