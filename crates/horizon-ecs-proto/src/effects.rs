//! The status effects capability (`MobEffectInstance`): a creature with an `Effects` component has a list of effects (a kind, a level, an end),
//! added and removed by events (`add`, `remove`). Nothing ticks per carrier: the periodic effects (poison, wither, regeneration) and the ends are
//! deadlines of the wheel (`Due::Effect`), one per creature at its nearest. What an effect changes: the speed (speed, slowness: `Body.speed`),
//! the attack (weakness: `AttackDamage`), the immunities (fire resistance, water breathing: `Immune`), the marker `Invisible` (read by the
//! targeting), the levitation (a field of `Drive` that the ground travel reads: no system). The count of a periodic effect is the `Mob`'s: with `D` ticks left,
//! it acts on the ticks where the remaining time is a multiple of `base >> level`; the hidden weaker effects wait behind it (`Effects.hidden`).
//! Not done: the instant effects, absorption and health boost, the undead that poison does not hurt, the particles and the icons.
use crate::cow::*;
use crate::goals::Events;
use crate::rules::effects as rule;
use crate::type_tags;
use bevy_ecs::prelude::*;

/// The mods' answer for a creature's effect swirls (`effect_particle_modification`), with the effects it was computed for. The host writes it
/// when the effects change and clears it with `set_modified_swirls`; sparse, since most creatures have none.
#[derive(Component, Clone, Debug, PartialEq)]
#[component(storage = "SparseSet")]
pub struct ModifiedSwirls(pub myriade_level::effects::ModifiedEffectParticles);

/// Keeps `swirls` on `e`, or clears them with `None`.
pub fn set_modified_swirls(w: &mut World, e: Entity, swirls: Option<myriade_level::effects::ModifiedEffectParticles>) {
    match swirls {
        Some(s) => {
            w.entity_mut(e).insert(ModifiedSwirls(s));
        }
        None => {
            w.entity_mut(e).remove::<ModifiedSwirls>();
        }
    }
}

#[cfg(test)]
mod swirl_tests {
    use super::*;
    use myriade_level::effects::{ModifiedEffectParticles, ShownEffect};

