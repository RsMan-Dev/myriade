//! Ambient sounds: either every `min..max` ticks of its own time the animal plays one of `sounds` (`PiglinAi` idle sounds,
//! `mobs/piglin.rs:478-512`), or with `interval` the `Mob.baseTick` law (`rules::ambient_sound::vanilla_delay`), and `choose` names the
//! sound (the wolf's whine, pant or growl). The goal runs for an instant: start plays, the next try is armed, nothing polls.
use super::*;
use crate::rules::ambient_sound;
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub sounds: &'static [&'static str],
    /// Ticks between two sounds: drawn in `min..=max`.
    pub min: u32,
    pub max: u32,
    /// `Mob.getAmbientSoundInterval`: when set, the delay follows `Mob.baseTick` instead of `min..max`.
    pub interval: Option<u32>,
    /// The interval of the creature's state (`HappyGhast.getAmbientSoundInterval`), read when the next delay is armed (not each tick).
    pub interval_of: Option<fn(&World, Entity) -> u32>,
    /// The sound of this play, chosen by the species from the creature's state; `sounds` is unused when set.
    pub choose: Option<fn(&mut World, Entity) -> &'static str>,
    /// The sounds of this list are played at volume 2 (`Fox.playAmbientSound`: the screech).
    pub loud: &'static [&'static str],
    /// Multiplied onto the voice pitch of the creature (`Bat.getVoicePitch`: 0.95 times the living pitch).
    pub voice_pitch: f32,
    /// `getVoicePitch` returns 1 with no draw (`HappyGhast`): the baby is not raised either.
    pub fixed_pitch: bool,
}
impl Params {
    pub const DEFAULT: Self = Self { sounds: &[], min: 200, max: 400, interval: None, interval_of: None, choose: None, loud: &[], voice_pitch: 1.0, fixed_pitch: false };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Ambient(pub &'static Params);

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
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Ambient(params::<Params>(p)));
}
fn delay(w: &mut World, e: Entity) -> i32 {
    let p = w.get::<Ambient>(e).unwrap().0;
    let played = w.get::<Played>(e).is_some();
    match p.interval {
        // `Mob.ambientSoundTime` is 0 at the start, and only a played sound sets it to `-interval`.
        Some(i) => {
            let i = p.interval_of.map_or(i, |f| f(w, e));
            let mut rng = w.get_mut::<Rng>(e).unwrap();
            ambient_sound::base_tick_delay(if played { i } else { 0 }, || rng.next_int(1000) as u32)
        }
        None => ambient_sound::delay(p.min, p.max, w.get_mut::<Rng>(e).unwrap().next_u32()),
    }
}
/// The tick of the entity's own time at which the one pending deadline falls: `arm_timers` may run again (a player arrives, the
/// zone wakes the animal) and must not start a second chain of sounds.
#[derive(Component, Clone, Copy)]
struct Armed(u64, Inst);
/// The creature has played a sound once: its counter then restarts at `-interval`, not at 0 as in a new creature.
#[derive(Component, Clone, Copy)]
struct Played;
fn arm(w: &mut World, e: Entity, inst: Inst) {
    let now = now_of(w, e);
    if w.get::<Armed>(e).is_some_and(|a| a.0 > now) {
        return;
    }
    let at = now + delay(w, e) as u64;
    w.entity_mut(e).insert(Armed(at, inst));
    arm_at(w, e, at, Due::Goal(inst.slot));
}
fn can_use(_: &mut World, _: Entity, _: Inst) -> bool {
    true
}
/// The sound of the creature's ambient, drawn from its random source and queued at its place; false without one.
pub fn play(w: &mut World, e: Entity) -> bool {
    let Some(p) = w.get::<Ambient>(e).map(|a| a.0) else { return false };
    let name = match p.choose {
        Some(choose) => choose(w, e),
        None if p.sounds.is_empty() => return false,
        None => p.sounds[ambient_sound::pick(w.get_mut::<Rng>(e).unwrap().next_u32(), p.sounds.len())],
    };
    // A `choose` that names no sound (`""`) is the vanilla null ambient: nothing plays.
    if name.is_empty() {
        return false;
    }
    let Some(name) = crate::sound_slots::sound(w, e, |o| o.ambient, Some(name)) else { return false };
    if !p.loud.contains(&name) {
        // `Mob.playAmbientSound`: the species' `getSoundVolume`, its own source and the voice pitch (a baby's raised).
        crate::tear_down::voice(w, e, name);
        return true;
    }
    let at = w.get::<Pos>(e).unwrap().0;
    w.resource_mut::<Events>().host.sound(name, [at.x, at.y, at.z], 2.0, 1.0);
    true
}
/// `Mob.playHurtSound` calls `resetAmbientSoundTime`: a hurt creature's counter restarts at `-interval`, and the sound already pending is
/// replaced by a later one. Only the `interval` law has the counter.
pub fn reset_on_hurt(w: &mut World, e: Entity) {
    let Some(Ambient(p)) = w.get::<Ambient>(e).copied() else { return };
    let Some(Armed(_, inst)) = w.get::<Armed>(e).copied() else { return };
    if p.interval.is_none() {
        return;
    }
    w.entity_mut(e).insert(Played);
    let at = now_of(w, e) + delay(w, e) as u64;
    w.entity_mut(e).insert(Armed(at, inst));
    arm_at(w, e, at, Due::Goal(inst.slot));
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    // A timer of before a hurt: the deadline is later now.
    if w.get::<Armed>(e).is_some_and(|a| a.0 > now_of(w, e)) {
        w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
        return;
    }
    w.entity_mut(e).remove::<Armed>().insert(Played);
    play(w, e);
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    arm(w, e, inst);
}
