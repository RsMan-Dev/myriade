//! The dragon's hurts: a hit on one of its parts (`EnderDragon.hurt`, `reallyHurt`, D:708-968), the knockback, the explosion's share
//! (D:968) and the contacts with the players and creatures in the wings and head (`dragon_contacts`, D:2931; DC). Sitting damage and
//! the takeoff are the perch's (`perch.rs`); the death is `death.rs`'s.
use super::data::*;
use super::parts::{part_box, Parts};
use super::perch;
use super::phases::{set_phase, PhaseId};
use crate::area::{creature_xz, creatures_in, hurt_creature, hurt_player, player_xz, players_in, push_creature, push_player};
use super::flight::DragonRng;
use crate::cow::{Cause, Clock, Health, Pos, Vel};
use crate::explosion::blast_damage;
use crate::goals::{Ev, Events};
use crate::hostile::PlayerState;
use crate::rules::dragon_hurt as rule;
pub use crate::rules::dragon_hurt::CREDIT_TICKS;
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};

/// What `EnderDragon.hurt` made of a hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hurt {
    /// It landed; `full` when it was a new hit and not just the excess over the last one (sound, knockback).
    Landed { full: bool },
    /// Refused: dying, too little damage, or fire on a part.
    Refused,
    /// A perched dragon took a direct arrow, trident or wind charge and shrugged it off.
    Shrugged,
}

/// One hit on one part. `arrow`: a direct arrow, trident or wind charge (a perched dragon shrugs it off). `from`: the attacker's
/// feet for a melee hit (the knockback pushes away from it). Counted as a source (`reallyHurt`) when `by` is a player or the
/// cause is a blast.
#[derive(Clone, Copy, Debug)]
pub struct Blow {
    pub part: usize,
    pub damage: f32,
    pub cause: Cause,
    pub by: Option<Who>,
    pub arrow: bool,
    pub from: Option<(f64, f64)>,
}

/// The dragon's own hurt clocks: the damage window (`invulnerable`), the last hit's damage (`last_hurt`) and `hurt_time`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct DragonHurt {
    pub window: i32,
    pub last: f32,
    pub time: i32,
}

/// `lastHurtByPlayer`: the player whose hit landed last, and the dragon's tick of it (credited if the kill follows within 100 ticks).
#[derive(Component, Clone, Copy, Debug)]
pub struct HurtByPlayer {
    pub player: u32,
    pub tick: u32,
}

/// A hit that reached `reallyHurt` (the `PartHit` of the spec). The fight (L5) reads it.
#[derive(Clone, Copy, Debug)]
pub struct PartHit {
    pub dragon: Entity,
    pub part: u8,
    pub source: Cause,
    pub by: Option<Who>,
    pub amount: f32,
}

/// The hits of this step, drained by the fight (its `Ev` is swapped like the others).
#[derive(Resource, Default)]
pub struct PartHits(pub Ev<PartHit>);

/// `#minecraft:is_fire`, which the dragon's parts refuse (`isInvulnerableToBase`): fire, burning floor and lava.
fn is_fire(cause: Cause) -> bool {
    matches!(cause, Cause::InFire | Cause::OnFire | Cause::Lava)
}

fn phase_of(w: &World, e: Entity) -> u8 {
    w.get::<PhaseId>(e).map_or(HOVERING, |p| p.0)
}

fn alive(w: &World, e: Entity) -> bool {
    w.get::<Health>(e).is_some_and(|h| h.0 > 0.0)
}

