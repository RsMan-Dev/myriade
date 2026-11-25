//! A creature that crumbles: its death lasts `TICKS` ticks instead of 20 (`Creaking.tickDeath`: `deathTime > 45`), then it vanishes with
//! no loot (`remove(DISCARDED)`) and the host shows the crumbling (`MiscEvent::Crumbled`). While it crumbles it takes nothing (its
//! `DamageLink` window is shut for good) and its brain does not run (`brain_step` skips `Crumbling`). The deadline is on the wheel.
use crate::cow::after;
use crate::damage_link::DamageLink;
use crate::host_out_misc::MiscEvent;
use bevy_ecs::prelude::*;

/// `deathTime > 45`: the ticks between the start of the death and the crumbling.
pub const TICKS: i32 = 46;

/// The death is running; the flag the client reads (`is_tearing_down`) is this marker.
/// `sound` is the death sound it makes at the end.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet", on_insert = crate::bridge::view::touched, on_remove = crate::bridge::view::touched)]
pub struct Crumbling {
    /// None: the plain death of a health put to 0 (`LivingEntity.tickDeath`: 20 ticks, the smoke at the end), not the heart-bound crumbling.
    pub sound: Option<&'static str>,
}

/// `LivingEntity.die`'s `setPose(Pose.DYING)`: the pose the client reads (index 6) to lay the body down. Only a real death poses it; `fade`
/// (a health put to 0 with no source) leaves the pose as it was.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet", on_insert = crate::bridge::view::touched, on_remove = crate::bridge::view::touched)]
pub struct DyingPose;

/// `LivingEntity.tickDeath`: `deathTime >= 20`.
pub const FADE_TICKS: i32 = 20;

/// `setHealth(0)` with no damage source (`Creaking.tick` of an orphan): no `die`, so no loot, no experience, no death said to mods. The
/// body lies `FADE_TICKS` ticks (the client sees the health), then the smoke and it is gone. Once only.
pub fn fade(w: &mut World, e: Entity) {
    if w.get::<Crumbling>(e).is_some() || w.get::<crate::cow::Dying>(e).is_some() || w.get_entity(e).is_err() {
        return;
    }
    if let Some(mut h) = w.get_mut::<crate::cow::Health>(e) {
        h.0 = 0.0;
    }
    crate::bridge::view::mark(w, e);
    linger(w, e);
}

/// The one predicate "this entity is dead": a body that lingers (`Crumbling`) or a death held by the host's veto (`cow::Dying`). Every
/// reader that must leave the dead alone (damage, targeting, pushing, the brain, a second death) asks this and nothing else.
pub type Dead = Or<(With<Crumbling>, With<crate::cow::Dying>)>;

/// `is_dead` for the code that holds the world.
pub fn is_dead(w: &World, e: Entity) -> bool {
    w.get::<Crumbling>(e).is_some() || w.get::<crate::cow::Dying>(e).is_some()
}

/// The body of a dead entity stays `FADE_TICKS` ticks (`LivingEntity.tickDeath`): untargetable, unpushable, no brain (the `Dead`
/// readers), then the smoke and it is gone. The deadline is on the wheel, nothing is polled. Used by `fade` and by every ordinary death.
/// What it still held lets go once, here: riders get off, the lead drops, the goals end and none starts, the path stops, the effects end.
pub fn linger(w: &mut World, e: Entity) {
    if w.get::<Crumbling>(e).is_some() || w.get_entity(e).is_err() {
        return;
    }
    w.entity_mut(e).insert((Crumbling { sound: None }, DamageLink { until: u64::MAX }));
    if w.get::<crate::mount::Mount>(e).is_some() {
        crate::mount::eject_passengers(w, e);
    }
    if crate::leash::is_leashed(w, e) {
        crate::leash::break_lead(w, e, false);
    }
    crate::goals::set_disabled_flags(w, e, crate::goals::MOVE | crate::goals::LOOK | crate::goals::JUMP | crate::goals::TARGET);
    crate::goals::stop_nav(w, e);
    let kinds: Vec<_> = w.get::<crate::effects::Effects>(e).map_or_else(Vec::new, |f| f.list.iter().map(|x| x.kind).collect());
    for kind in kinds {
        crate::effects::remove(w, e, kind);
    }
    after(w, e, FADE_TICKS, finish);
}

