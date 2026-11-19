//! The status effects capability (`MobEffectInstance`): a creature with an `Effects` component has a list of effects (a kind, a level, an end),
//! added and removed by events (`add`, `remove`). Nothing ticks per carrier: the periodic effects (poison, wither, regeneration) and the ends are
//! deadlines of the wheel (`Due::Effect`), one per creature at its nearest. What an effect changes: the speed (speed, slowness: `Body.speed`),
//! the attack (weakness: `AttackDamage`), the immunities (fire resistance, water breathing: `Immune`), the marker `Invisible` (read by the
//! targeting), the levitation (a field of `Drive` that the ground travel reads: no system). The count of a periodic effect is the `Mob`'s: with `D` ticks left,
//! it acts on the ticks where the remaining time is a multiple of `base >> level`. Not done: the chains of hidden weaker effects, the
//! instant effects, absorption and health boost, the undead that poison does not hurt, the particles and the icons.
use crate::cow::*;
use crate::goals::Events;
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Speed,
    Slowness,
    Weakness,
    Poison,
    Wither,
    Regeneration,
    FireResistance,
    WaterBreathing,
    Invisibility,
    Levitation,
    /// Hunger: nothing for a creature (it is a player's).
    Hunger,
}
impl Kind {
    /// The period of a periodic effect at level 0 (`MobEffect.shouldApplyEffectTickThisTick`).
    fn base(self) -> Option<i32> {
        match self {
            Kind::Poison => Some(25),
            Kind::Wither => Some(40),
            Kind::Regeneration => Some(50),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Effect {
    pub kind: Kind,
    pub amp: i32,
    /// The last tick of the effect: it acts on it if its period falls there, and is gone after the tick's acts.
    pub ends: u64,
    /// The next tick a periodic effect acts on.
    pub next: u64,
}
#[derive(Component, Clone, Debug, Default)]
pub struct Effects {
    pub list: Vec<Effect>,
    base_speed: Option<f64>,
    base_attack: Option<f32>,
    /// The `Immune` bits the effects added.
    granted: u32,
    /// The tick of the wheel entry that is pending (0: none).
    armed: u64,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Invisible;

/// Fire resistance: no fire damage (`Immune` bit read by the burn).
pub const IMMUNE_FIRE: u32 = 8;

fn period(kind: Kind, amp: i32) -> Option<i32> {
    kind.base().map(|b| b.wrapping_shr(amp as u32).max(1))
}
pub fn has(w: &World, e: Entity, kind: Kind) -> bool {
    w.get::<Effects>(e).is_some_and(|f| f.list.iter().any(|x| x.kind == kind))
}
/// `LivingEntity.addEffect`: a new effect, or one that replaces the same kind when it is of a higher level or of the same level and longer.
pub fn add(w: &mut World, e: Entity, kind: Kind, amp: i32, ticks: i32) -> bool {
    if w.get::<Health>(e).is_none() {
        return false;
    }
    let first = now_of(w, e) + 1;
    // `tickEffects` drops the effect at the end of its last tick, before the travel of that tick: it is gone from that tick on.
    let ends = first + (ticks.max(1) - 1) as u64;
    let next = period(kind, amp).map_or(u64::MAX, |p| first + (ticks.max(0) as u64) % p as u64);
    if w.get::<Effects>(e).is_none() {
        w.entity_mut(e).insert(Effects::default());
    }
    let mut f = w.get_mut::<Effects>(e).unwrap();
    let new = Effect { kind, amp, ends, next };
    match f.list.iter().position(|x| x.kind == kind) {
        Some(i) => {
            let old = f.list[i];
            if amp > old.amp || amp == old.amp && ends > old.ends {
                f.list[i] = new;
            } else {
                return false;
            }
        }
        None => f.list.push(new),
    }
    modifiers(w, e);
    arm(w, e);
    w.resource_mut::<Events>().effect_changed.push(e);
    true
}
/// `LivingEntity.removeEffect`.
pub fn remove(w: &mut World, e: Entity, kind: Kind) -> bool {
    let Some(mut f) = w.get_mut::<Effects>(e) else { return false };
    let n = f.list.len();
    f.list.retain(|x| x.kind != kind);
    let removed = f.list.len() != n;
    if removed {
        modifiers(w, e);
        w.resource_mut::<Events>().effect_changed.push(e);
    }
    removed
}
/// What the list changes in the entity: its speed, its attack, its immunities, its markers.
fn modifiers(w: &mut World, e: Entity) {
    let Some(f) = w.get::<Effects>(e).cloned() else { return };
    let mut f = f;
    // The speed: the base is kept while some effect changes it, and put back after.
    let mut speed = 1.0_f64;
    for x in &f.list {
        match x.kind {
            Kind::Speed => speed *= 1.0 + 0.2 * f64::from(x.amp + 1),
            Kind::Slowness => speed *= (1.0 - 0.15 * f64::from(x.amp + 1)).max(0.0),
            _ => {}
        }
    }
    let changes = f.list.iter().any(|x| matches!(x.kind, Kind::Speed | Kind::Slowness));
    if let Some(mut b) = w.get_mut::<Body>(e) {
        if changes && f.base_speed.is_none() {
            f.base_speed = Some(b.speed);
        }
        if let Some(base) = f.base_speed {
            b.speed = base * speed;
            if !changes {
                f.base_speed = None;
            }
        }
    }
    let weak: f32 = f.list.iter().filter(|x| x.kind == Kind::Weakness).map(|x| 4.0 * (x.amp + 1) as f32).sum();
    if let Some(mut a) = w.get_mut::<AttackDamage>(e) {
        if weak > 0.0 && f.base_attack.is_none() {
            f.base_attack = Some(a.0);
        }
        if let Some(base) = f.base_attack {
            a.0 = (base - weak).max(0.0);
            if weak == 0.0 {
                f.base_attack = None;
            }
        }
    }
    if let Some(mut im) = w.get_mut::<Immune>(e) {
        let wants = (if f.list.iter().any(|x| x.kind == Kind::FireResistance) { IMMUNE_FIRE } else { 0 }) | (if f.list.iter().any(|x| x.kind == Kind::WaterBreathing) { BREATHES_WATER } else { 0 });
        let fresh = wants & !im.0;
        im.0 = (im.0 & !f.granted) | wants;
        f.granted = (f.granted & wants) | fresh;
    }
    let invisible = f.list.iter().any(|x| x.kind == Kind::Invisibility);
    let lev = f.list.iter().find(|x| x.kind == Kind::Levitation).map(|x| x.amp);
    let mut en = w.entity_mut(e);
    if invisible {
        en.insert(Invisible);
    } else {
        en.remove::<Invisible>();
    }
    match lev {
        Some(a) => {
            en.insert(Moving);
            if let Some(mut d) = w.get_mut::<Drive>(e) {
                d.levitation = a + 1;
            }
        }
        None => {
            if let Some(mut d) = w.get_mut::<Drive>(e) {
                d.levitation = 0;
            }
        }
    }
    *w.get_mut::<Effects>(e).unwrap() = f;
}
/// The nearest tick something happens: a periodic act or an end.
fn nearest(f: &Effects) -> Option<u64> {
    f.list.iter().map(|x| x.next.min(x.ends)).min()
}
/// Puts the creature's deadline in the wheel when none is pending or the nearest moved earlier.
fn arm(w: &mut World, e: Entity) {
    let now = now_of(w, e);
    let Some(f) = w.get::<Effects>(e) else { return };
    let Some(at) = nearest(f) else { return };
    let at = at.max(now + 1).min(now + 4000);
    if f.armed != 0 && f.armed <= at && f.armed > now {
        return;
    }
    w.get_mut::<Effects>(e).unwrap().armed = at;
    let when = w.resource::<Clock>().at(e, at);
    w.resource_mut::<Wheel>().push(when, e, Due::Effect);
}
/// The deadline: the effects that act now act, the ended ones go, the next deadline is armed.
pub fn due(w: &mut World, e: Entity) {
    let now = now_of(w, e);
    let Some(mut f) = w.get::<Effects>(e).cloned() else { return };
    if f.armed != now {
        return;
    }
    f.armed = 0;
    let max = w.get::<SpeciesId>(e).map_or(f32::MAX, |s| w.resource::<SpeciesTable>().0[s.0 as usize].max_health);
    let mut acts: Vec<(Kind, u64)> = Vec::new();
    for x in &mut f.list {
        if let Some(p) = period(x.kind, x.amp) {
            while x.next <= now && x.next <= x.ends {
                if x.next == now {
                    acts.push((x.kind, now));
                }
                x.next += p as u64;
            }
        }
    }
    let before = f.list.len();
    f.list.retain(|x| x.ends > now);
    let ended = f.list.len() != before;
    *w.get_mut::<Effects>(e).unwrap() = f;
    for (kind, _) in acts {
        match kind {
            Kind::Poison => {
                if w.get::<Health>(e).is_some_and(|h| h.0 > 1.0) && w.get::<Undead>(e).is_none() {
                    w.resource_mut::<DamageQueue>().add(e, 1.0, Cause::Magic);
                }
            }
            Kind::Wither => w.resource_mut::<DamageQueue>().add(e, 1.0, Cause::Wither),
            Kind::Regeneration => {
                if w.get::<Undead>(e).is_some() {
                    continue;
                }
                if let Some(mut h) = w.get_mut::<Health>(e) {
                    h.0 = (h.0 + 1.0).min(max);
                }
            }
            _ => {}
        }
    }
    if ended {
        modifiers(w, e);
        w.resource_mut::<Events>().effect_changed.push(e);
    }
    arm(w, e);
}

/// The effects a creature's blow puts on a creature it hits (`CaveSpider.doHurtTarget`, `WitherSkeleton`, `Husk`): (kind, level, ticks).
#[derive(Component, Clone, Copy, Debug)]
pub struct OnHit(pub &'static [(Kind, i32, i32)]);
/// The rule that gives it (`species` configuration: no code per species). Not done: a player target has no `Effects`.
pub static ON_HIT: crate::rules::Rule = crate::rules::Rule { name: "on_hit_effects", equip: equip_on_hit, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };
pub struct OnHitParams(pub &'static [(Kind, i32, i32)]);
fn equip_on_hit(w: &mut World, e: Entity, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(OnHit(crate::rules::params::<OnHitParams>(p).0));
}
/// The blow landed: the effects of `from` are put on `target` (a creature, or a player: the player's list is `PlayerEffects`).
pub fn hit_effects(w: &mut World, from: Entity, target: crate::targeting::Who) {
    let Some(OnHit(list)) = w.get::<OnHit>(from).copied() else { return };
    for &(kind, amp, ticks) in list {
        match target {
            crate::targeting::Who::Entity(t) => {
                add(w, t, kind, amp, ticks);
            }
            crate::targeting::Who::Player(id) => add_player(w, id, kind, amp, ticks),
        }
    }
}
/// What the creatures of some species put on what they hit, at the normal difficulty (`CaveSpider`, `Bee`, `WitherSkeleton`, `Husk`, a puffed
/// `Pufferfish`): the parameters of the rule `on_hit_effects`.
pub static CAVE_SPIDER: OnHitParams = OnHitParams(&[(Kind::Poison, 0, 140)]);
pub static BEE_STING: OnHitParams = OnHitParams(&[(Kind::Poison, 0, 200)]);
pub static WITHER_SKELETON: OnHitParams = OnHitParams(&[(Kind::Wither, 0, 200)]);
pub static HUSK: OnHitParams = OnHitParams(&[(Kind::Hunger, 0, 140)]);
pub static PUFFERFISH: OnHitParams = OnHitParams(&[(Kind::Poison, 1, 1200)]);
/// A creature that `EntityTypeTags.UNDEAD` names: poison and regeneration do nothing to it, harming heals it and healing hurts it.
#[derive(Component, Clone, Copy, Debug)]
pub struct Undead;
/// The effects of the players (they are not entities): a list per player, read at the tick it is asked (an effect has its end; nothing ticks).
/// The periodic damage and the speed of a player are the player's own system's; here the reading (a witch's choice, the sight a creature has
/// of an invisible player, the hunger a husk gives) and the writing.
#[derive(Resource, Default)]
pub struct PlayerEffects(pub std::collections::HashMap<u32, Vec<(Kind, i32, u64)>>);
pub fn add_player(w: &mut World, id: u32, kind: Kind, amp: i32, ticks: i32) {
    let now = w.resource::<Clock>().0;
    let ends = now + ticks.max(0) as u64;
    // Born at the first effect of a player (a resource registered at the world's creation costs `arbitrate` even when nothing reads it).
    let mut res = w.get_resource_or_insert_with(PlayerEffects::default);
    let list = res.0.entry(id).or_default();
    list.retain(|x| x.2 > now);
    match list.iter().position(|x| x.0 == kind) {
        Some(i) if amp > list[i].1 || amp == list[i].1 && ends > list[i].2 => list[i] = (kind, amp, ends),
        Some(_) => {}
        None => list.push((kind, amp, ends)),
    }
}
pub fn player_has(w: &World, id: u32, kind: Kind) -> bool {
    let now = w.resource::<Clock>().0;
    w.get_resource::<PlayerEffects>().is_some_and(|p| p.0.get(&id).is_some_and(|l| l.iter().any(|x| x.0 == kind && x.2 > now)))
}
/// What a thrown potion carries (`PotionContents`): the effects it puts on what its splash reaches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Brew {
    /// A timed effect of this level and length (before the 0.75 and the nearness of a splash).
    Timed(Kind, i32, i32),
    /// `InstantenousMobEffect`: harming deals `6 * (level + 1)`, healing gives `4 * (level + 1)`, scaled by the nearness.
    Harm(i32),
    Heal(i32),
}
/// `ThrownSplashPotion.onHit` / `applySplash`: the creatures within 4 blocks (2 up and down) of `at` and nearer than 4 take the brews, scaled
/// by `1 - distance / 4` (the creature hit itself takes them whole); a timed effect lasts `(int)(scale * ticks * 0.75 + 0.5)`.
pub fn splash(w: &mut World, at: horizon_core::geometry::Vec3, hit: Option<Entity>, brews: &'static [Brew], owner: Option<crate::targeting::Who>) {
    let mut near: Vec<Entity> = Vec::new();
    {
        let idx = w.resource::<Index>();
        idx.for_radius(at.x, at.z, 4.0, |c| near.push(c));
    }
    let area = horizon_core::geometry::Aabb::new(at, at).inflate(horizon_core::geometry::Vec3::new(4.0, 2.0, 4.0));
    for c in near {
        let (Some(b), Some(p)) = (w.get::<Bounds>(c).map(|b| b.0), w.get::<Pos>(c).map(|p| p.0)) else { continue };
        if w.get::<Health>(c).is_none() || !b.intersects(area) {
            continue;
        }
        let d2 = (p.x - at.x).powi(2) + (p.y - at.y).powi(2) + (p.z - at.z).powi(2);
        if d2 >= 16.0 && hit != Some(c) {
            continue;
        }
        let scale = if hit == Some(c) { 1.0 } else { 1.0 - d2.sqrt() / 4.0 };
        for &brew in brews {
            match brew {
                Brew::Timed(kind, amp, ticks) => {
                    let t = (scale * f64::from(ticks) * 0.75 + 0.5) as i32;
                    if t > 0 {
                        add(w, c, kind, amp, t);
                    }
                }
                Brew::Harm(amp) | Brew::Heal(amp) => {
                    let undead = w.get::<Undead>(c).is_some();
                    let harms = matches!(brew, Brew::Harm(_)) != undead;
                    let base = if matches!(brew, Brew::Harm(_)) { 6 } else { 4 };
                    let amount = (scale * f64::from(base * (amp + 1)) + 0.5).floor() as f32;
                    if harms {
                        match owner {
                            Some(crate::targeting::Who::Entity(o)) => w.resource_mut::<DamageQueue>().add_by_entity(c, amount, (at.x, at.z), o),
                            _ => w.resource_mut::<DamageQueue>().add(c, amount, Cause::Magic),
                        }
                    } else {
                        let max = w.get::<SpeciesId>(c).map_or(f32::MAX, |s| w.resource::<SpeciesTable>().0[s.0 as usize].max_health);
                        if let Some(mut h) = w.get_mut::<Health>(c) {
                            h.0 = (h.0 + amount).min(max);
                        }
                    }
                }
            }
        }
    }
}