/// `EnderDragonPart.hurtServer` and `EnderDragon.hurt`, then `reallyHurt` for a hit that counts.
pub fn hurt(w: &mut World, e: Entity, b: Blow) -> Hurt {
    if is_fire(b.cause) {
        return Hurt::Refused;
    }
    let phase = phase_of(w, e);
    if phase == DYING || !alive(w, e) {
        return Hurt::Refused;
    }
    let sitting = is_sitting(phase);
    if b.arrow && sitting {
        return Hurt::Shrugged;
    }
    // The body parts take a quarter of the hit and at most one more; the head and neck take all of it.
    let damage = rule::part_damage(b.part, b.damage);
    if damage < MIN_DAMAGE {
        return Hurt::Refused;
    }
    let counts = matches!(b.by, Some(Who::Player(_))) || b.cause == Cause::Blast;
    if !counts {
        return Hurt::Landed { full: false };
    }
    let clock = w.get::<DragonHurt>(e).copied().unwrap_or_default();
    // `LivingEntity.hurtServer`: inside the window only the excess over the last hit lands.
    let window = rule::window_open(clock.window);
    if window && damage <= clock.last {
        return Hurt::Landed { full: false };
    }
    let dealt = rule::dealt(damage, clock.last, window);
    let before = w.get::<Health>(e).map_or(0.0, |h| h.0);
    let (after, dying) = rule::health_after(before, dealt, sitting);
    let full = !window;
    if dying {
        // `handleKillingBlow`: the dragon flies its dying flight first.
        set_phase(w, e, DYING);
    }
    if let Some(mut h) = w.get_mut::<Health>(e) {
        h.0 = after;
    }
    let ticks = w.get::<super::flight::Flight>(e).map_or(0, |f| f.ticks);
    if let Some(Who::Player(player)) = b.by {
        w.entity_mut(e).insert(HurtByPlayer { player, tick: ticks });
    }
    if after <= 0.0 || phase_of(w, e) == DYING {
        credit(w, e, b, ticks);
    }
    w.entity_mut(e).insert(DragonHurt { window: rule::window_after(window, clock.window), last: damage, time: rule::time_after(full, clock.time) });
    if full && let Some((fx, fz)) = b.from {
        let p = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
        knockback(w, e, f64::from(KNOCKBACK), fx - p.x, fz - p.z);
    }
    let at = w.get::<Pos>(e).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
    w.resource_mut::<PartHits>().0.push(PartHit {
        dragon: e,
        part: b.part as u8,
        source: b.cause,
        by: b.by,
        amount: dealt,
    });
    if full {
        // The hurt animation of the guests (`host.hurt`), as `cow::health` emits it.
        let cause = match b.by {
            Some(Who::Player(p)) => p,
            Some(Who::Entity(a)) => w.get::<crate::bridge::ids::NetId>(a).map_or(0, |n| n.0),
            None => 0,
        };
        if let Some(id) = w.get::<crate::bridge::ids::NetId>(e).map(|n| n.0) {
            let name = crate::cow::cause_name(b.cause, b.by);
            w.resource_mut::<Events>().host.hurt(id, name, cause, b.by.is_some() && !b.arrow, at);
        }
        w.resource_mut::<Events>().host.sound("minecraft:entity.ender_dragon.hurt", at, 5.0, 1.0);
    }
    if is_sitting(phase_of(w, e)) {
        perch::sitting_hurt(w, e, before - after.max(0.0));
    }
    Hurt::Landed { full }
}

/// The killing blow (`!was_down && down()`): the cause is kept for the fight, and the last player to hurt it within 100 ticks is credited.
fn credit(w: &mut World, e: Entity, b: Blow, ticks: u32) {
    if let Some(mut f) = w.get_resource_mut::<super::fight::EndFight>() {
        f.last_blow = Some(b.cause);
    }
    let Some(h) = w.get::<HurtByPlayer>(e).copied().filter(|h| rule::in_credit(ticks, h.tick)) else { return };
    let by_player = b.by.is_some();
    if let Some(mut ev) = w.get_resource_mut::<super::fight::FightEvents>() {
        ev.credit.push(super::fight::KillCredit { by: h.player, last_blow: b.cause, by_player });
    }
}

/// `EnderDragon.knockBack`: nothing while perched; else the velocity is halved and pushed `power` away from `(xd, zd)` (the hit
/// minus the dragon). A zero direction is redrawn from the dragon's own stream.
fn knockback(w: &mut World, e: Entity, power: f64, mut xd: f64, mut zd: f64) {
    if is_sitting(phase_of(w, e)) || power <= 0.0 {
        return;
    }
    if let Some(mut rng) = w.get_mut::<DragonRng>(e) {
        (xd, zd) = rule::redraw_dir(xd, zd, &mut rng.0);
    }
    let (kx, kz) = rule::push_dir(xd, zd, power);
    if let Some(mut v) = w.get_mut::<Vel>(e) {
        v.0 = Vec3::new(v.0.x / 2.0 - kx, v.0.y, v.0.z / 2.0 - kz);
    }
}