/// `Mob.voice`: `name` at the entity's place with a pitch drawn from its own stream; nothing is heard while it is `Silent`.
pub fn voice(w: &mut World, e: Entity, name: &'static str) {
    // The voice is queued by a hit: the entity may be gone by the time it runs, and a thing without a stream of its own has no voice.
    if w.get::<crate::cow::Rng>(e).is_none() {
        return;
    }
    // `AgeableMob.getVoicePitch`: a baby's voice is raised by half an octave, the same two draws.
    let base = if w.get::<crate::cow::GrowAt>(e).is_some() { 1.5 } else { 1.0 };
    let pitch = crate::goals::with_rng(w, e, |r| {
        let a = (r.next_u32() >> 8) as f32 / 16_777_216.0;
        let b = (r.next_u32() >> 8) as f32 / 16_777_216.0;
        (a - b) * 0.2 + base
    });
    if w.get::<crate::behaviours::elder_curse::Silent>(e).is_none() {
        crate::behaviours::act_phase::sound(w, e, name, 1.0, pitch);
    }
}

/// Starts the death of `e`: it stops taking hits, the client plays the death pose and the host is told when it is over. Once only.
pub fn start(w: &mut World, e: Entity, sound: &'static str) {
    if w.get::<Crumbling>(e).is_some() || w.get_entity(e).is_err() {
        return;
    }
    w.entity_mut(e).insert((Crumbling { sound: Some(sound) }, DamageLink { until: u64::MAX }));
    if let (Some(id), Some(p)) = (crate::bridge::ids::id_of(w, e), w.get::<crate::cow::Pos>(e).map(|p| p.0)) {
        w.resource_mut::<crate::goals::Events>().host.death_animation(id, [p.x, p.y, p.z]);
    }
    after(w, e, TICKS, finish);
}

/// `LivingEntity.tickDeath`'s `broadcastEntityEvent(60)`: the smoke of `e` about its box, told to the host before `e` goes.
pub fn smoke(w: &mut World, e: Entity) {
    let (Some(b), Some(id)) = (w.get::<crate::cow::Bounds>(e).map(|b| b.0), crate::bridge::ids::id_of(w, e)) else { return };
    let (lo, hi) = (b.min_position(), b.max_position());
    let at = [(lo.x + hi.x) / 2.0, lo.y, (lo.z + hi.z) / 2.0];
    w.resource_mut::<crate::goals::Events>().host.death_smoke(id, at, (hi.x - lo.x) / 2.0, hi.y - lo.y);
}

/// The end of the death: the centre and the spread of the box (`Creaking.tearDown`: a third of its size) go to the host, the entity goes.
fn finish(w: &mut World, e: Entity) {
    let Some(Crumbling { sound }) = w.get::<Crumbling>(e).copied() else { return };
    let bounds = w.get::<crate::cow::Bounds>(e).map(|b| b.0);
    if sound.is_none() {
        smoke(w, e);
    }
    if let Some(sound) = sound {
        voice(w, e, sound);
    }
    if let (Some(_), Some(b)) = (sound, bounds) {
        let (lo, hi) = (b.min_position(), b.max_position());
        let center = [(lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, (lo.z + hi.z) / 2.0];
        let spread = [(hi.x - lo.x) * 0.3, (hi.y - lo.y) * 0.3, (hi.z - lo.z) * 0.3];
        w.get_resource_or_insert_with(crate::colony_register::MiscOut::default).0.push(MiscEvent::Crumbled { entity: e, center, spread });
    }
    if let Some(c) = w.get::<crate::cow::Cell>(e).map(|c| c.0)
        && c != u32::MAX
    {
        w.resource_mut::<crate::cow::Index>().remove(e, c);
    }
    crate::removed_effects::on_killed(w, e);
    crate::spawn_trial::forget(w, e);
    crate::mount::leave(w, e);
    w.despawn(e);
}
