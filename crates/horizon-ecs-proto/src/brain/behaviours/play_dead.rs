//! Playing dead (`B::PlayDead`, `animal_brain.rs:1119-1127` start, `:1541` tick, `Axolotl.hurtServer` `axolotl.rs:21-56`): when the entity is
//! hurt by a creature while in water it may start playing dead (`play_dead_ticks`, a memory that lasts `ticks`;
//! `is_playing_dead` is read by others: the attackables sensor skips it, a target selector may). The roll is the `Mob`'s: `nextInt(3) == 0`, then
//! `nextInt(3) < damage` or health under half, damage under the health it had, in water, hurt by an entity, not already.
//! Starting playing dead drops the attack target (and starts the hunting cooldown). The behaviour itself (its own activity, 200 ticks):
//! clears the walk and the look, adds Regeneration (the status effect of `effects.rs`); it ends with the memory.
//! `play_dead_ticks` expires by its deadline (the `Mob` counts it down each tick in `ValidatePlayDead`).
use crate::behaviours::Cap;
use crate::brain::sensors::attackables::HAS_HUNTING_COOLDOWN;
use crate::brain::*;
use crate::cow::{FluidState, Health, SpeciesId, SpeciesTable};
use crate::effects;
use crate::rules::play_dead::{hurt_enough, is_first_roll};

pub static PLAY_DEAD_TICKS: MemKey = MemKey::new("minecraft:play_dead_ticks", Shape::Int);

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:play_dead",
    mems: &[&PLAY_DEAD_TICKS, &HURT_BY_ENTITY, &WALK_TARGET, &LOOK_TARGET, &ATTACK_TARGET, &HAS_HUNTING_COOLDOWN],
    requires: &[Cap::Brain, Cap::Living],
    passive: true,
    can_start,
    start,
    tick,
    stop,
    damaged,
    ..Behaviour::DEFAULT
};

/// Whether the entity plays dead (`play_dead_ticks` is present): others do not target it, its animation reads it.
pub fn is_playing_dead(w: &World, e: Entity) -> bool {
    present(w, e, &PLAY_DEAD_TICKS)
}

/// The species' values: how long it plays dead (200), the Regeneration it gets (level, ticks), the hunting cooldown it starts if it
/// was fighting (2400; none: no cooldown).
pub struct Params {
    pub ticks: u64,
    pub regeneration: (i32, i32),
    pub hunting_cooldown: Option<u64>,
}
impl Params {
    pub const DEFAULT: Params = Params { ticks: 200, regeneration: (0, 200), hunting_cooldown: Some(2400) };
}

fn in_water(w: &World, e: Entity) -> bool {
    w.get::<FluidState>(e).is_some_and(|f| f.in_water)
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    in_water(w, e) && present(w, e, &PLAY_DEAD_TICKS) && present(w, e, &HURT_BY_ENTITY)
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    erase(w, e, &WALK_TARGET);
    erase(w, e, &LOOK_TARGET);
    crate::goals::stop_nav(w, e);
    let (amp, ticks) = c.get::<Params>().regeneration;
    effects::add(w, e, effects::Kind::Regeneration, amp, ticks);
}

fn tick(w: &mut World, e: Entity, _: &Ctx) -> bool {
    in_water(w, e) && present(w, e, &PLAY_DEAD_TICKS)
}

fn stop(w: &mut World, e: Entity, c: &Ctx) {
    // `ValidatePlayDead` at the end of the count forgets the culprit; leaving the water only interrupts the behaviour (the memory goes on,
    // and the behaviour starts again in water).
    if timed_out(w, e, c) || !present(w, e, &PLAY_DEAD_TICKS) {
        erase(w, e, &HURT_BY_ENTITY);
    }
}

/// `Axolotl.hurtServer` (after the damage: the health before is the health now and the damage applied).
fn damaged(w: &mut World, e: Entity, c: &Ctx, d: &crate::behaviours::Damaged) {
    let p = c.get::<Params>();
    let (Some(after), Some(sp)) = (w.get::<Health>(e).map(|h| h.0), w.get::<SpeciesId>(e)) else { return };
    let max = w.resource::<SpeciesTable>().0[sp.0 as usize].max_health;
    let before = after + d.amount;
    let first = is_first_roll(crate::goals::with_rng(w, e, |r| r.next_int(3)));
    if !first {
        return;
    }
    let second = hurt_enough(crate::goals::with_rng(w, e, |r| r.next_int(3)), d.amount, before, max);
    let by_entity = w.get::<LastDamage>(e).is_some_and(|l| l.by.is_some());
    if !(second && d.amount < before && in_water(w, e) && by_entity && !is_playing_dead(w, e)) {
        return;
    }
    if present(w, e, &ATTACK_TARGET) {
        erase(w, e, &ATTACK_TARGET);
        crate::targeting::clear_target(w, e);
        if let Some(t) = p.hunting_cooldown {
            set(w, e, &HAS_HUNTING_COOLDOWN, MemValue::Bool(true), Some(t));
        }
    }
    set(w, e, &PLAY_DEAD_TICKS, MemValue::Int(p.ticks as i64), Some(p.ticks - 1));
}