/// `ServerExplosion.hurtEntities` on the dragon: the whole body (reported as the body part) and then each part in the blast
/// (distance at most the diameter), each with the blast's damage (`explosion::blast_damage`) and `exposure` of its box.
pub fn explosion_hits(w: &World, e: Entity, center: [f64; 3], radius: f32, exposure: impl Fn(Aabb) -> f32) -> Vec<(usize, f32)> {
    let (Some(p), Some(parts)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Parts>(e)) else { return Vec::new() };
    let diameter = f64::from(radius * 2.0);
    let dist = |at: [f64; 3]| rule::blast_dist(at, center, diameter);
    let whole = [p.x, p.y, p.z];
    std::iter::once((BODY, whole, whole_box(p)))
        .chain((0..8).map(|i| (i, parts.0[i].pos, part_box(parts, i))))
        .filter_map(|(part, at, b)| {
            let d = dist(at);
            (d <= 1.0).then(|| (part, blast_damage(d, exposure(b), diameter)))
        })
        .collect()
}

/// `Entity.pushFromExplosion` on the dragon (`ServerExplosion` knockback of the whole body, not of the parts): the push along the
/// blast to its eye, `(1 - distance) * exposure`. `None` outside the blast.
pub fn explosion_push(w: &World, e: Entity, center: [f64; 3], radius: f32, exposure: f32) -> Option<[f64; 3]> {
    let p = w.get::<Pos>(e)?.0;
    rule::blast_push([p.x, p.y, p.z], center, radius, exposure)
}

/// The wing and head zones of the dragon: each wing's box inflated and dropped (`WING_ZONE`, `WING_DROP`), each head's inflated.
fn zones(parts: &Parts) -> ([Aabb; 2], [Aabb; 2], Aabb) {
    let wing = |i: usize| rule::wing_box(part_box(parts, i));
    let head = |i: usize| rule::head_box(part_box(parts, i));
    ([wing(WING_1), wing(WING_2)], [head(HEAD), head(NECK)], part_box(parts, BODY))
}

/// `EnderDragon.aiStep`'s contacts (`dragon_contacts`, DC): the wings push and hurt, the head and neck hurt, the players and the
/// creatures alike. Nothing while the dragon was hurt lately (`hurt_time`). Team rules are not read (see the notes).
pub fn contacts(w: &mut World, e: Entity) {
    let clock = w.get::<DragonHurt>(e).copied().unwrap_or_default();
    if clock.time > 0 {
        return;
    }
    let sitting = is_sitting(phase_of(w, e));
    let (wings, heads, body) = match w.get::<Parts>(e) {
        Some(parts) => zones(parts),
        None => return,
    };
    let (bc, bz) = rule::centre_xz(body);
    let now = w.resource::<Clock>().0;
    for zone in wings {
        for id in players_in(w, zone) {
            if w.resource::<PlayerState>().creative(id) || w.resource::<PlayerState>().spectator(id) {
                continue;
            }
            let (x, z) = player_xz(w, id);
            let push = rule::push_from(x, z, bc, bz);
            push_player(w, id, push);
            let hurt_lately = w.resource::<PlayerState>().hurt_by.get(id as usize).copied().flatten().is_some_and(|(_, at)| at + 2 >= now);
            if !sitting && !hurt_lately {
                hurt_player(w, id, WING_DAMAGE, e);
            }
        }
        for c in creatures_in(w, zone, e) {
            let (x, z) = creature_xz(w, c);
            push_creature(w, c, rule::push_from(x, z, bc, bz));
            if !sitting {
                hurt_creature(w, c, e, WING_DAMAGE);
            }
        }
    }
    for zone in heads {
        for id in players_in(w, zone) {
            if !w.resource::<PlayerState>().creative(id) && !w.resource::<PlayerState>().spectator(id) {
                hurt_player(w, id, HEAD_DAMAGE, e);
            }
        }
        for c in creatures_in(w, zone, e) {
            hurt_creature(w, c, e, HEAD_DAMAGE);
        }
    }
}