    #[test]
    fn the_mods_swirls_stay_on_the_creature_until_they_are_cleared() {
        let mut w = World::new();
        let e = w.spawn_empty().id();
        let speed = ShownEffect { effect: "minecraft:speed", amplifier: 0, visible: true, ambient: false };
        let swirls = ModifiedEffectParticles::new([speed], vec![]);
        set_modified_swirls(&mut w, e, Some(swirls.clone()));
        assert_eq!(w.get::<ModifiedSwirls>(e).map(|s| s.0.clone()), Some(swirls.clone()));
        assert!(swirls.computed_for([speed]), "computed for the effect it shows");
        set_modified_swirls(&mut w, e, None);
        assert!(w.get::<ModifiedSwirls>(e).is_none());
    }
}

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
    /// `MobEffects.MINING_FATIGUE`: a player's (the elder guardian's curse).
    MiningFatigue,
    /// `MobEffects.BLINDNESS`: the illusioner's spell; nothing for a creature (the sight is the Mob's own, not ported).
    Blindness,
    /// `MobEffects.JUMP_BOOST`: the long jump's velocity grows by 0.1 x (level + 1) (`Frog`, `Goat`); not periodic.
    JumpBoost,
    /// `MobEffects.GLOWING` (`minecraft:glowing`): what a spectral arrow gives, at amplifier 0 and 200 ticks.
    Glowing,
    /// `MobEffects.WEAVING`: a cobweb it stands in slows it less (`WebBlock.entityInside`: 0.5, 0.25, 0.5).
    Weaving,
    /// `MobEffects.STRENGTH`: what a cure gives the zombie villager it starts (`startConverting`).
    Strength,
    /// `MobEffects.RESISTANCE`: the hurt takes `(amp + 1) * 5` of 25 off (`LivingEntity.getDamageAfterMagicAbsorb`), see `Resistance`.
    Resistance,
    /// `MobEffects.INFESTED`: a hit on the bearer may let silverfish out (`hurt_spawn`).
    Infested,
    /// `MobEffects.ABSORPTION`: adding it raises the absorption to 4 x (level + 1) at least; ending it takes that back.
    Absorption,
    /// `MobEffects.HEALTH_BOOST`: +4 max health per level on `max_health` (`attribute_modifiers`), taken back when it ends.
    HealthBoost,
    /// `MobEffects.CONDUIT_POWER`: the water breathing of the creature (`fluids::breathing`).
    ConduitPower,
    /// `MobEffects.BREATH_OF_THE_NAUTILUS`: the air under water is held, not refilled (`fluids::breathing`).
    BreathOfTheNautilus,
    /// `MobEffects.SLOW_FALLING`: the fall distance is discarded before each travel (`fall_reset`).
    SlowFalling,
    /// `MobEffects.HASTE`: stored, counted down, sent and saved; no server-side action.
    Haste,
    /// `MobEffects.NAUSEA`: stored, counted down, sent and saved; no server-side action.
    Nausea,
    /// `MobEffects.NIGHT_VISION`: stored, counted down, sent and saved; no server-side action.
    NightVision,
    /// `MobEffects.SATURATION`: stored, counted down, sent and saved; no server-side action.
    Saturation,
    /// `MobEffects.LUCK`: stored, counted down, sent and saved; no server-side action.
    Luck,
    /// `MobEffects.UNLUCK`: stored, counted down, sent and saved; no server-side action.
    Unluck,
    /// `MobEffects.DOLPHINS_GRACE`: stored, counted down, sent and saved; no server-side action.
    DolphinsGrace,
    /// `MobEffects.BAD_OMEN`: stored, counted down, sent and saved; no server-side action.
    BadOmen,
    /// `MobEffects.HERO_OF_THE_VILLAGE`: stored, counted down, sent and saved; no server-side action.
    HeroOfTheVillage,
    /// `MobEffects.DARKNESS`: stored, counted down, sent and saved; no server-side action.
    Darkness,
    /// `MobEffects.TRIAL_OMEN`: stored, counted down, sent and saved; no server-side action.
    TrialOmen,
    /// `MobEffects.RAID_OMEN`: stored, counted down, sent and saved; no server-side action.
    RaidOmen,
    /// `MobEffects.WIND_CHARGED`: stored, counted down, sent and saved; no server-side action.
    WindCharged,
    /// `MobEffects.OOZING`: stored, counted down, sent and saved; no server-side action.
    Oozing,
    /// `MobEffects.INSTANT_HEALTH`: acts on each tick of its duration (1 tick for the usual one); the undead are hurt by it.
    InstantHealth,
    /// `MobEffects.INSTANT_DAMAGE`: as `InstantHealth`, healing the undead.
    InstantDamage,
}
impl Kind {
    /// The period of a periodic effect at level 0 (`MobEffect.shouldApplyEffectTickThisTick`).
    fn base(self) -> Option<i32> {
        match self {
            Kind::Poison => Some(25),
            Kind::Wither => Some(40),
            Kind::Regeneration => Some(50),
            Kind::InstantHealth | Kind::InstantDamage => Some(1),
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
    /// `HIDDEN` and `AMBIENT`: what the effect's source set (the byte fits in the padding after `kind`).
    pub flags: u8,
}
/// The effect's particles are not shown (`show_particles` 0).
pub const HIDDEN: u8 = 1;
/// The effect is ambient (from a beacon or a source that sets it).
pub const AMBIENT: u8 = 2;
/// The effect's icon is not shown (`show_icon` false).
pub const NO_ICON: u8 = 4;
#[derive(Component, Clone, Debug, Default)]
pub struct Effects {
    pub list: Vec<Effect>,
    base_attack: Option<f32>,
    /// The `Immune` bits the effects added.
    granted: u32,
    /// The tick of the wheel entry that is pending (0: none).
    armed: u64,
    /// The hidden instances (`MobEffectInstance.hiddenEffect`), per kind in chain order; their `ends` is the time left, frozen.
    pub hidden: Vec<Effect>,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet", on_insert = crate::bridge::view::touched, on_remove = crate::bridge::view::touched)]
pub struct Invisible;
/// The level of the Resistance effect (`amp + 1`), sparse: absent when the creature has no Resistance.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[component(storage = "SparseSet")]
pub struct Resistance(pub i32);

/// The breath of the nautilus: under water the air is held (no refill, no drowning) unless water breathing or conduit power is there.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct HoldsAir;

/// Fire resistance: no fire damage (`Immune` bit read by the burn).
pub const IMMUNE_FIRE: u32 = 8;
/// `canBeAffected` false for `WITHER` (the wither and the wither skeleton): a base bit of `Immune`, set at creation.
pub const IMMUNE_WITHER: u32 = 16;
/// `canBeAffected` false for `POISON` (`Spider`, `CaveSpider`, `AbstractNautilus`) and for `WEAKNESS` (`Parched`): base bits, set at creation.
pub const IMMUNE_POISON: u32 = 32;
pub const IMMUNE_WEAKNESS: u32 = 64;
/// `canFreeze` false (the powder snow adds no `frozenTicks`): a base bit of `Immune`, set at creation.
pub const IMMUNE_FREEZE: u32 = 128;
/// The base `Immune` bit that makes `canBeAffected` refuse the effect `kind` (0: none).
fn refusing_bit(kind: Kind) -> u32 {
    match kind {
        Kind::Wither => IMMUNE_WITHER,
        Kind::Poison => IMMUNE_POISON,
        Kind::Weakness => IMMUNE_WEAKNESS,
        _ => 0,
    }
}

fn period(kind: Kind, amp: i32) -> Option<i32> {
    kind.base().map(|b| rule::period(b, amp))
}
pub fn has(w: &World, e: Entity, kind: Kind) -> bool {
    w.get::<Effects>(e).is_some_and(|f| f.list.iter().any(|x| x.kind == kind))
}
/// `LivingEntity.addEffect`: a new effect, or one that replaces the same kind when it is of a higher level or of the same level and longer.
pub fn add(w: &mut World, e: Entity, kind: Kind, amp: i32, ticks: i32) -> bool {
    add_flagged(w, e, kind, amp, ticks, 0)
}
/// `add` with the `HIDDEN` and `AMBIENT` flags of the effect.
pub fn add_flagged(w: &mut World, e: Entity, kind: Kind, amp: i32, ticks: i32, flags: u8) -> bool {
    // `LivingEntity.canBeAffected`: the tag `ignores_poison_and_regen` refuses poison and regeneration; the species' own refusals are `Immune` bits.
    let tagged = matches!(kind, Kind::Poison | Kind::Regeneration) && type_tags::is::<IgnoresPoisonAndRegen>(w, e);
    if w.get::<Health>(e).is_none() || tagged || w.get::<Immune>(e).is_some_and(|i| i.has(refusing_bit(kind))) {
        return false;
    }
    if kind == Kind::Absorption {
        let cur = w.get::<crate::absorption::Absorption>(e).map_or(0.0, |a| a.0);
        // The effect's own modifier on `max_absorption` is not in the list yet: the cap is at least what it gives.
        crate::absorption::set_with_cap(w, e, cur.max(4.0 * (amp + 1) as f32), 4.0 * (amp + 1) as f32);
    }
    let first = now_of(w, e) + 1;
    // `tickEffects` drops the effect at the end of its last tick, before the travel of that tick: it is gone from that tick on.
    // A negative duration is infinite (`MobEffectInstance.INFINITE_DURATION`): it never ends.
    let ends = if ticks < 0 { u64::MAX } else { rule::ends_at(first, ticks) };
    let next = period(kind, amp).map_or(u64::MAX, |p| rule::first_act(first, ticks, p));
    if w.get::<Effects>(e).is_none() {
        w.entity_mut(e).insert(Effects::default());
    }
    let now = first - 1;
    if ticks == 0 {
        // `tickServer` drops a zero duration before applying it, with its hidden part: that part is never promoted.
        let mut f = w.get_mut::<Effects>(e).unwrap();
        if f.list.iter().find(|x| x.kind == kind).is_none_or(|x| x.amp < amp) {
            f.list.retain(|x| x.kind != kind);
            f.hidden.retain(|x| x.kind != kind);
            drop(f);
            modifiers(w, e);
            arm(w, e);
            w.resource_mut::<Events>().effect_changed.push(e);
        }
        return true;
    }
    let mut f = w.get_mut::<Effects>(e).unwrap();
    let new = Effect { kind, amp, ends, next, flags };
    match f.list.iter().position(|x| x.kind == kind) {
        Some(i) => {
            let mut chain = take_hidden_chain(&mut f.hidden, kind, now);
            let mut cur = f.list[i];
            let mut changed = update(&mut cur, &mut chain, new);
            if !changed && cur.flags != flags {
                cur.flags = flags;
                changed = true;
            }
            f.list[i] = cur;
            f.hidden.extend(chain.into_iter().map(|x| Effect { ends: x.ends.saturating_sub(now), ..x }));
            if !changed {
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
/// The hidden instances of `kind`, in chain order, their time left made absolute from `now`.
fn take_hidden_chain(hidden: &mut Vec<Effect>, kind: Kind, now: u64) -> Vec<Effect> {
    let mut chain = Vec::new();
    hidden.retain(|x| {
        if x.kind == kind {
            chain.push(Effect { ends: x.ends.saturating_add(now), ..*x });
            false
        } else {
            true
        }
    });
    chain
}
/// `MobEffectInstance.update`: `other` (the new effect) meets the instance `cur`, whose hidden chain is `chain`. A stronger `other` takes the
/// place of `cur`, which is hidden under it when `other` is shorter. An equal or weaker `other` changes the duration of `cur` when they are equal
/// in level and `cur` is shorter, else it is hidden under `cur` (when `cur` is shorter). Returns whether `cur` changed.
fn update(cur: &mut Effect, chain: &mut Vec<Effect>, other: Effect) -> bool {
    if other.amp > cur.amp {
        if other.ends < cur.ends {
            chain.insert(0, *cur);
        }
        *cur = other;
        return true;
    }
    if cur.ends >= other.ends {
        return false;
    }
    if cur.amp == other.amp {
        cur.ends = other.ends;
        cur.next = other.next;
        return true;
    }
    if chain.is_empty() {
        chain.push(other);
    } else {
        let mut head = chain.remove(0);
        update(&mut head, chain, other);
        chain.insert(0, head);
    }
    false
}
/// `downgradeToHiddenEffect`: an ended effect gives way to the hidden one of its kind, which runs for the time it had left.
fn promote(hidden: &mut Vec<Effect>, ended: Effect, now: u64) -> Option<Effect> {
    let i = hidden.iter().position(|x| x.kind == ended.kind)?;
    let h = hidden.remove(i);
    let next = period(h.kind, h.amp).map_or(u64::MAX, |p| rule::first_act(now + 1, h.ends as i32, p));
    Some(Effect { ends: now.saturating_add(h.ends), next, ..h })
}
/// `LivingEntity.removeEffect`.
pub fn remove(w: &mut World, e: Entity, kind: Kind) -> bool {
    let Some(mut f) = w.get_mut::<Effects>(e) else { return false };
    let n = f.list.len();
    f.list.retain(|x| x.kind != kind);
    f.hidden.retain(|x| x.kind != kind);
    let removed = f.list.len() != n;
    if removed {
        modifiers(w, e);
        w.resource_mut::<Events>().effect_changed.push(e);
    }
    removed
}
/// The attribute modifiers the effects of `e` give to `name` (without `minecraft:`), derived from the list on read (`MobEffect.addAttributeModifiers`,
/// id `minecraft:effect.<effect>`, amount scaled by `amp + 1`): the same amounts `modifiers` applies to `Body.speed` and `AttackDamage`.
pub fn attribute_modifiers(w: &World, e: Entity, name: &str) -> Vec<crate::attr_mod::Modifier> {
    use crate::attr_mod::{Modifier, Operation};
    let Some(f) = w.get::<Effects>(e) else { return Vec::new() };
    let make = |id: &str, amount: f64, operation| Modifier { id: id.to_owned(), amount, operation, permanent: false };
    f.list
        .iter()
        .filter_map(|x| {
            let n = f64::from(x.amp + 1);
            match (name, x.kind) {
                ("movement_speed", Kind::Speed) => Some(make("minecraft:effect.speed", f64::from(0.2f32) * n, Operation::AddMultipliedTotal)),
                ("movement_speed", Kind::Slowness) => Some(make("minecraft:effect.slowness", f64::from(-0.15f32) * n, Operation::AddMultipliedTotal)),
                ("attack_damage", Kind::Weakness) => Some(make("minecraft:effect.weakness", -4.0 * n, Operation::AddValue)),
                ("attack_damage", Kind::Strength) => Some(make("minecraft:effect.strength", 3.0 * n, Operation::AddValue)),
                ("max_health", Kind::HealthBoost) => Some(make("minecraft:effect.health_boost", 4.0 * n, Operation::AddValue)),
                ("max_absorption", Kind::Absorption) => Some(make("minecraft:effect.absorption", 4.0 * n, Operation::AddValue)),
                _ => None,
            }
        })
        .collect()
}
/// The attributes whose modifiers the effects of `e` give.
pub fn attribute_names(w: &World, e: Entity) -> impl Iterator<Item = &'static str> + '_ {
    let list = w.get::<Effects>(e).map_or(&[][..], |f| &f.list[..]);
    list.iter().filter_map(|x| match x.kind {
        Kind::Speed | Kind::Slowness => Some("movement_speed"),
        Kind::Weakness | Kind::Strength => Some("attack_damage"),
        Kind::HealthBoost => Some("max_health"),
        Kind::Absorption => Some("max_absorption"),
        _ => None,
    })
}
/// What the list changes in the entity: its speed, its attack, its immunities, its markers.
fn modifiers(w: &mut World, e: Entity) {
    let Some(f) = w.get::<Effects>(e).cloned() else { return };
    let mut f = f;
    // The maximum health follows the Health Boost, added or taken back (`LivingEntity.onAttributeUpdated`).
    crate::attr_mod::touched(w, e, "max_health");
    // The speed is the `movement_speed` attribute, its effect modifiers included (`attribute_modifiers`): one path, in `touched`.
    crate::attr_mod::touched(w, e, "movement_speed");
    let weak: f32 = f.list.iter().filter(|x| x.kind == Kind::Weakness).map(|x| 4.0 * (x.amp + 1) as f32).sum::<f32>()
        - f.list.iter().filter(|x| x.kind == Kind::Strength).map(|x| 3.0 * (x.amp + 1) as f32).sum::<f32>();
    if let Some(mut a) = w.get_mut::<AttackDamage>(e) {
        if weak != 0.0 && f.base_attack.is_none() {
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
        let wants = (if f.list.iter().any(|x| x.kind == Kind::FireResistance) { IMMUNE_FIRE } else { 0 }) | (if f.list.iter().any(|x| matches!(x.kind, Kind::WaterBreathing | Kind::ConduitPower)) { BREATHES_WATER } else { 0 });
        let fresh = wants & !im.0;
        im.0 = (im.0 & !f.granted) | wants;
        f.granted = (f.granted & wants) | fresh;
    }
    let invisible = f.list.iter().any(|x| x.kind == Kind::Invisibility);
    let lev = f.list.iter().find(|x| x.kind == Kind::Levitation).map(|x| x.amp);
    let slow = f.list.iter().any(|x| x.kind == Kind::SlowFalling);
    let resist = f.list.iter().find(|x| x.kind == Kind::Resistance).map(|x| x.amp + 1);
    let mut en = w.entity_mut(e);
    if invisible {
        en.insert(Invisible);
    } else {
        en.remove::<Invisible>();
    }
    match resist {
        Some(n) => {
            en.insert(Resistance(n));
        }
        None => {
            en.remove::<Resistance>();
        }
    }
    if f.list.iter().any(|x| x.kind == Kind::BreathOfTheNautilus) {
        en.insert(HoldsAir);
    } else {
        en.remove::<HoldsAir>();
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
    if let Some(mut d) = w.get_mut::<Drive>(e) {
        d.slow_falling = slow;
    }
    *w.get_mut::<Effects>(e).unwrap() = f;
    w.get_resource_or_insert_with(crate::attr_mod::AttrDirty::default).0.insert(e);
    // The ambient flags are read by the synchronized data: every change of the list goes through here.
    crate::bridge::view::mark(w, e);
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
    let mut acts: Vec<(Kind, i32)> = Vec::new();
    for x in &mut f.list {
        if let Some(p) = period(x.kind, x.amp) {
            let (next, hit) = rule::settle(x.next, x.ends, now, p as u64);
            x.next = next;
            if hit {
                acts.push((x.kind, x.amp));
            }
        }
    }
    let absorbed: Vec<i32> = f.list.iter().filter(|x| x.ends <= now && x.kind == Kind::Absorption).map(|x| x.amp).collect();
    let ended = f.list.iter().any(|x| x.ends <= now);
    let hidden = &mut f.hidden;
    let mut promoted: Vec<i32> = Vec::new();
    f.list = std::mem::take(&mut f.list).into_iter().filter_map(|x| if x.ends > now { Some(x) } else { promote(hidden, x, now).inspect(|p| if p.kind == Kind::Absorption { promoted.push(p.amp) }) }).collect();
    *w.get_mut::<Effects>(e).unwrap() = f;
    for amp in absorbed {
        let cur = w.get::<crate::absorption::Absorption>(e).map_or(0.0, |a| a.0);
        crate::absorption::set(w, e, (cur - 4.0 * (amp + 1) as f32).max(0.0));
    }
    // A hidden absorption that takes over starts again at its own level (`onEffectStarted`).
    for amp in promoted {
        let cur = w.get::<crate::absorption::Absorption>(e).map_or(0.0, |a| a.0);
        crate::absorption::set(w, e, cur.max(4.0 * (amp + 1) as f32));
    }
    for (kind, amp) in acts {
        match kind {
            Kind::Poison => {
                if w.get::<Health>(e).is_some_and(|h| h.0 > 1.0) && !type_tags::is::<IgnoresPoisonAndRegen>(w, e) {
                    w.resource_mut::<DamageQueue>().add(e, 1.0, Cause::Magic);
                }
            }
            Kind::Wither => w.resource_mut::<DamageQueue>().add(e, 1.0, Cause::Wither),
            Kind::InstantHealth | Kind::InstantDamage => {
                // `InstantenousMobEffect.applyEffectTick`: healing gives `4 << amp`, harming hurts `6 << amp`; the undead take the other.
                let undead = type_tags::is::<InvertedHealingAndHarm>(w, e);
                let harm_kind = kind == Kind::InstantDamage;
                let amount = rule::harm_amount(1.0, if harm_kind { 6 } else { 4 }, amp);
                if rule::harms(harm_kind, undead) {
                    w.resource_mut::<DamageQueue>().add(e, amount, Cause::Magic);
                } else {
                    if let Some(mut h) = w.get_mut::<Health>(e) {
                        h.0 = (h.0 + amount).min(max);
                    }
                    crate::bridge::view::mark(w, e);
                }
            }
            Kind::Regeneration => {
                if type_tags::is::<IgnoresPoisonAndRegen>(w, e) {
                    continue;
                }
                if let Some(mut h) = w.get_mut::<Health>(e) {
                    h.0 = (h.0 + 1.0).min(max);
                }
                crate::bridge::view::mark(w, e);
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

/// The level's difficulty id (0 peaceful, 1 easy, 2 normal, 3 hard), written by the host; absent: normal (what the poison of a sting reads).
#[derive(Resource, Clone, Copy, Debug)]
pub struct Difficulty(pub u8);
pub fn difficulty(w: &World) -> u8 {
    w.get_resource::<Difficulty>().map_or(2, |d| d.0)
}
/// The effects a creature's blow puts on a creature it hits (`CaveSpider.doHurtTarget`, `WitherSkeleton`, `Husk`): (kind, level, ticks).
#[derive(Component, Clone, Copy, Debug)]
pub struct OnHit(pub &'static [(Kind, i32, Span)]);
/// How long an effect of a blow lasts, from the level's difficulty (`CaveSpider.doHurtTarget`, `Husk.doHurtTarget`).
#[derive(Clone, Copy, Debug)]
pub enum Span {
    Ticks(i32),
    /// Ticks at peaceful, easy, normal, hard; 0 puts no effect.
    ByDifficulty([i32; 4]),
    /// Ticks per whole point of the effective difficulty (`(int) getEffectiveDifficulty()`).
    PerEffective(i32),
    /// `PerEffective`, only while the attacker's main hand is empty (`Husk.doHurtTarget`).
    PerEffectiveBare(i32),
}
/// `getCurrentDifficultyAt(blockPosition()).getEffectiveDifficulty()` at `e`, read from the level's world (the chunk's inhabited time, the
/// moon, the day) when a blow lands; nothing is stored. Without a level: the formula's floor, `base * 0.75`.
pub fn effective_difficulty(w: &World, e: Entity) -> f32 {
    let at = w.get::<Pos>(e).map(|p| [p.0.x, p.0.y, p.0.z]);
    match (w.get_resource::<Level>(), at) {
        (Some(level), Some(at)) => myriade_engine_lib::mobs::spawning::LocalDifficulty::at(level.mw.0.world, at).effective,
        _ => if difficulty(w) == 0 { 0.0 } else { f32::from(difficulty(w)) * 0.75 },
    }
}
/// `getCurrentDifficultyAt(blockPosition()).getSpecialMultiplier()` at `e`, read like `effective_difficulty`; without a level, the host's last `Special`.
pub fn special_difficulty(w: &World, e: Entity) -> f32 {
    let at = w.get::<Pos>(e).map(|p| [p.0.x, p.0.y, p.0.z]);
    match (w.get_resource::<Level>(), at) {
        (Some(level), Some(at)) => myriade_engine_lib::mobs::spawning::LocalDifficulty::at(level.mw.0.world, at).special(),
        _ => w.get_resource::<crate::spawn_finalize::Special>().map_or(0.0, |s| s.0),
    }
}
impl Span {
    /// The ticks of this span for a blow of `from`.
    pub fn ticks(self, w: &World, from: Entity) -> i32 {
        match self {
            Span::Ticks(t) => t,
            Span::ByDifficulty(t) => t[usize::from(difficulty(w).min(3))],
            Span::PerEffective(t) => t * effective_difficulty(w, from) as i32,
            Span::PerEffectiveBare(t) => {
                if crate::equip_stack::get(w, from, 0).is_some() { 0 } else { t * effective_difficulty(w, from) as i32 }
            }
        }
    }
}
/// The rule that gives it (`species` configuration: no code per species). Not done: a player target has no `Effects`.
pub static ON_HIT: crate::rules::Rule = crate::rules::Rule { name: "on_hit_effects", equip: equip_on_hit, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };
pub struct OnHitParams(pub &'static [(Kind, i32, Span)]);
fn equip_on_hit(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(OnHit(crate::rules::params::<OnHitParams>(p).0));
}
/// The blow landed: the effects of `from` are put on `target` (a creature, or a player: the player's list is `PlayerEffects`).
pub fn hit_effects(w: &mut World, from: Entity, target: crate::targeting::Who) {
    let Some(OnHit(list)) = w.get::<OnHit>(from).copied() else { return };
    for &(kind, amp, span) in list {
        let ticks = span.ticks(w, from);
        if ticks <= 0 {
            continue;
        }
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
pub static CAVE_SPIDER: OnHitParams = OnHitParams(&[(Kind::Poison, 0, Span::ByDifficulty([0, 0, 140, 300]))]);
pub static BEE_STING: OnHitParams = OnHitParams(&[(Kind::Poison, 0, Span::Ticks(200))]);
pub static WITHER_SKELETON: OnHitParams = OnHitParams(&[(Kind::Wither, 0, Span::Ticks(200))]);
pub static HUSK: OnHitParams = OnHitParams(&[(Kind::Hunger, 0, Span::PerEffectiveBare(140))]);
pub static PUFFERFISH: OnHitParams = OnHitParams(&[(Kind::Poison, 1, Span::Ticks(1200))]);
/// The entity type tags the effects read, as typed markers: poison and regeneration do nothing to a creature `ignores_poison_and_regen`
/// names; harming heals and healing hurts one `inverted_healing_and_harm` names (`potions.rs`).
#[derive(Component, Clone, Copy, Debug)]
pub struct IgnoresPoisonAndRegen;
#[derive(Component, Clone, Copy, Debug)]
pub struct InvertedHealingAndHarm;
/// The entity type tag `can_breathe_under_water`, read by the breath (`fluids::breathing`): a creature with it does not drown.
#[derive(Component, Clone, Copy, Debug)]
pub struct CanBreatheUnderWater;
/// The typed tags of the effects and the breath, for the world's builder (`type_tags::register`).
pub static TYPED_TAGS: &[type_tags::Typed] = &[
    type_tags::typed::<IgnoresPoisonAndRegen>("ignores_poison_and_regen"),
    type_tags::typed::<InvertedHealingAndHarm>("inverted_healing_and_harm"),
    type_tags::typed::<CanBreatheUnderWater>("can_breathe_under_water"),
];
/// The effects of the players (they are not entities): a list per player, read at the tick it is asked (an effect has its end; nothing ticks).
/// The periodic damage and the speed of a player are the player's own system's; here the reading (a witch's choice, the sight a creature has
/// of an invisible player, the hunger a husk gives) and the writing.
#[derive(Resource, Default)]
pub struct PlayerEffects(pub std::collections::HashMap<u32, Vec<(Kind, i32, u64)>>);
pub fn add_player(w: &mut World, id: u32, kind: Kind, amp: i32, ticks: i32) {
    add_player_flagged(w, id, kind, amp, ticks, 0);
}
/// `add_player` with the `HIDDEN` and `AMBIENT` flags; they are kept in `PlayerEffectFlags`, only where one is set.
pub fn add_player_flagged(w: &mut World, id: u32, kind: Kind, amp: i32, ticks: i32, flags: u8) {
    let now = w.resource::<Clock>().0;
    let ends = now + ticks.max(0) as u64;
    // Born at the first effect of a player (a resource registered at the world's creation costs `arbitrate` even when nothing reads it).
    let mut res = w.get_resource_or_insert_with(PlayerEffects::default);
    let list = res.0.entry(id).or_default();
    list.retain(|x| x.2 > now);
    let replaced = match list.iter().position(|x| x.0 == kind) {
        Some(i) if rule::replaces(amp, ends, list[i].1, list[i].2) => {
            list[i] = (kind, amp, ends);
            true
        }
        Some(_) => false,
        None => {
            list.push((kind, amp, ends));
            true
        }
    };
    drop(res);
    if replaced {
        if flags == 0 {
            if let Some(mut f) = w.get_resource_mut::<PlayerEffectFlags>() {
                f.0.remove(&(id, kind as u8));
            }
        } else {
            w.get_resource_or_insert_with(PlayerEffectFlags::default).0.insert((id, kind as u8), flags);
        }
    }
}
/// The flags of the players' effects, by (player, kind): only the effects that have one.
#[derive(Resource, Default)]
pub struct PlayerEffectFlags(pub std::collections::HashMap<(u32, u8), u8>);
pub fn player_has(w: &World, id: u32, kind: Kind) -> bool {
    let now = w.resource::<Clock>().0;
    w.get_resource::<PlayerEffects>().is_some_and(|p| p.0.get(&id).is_some_and(|l| l.iter().any(|x| x.0 == kind && x.2 > now)))
}
/// What a thrown potion carries (`PotionContents`): the effects it puts on what its splash reaches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Brew {
    /// A timed effect of this level and length (before the 0.75 and the nearness of a splash).
    Timed(Kind, i32, i32),
    /// A custom effect of a potion item, with its `HIDDEN` and `AMBIENT` flags (only the splash and the save read them).
    Flagged(Kind, i32, i32, u8),
    /// `InstantenousMobEffect`: harming deals `6 << level`, healing gives `4 << level` (the jar shifts, it does not multiply), scaled by the nearness.
    Harm(i32),
    Heal(i32),
}
/// The base potions (level I, `Potions`): id and brews. Slowness 1 800 ticks, poison 900, weakness 1 800, harming instant.
pub const POTIONS: &[(&str, &[Brew])] = &[
    ("minecraft:harming", &[Brew::Harm(0)]),
    ("minecraft:slowness", &[Brew::Timed(Kind::Slowness, 0, 1800)]),
    ("minecraft:poison", &[Brew::Timed(Kind::Poison, 0, 900)]),
    ("minecraft:weakness", &[Brew::Timed(Kind::Weakness, 0, 1800)]),
];
pub fn potion_brews(id: &str) -> Option<&'static [Brew]> {
    POTIONS.iter().find(|(n, _)| *n == id).map(|(_, b)| *b)
}
/// The table's name of a potion (`PotionType` key), when the table knows it.
pub fn potion_name(name: &str) -> Option<&'static str> {
    POTIONS.iter().find(|(n, _)| *n == name).map(|(n, _)| *n)
}

/// The mob effect of a jar id (`minecraft:speed`…), when the cloud and the splash know it.
pub fn kind_of(name: &str) -> Option<Kind> {
    crate::nbt_fields::effect_cloud::KINDS.iter().find(|(n, _)| *n == name).map(|(_, k)| *k)
}

/// The jar's `custom_effects` of a potion item, one timed brew each; an id the mob effects do not know is skipped.
pub fn custom_brews(stack: &myriade_engine_lib::items::ItemStack) -> Vec<Brew> {
    let Some((_, horizon_core::nbt::Tag::List(list))) = stack.data().and_then(|d| d.potion_extra.iter().find(|(k, _)| k == "custom_effects")) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|e| {
            let Some(horizon_core::nbt::Tag::String(id)) = e.get("id") else { return None };
            let id = id.to_string_lossy();
            let kind = crate::nbt_fields::effect_cloud::KINDS.iter().find(|(n, _)| *n == id.as_str())?.1;
            let int = |k: &str| match e.get(k) {
                Some(horizon_core::nbt::Tag::Int(v)) => *v,
                _ => 0,
            };
            let ambient = matches!(e.get("ambient"), Some(horizon_core::nbt::Tag::Byte(v)) if *v != 0);
            let hidden = matches!(e.get("show_particles"), Some(horizon_core::nbt::Tag::Byte(0)));
            let flags = if ambient { AMBIENT } else { 0 } | if hidden { HIDDEN } else { 0 };
            Some(Brew::Flagged(kind, int("amplifier").clamp(0, 255), int("duration"), flags))
        })
        .collect()
}

/// All the brews a potion item gives: its named potion's, then its custom effects.
pub fn contents_brews(stack: &myriade_engine_lib::items::ItemStack) -> Vec<Brew> {
    let mut brews = stack.potion().and_then(potion_brews).map_or_else(Vec::new, <[Brew]>::to_vec);
    brews.extend(custom_brews(stack));
    brews
}

/// The `custom_color` of a potion item, when it has one.
pub fn custom_color(stack: &myriade_engine_lib::items::ItemStack) -> Option<i32> {
    stack.data()?.potion_extra.iter().find(|(k, _)| k == "custom_color").and_then(|(_, t)| match t {
        horizon_core::nbt::Tag::Int(c) => Some(*c),
        _ => None,
    })
}
/// `ThrownSplashPotion.onHit` / `applySplash`: the creatures within 4 blocks (2 up and down) of `at` and nearer than 4 take the brews, scaled
/// by `1 - distance / 4` (the creature hit itself takes them whole); a timed effect lasts `(int)(scale * ticks * 0.75 + 0.5)`.
pub fn splash(w: &mut World, at: horizon_core::geometry::Vec3, hit: Option<Entity>, brews: &[Brew], owner: Option<crate::targeting::Who>) {
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
        let direct = hit == Some(c);
        if !rule::splash_reaches(d2, direct) {
            continue;
        }
        let scale = rule::splash_scale(d2, direct);
        for &brew in brews {
            let flags = if let Brew::Flagged(.., flags) = brew { flags } else { 0 };
            match brew {
                Brew::Timed(kind, amp, ticks) | Brew::Flagged(kind, amp, ticks, _) => {
                    let t = rule::timed_ticks(scale, ticks);
                    if t > 0 {
                        add_flagged(w, c, kind, amp, t, flags);
                    }
                }
                Brew::Harm(amp) | Brew::Heal(amp) => {
                    let undead = type_tags::is::<InvertedHealingAndHarm>(w, c);
                    let harms = rule::harms(matches!(brew, Brew::Harm(_)), undead);
                    let base: i32 = if matches!(brew, Brew::Harm(_)) { 6 } else { 4 };
                    let amount = rule::harm_amount(scale, base, amp);
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
                        crate::bridge::view::mark(w, c);
                    }
                }
            }
        }
    }
}
