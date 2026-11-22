//! The keys of a potion cloud the entity keeps (`AreaEffectCloud.addAdditionalSaveData`, same keys as the jar): `Age`, `Duration`, `WaitTime`,
//! `ReapplicationDelay`, `DurationOnUse`, `RadiusOnUse`, `RadiusPerTick`, `Radius`, `custom_particle`, `potion_contents` and
//! `potion_duration_scale`. `Owner` goes through the `Owner` field of `things.rs` and `owners.rs`.
use crate::cloud_tick::wake;
use crate::cow::{after, now_of};
use crate::effect_cloud::CloudSpec;
use crate::effects::{potion_name, Brew, Kind, AMBIENT, HIDDEN};
use crate::nbt::{accept, owner, Field};
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, JavaString, Tag};
use std::borrow::Cow;

/// The keys of a potion cloud, in the jar's order of writing.
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("Age"), write: write_age, read: read_age, accepts: accept::int, owner: Some(owner::<CloudSpec>) },
    Field { key: Cow::Borrowed("Duration"), write: write_duration, read: read_duration, accepts: accept::int, owner: Some(owner::<CloudSpec>) },
    Field { key: Cow::Borrowed("WaitTime"), write: write_wait, read: read_wait, accepts: accept::int, owner: Some(owner::<CloudSpec>) },
    Field { key: Cow::Borrowed("ReapplicationDelay"), write: write_reapplication, read: read_reapplication, accepts: accept::int, owner: Some(owner::<CloudSpec>) },
    Field { key: Cow::Borrowed("DurationOnUse"), write: write_duration_on_use, read: read_duration_on_use, accepts: accept::int, owner: Some(owner::<CloudSpec>) },
    Field { key: Cow::Borrowed("RadiusOnUse"), write: write_radius_on_use, read: read_radius_on_use, accepts: accept::float, owner: Some(owner::<CloudSpec>) },
    Field { key: Cow::Borrowed("RadiusPerTick"), write: write_radius_per_tick, read: read_radius_per_tick, accepts: accept::float, owner: Some(owner::<CloudSpec>) },
    Field { key: Cow::Borrowed("Radius"), write: write_radius, read: read_radius, accepts: accept::float, owner: Some(owner::<CloudSpec>) },
    Field { key: Cow::Borrowed("custom_particle"), write: write_particle, read: read_particle, accepts: compound, owner: Some(owner::<CloudSpec>) },
    Field { key: Cow::Borrowed("potion_contents"), write: write_contents, read: read_contents, accepts: compound, owner: Some(owner::<CloudSpec>) },
    Field { key: Cow::Borrowed("potion_duration_scale"), write: write_scale, read: read_scale, accepts: accept::float, owner: Some(owner::<CloudSpec>) },
];

fn compound(t: &Tag) -> bool {
    matches!(t, Tag::Compound(_))
}

/// An `i32` key of the cloud, read and written as the field `$f`.
macro_rules! int_key {
    ($w:ident, $r:ident, $f:ident, $key:literal) => {
        fn $w(w: &World, e: Entity, c: &mut Compound) {
            if let Some(s) = w.get::<CloudSpec>(e) {
                c.insert($key.into(), Tag::Int(s.$f));
            }
        }
        fn $r(w: &mut World, e: Entity, t: &Tag) -> bool {
            let Tag::Int(v) = t else { return false };
            let Some(mut s) = w.get_mut::<CloudSpec>(e) else { return false };
            s.$f = *v;
            true
        }
    };
}

/// An `f32` key of the cloud, stored as its bits as the protocol does.
macro_rules! float_key {
    ($w:ident, $r:ident, $f:ident, $key:literal) => {
        fn $w(w: &World, e: Entity, c: &mut Compound) {
            if let Some(s) = w.get::<CloudSpec>(e) {
                c.insert($key.into(), Tag::Float(s.$f.to_bits()));
            }
        }
        fn $r(w: &mut World, e: Entity, t: &Tag) -> bool {
            let Tag::Float(v) = t else { return false };
            let Some(mut s) = w.get_mut::<CloudSpec>(e) else { return false };
            s.$f = f32::from_bits(*v);
            true
        }
    };
}

int_key!(write_duration, read_duration, duration, "Duration");
int_key!(write_wait, read_wait, wait, "WaitTime");
int_key!(write_reapplication, read_reapplication, reapplication, "ReapplicationDelay");
int_key!(write_duration_on_use, read_duration_on_use, duration_on_use, "DurationOnUse");
float_key!(write_radius_on_use, read_radius_on_use, radius_on_use, "RadiusOnUse");
float_key!(write_radius_per_tick, read_radius_per_tick, radius_per_tick, "RadiusPerTick");
float_key!(write_radius, read_radius, radius, "Radius");

/// `Age`: the cloud's age, its own time plus `age_base` (the saved age less the time already run), so the end (`Duration`) and the dose
/// phase go on where they were; the wake-up is armed with what the wait leaves of it (now if none is left).
fn write_age(w: &World, e: Entity, c: &mut Compound) {
    if let Some(s) = w.get::<CloudSpec>(e) {
        c.insert("Age".into(), Tag::Int(now_of(w, e) as i32 + s.age_base));
    }
}

fn read_age(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let Some(wait) = w.get::<CloudSpec>(e).map(|s| s.wait) else { return false };
    let now = now_of(w, e) as i32;
    if let Some(mut s) = w.get_mut::<CloudSpec>(e) {
        s.age_base = *v - now;
    }
    if *v < wait {
        after(w, e, wait - *v, wake);
    } else {
        wake(w, e);
    }
    true
}

/// `custom_particle`: the particle compound as saved, written only when there is one.
fn write_particle(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<CloudSpec>(e).and_then(|s| s.custom_particle.clone()) {
        c.insert("custom_particle".into(), Tag::Compound(p));
    }
}

fn read_particle(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Compound(p) = t else { return false };
    let Some(mut s) = w.get_mut::<CloudSpec>(e) else { return false };
    s.custom_particle = Some(p.clone());
    true
}

/// The mob effects of the jar's `custom_effects`, by id; the instant brews are the two instant effects.
pub(crate) const KINDS: &[(&str, Kind)] = &[
    ("minecraft:speed", Kind::Speed),
    ("minecraft:slowness", Kind::Slowness),
    ("minecraft:weakness", Kind::Weakness),
    ("minecraft:poison", Kind::Poison),
    (crate::species::lists::WITHER_EFFECT, Kind::Wither),
    ("minecraft:regeneration", Kind::Regeneration),
    ("minecraft:fire_resistance", Kind::FireResistance),
    ("minecraft:water_breathing", Kind::WaterBreathing),
    ("minecraft:invisibility", Kind::Invisibility),
    ("minecraft:levitation", Kind::Levitation),
    ("minecraft:hunger", Kind::Hunger),
    ("minecraft:mining_fatigue", Kind::MiningFatigue),
    ("minecraft:blindness", Kind::Blindness),
    ("minecraft:jump_boost", Kind::JumpBoost),
    ("minecraft:glowing", Kind::Glowing),
    ("minecraft:weaving", Kind::Weaving),
    ("minecraft:infested", Kind::Infested),
];

/// One `custom_effects` entry: its id, amplifier and duration (an instant brew lasts 1 tick, as the jar's instant effects do).
fn brew_tag(b: Brew) -> Compound {
    let flags = if let Brew::Flagged(.., f) = b { f } else { 0 };
    let (id, amp, ticks) = match b {
        Brew::Timed(kind, amp, ticks) | Brew::Flagged(kind, amp, ticks, _) => (KINDS.iter().find(|(_, k)| *k == kind).map_or("", |(n, _)| *n), amp, ticks),
        Brew::Harm(level) => ("minecraft:instant_damage", level, 1),
        Brew::Heal(level) => ("minecraft:instant_health", level, 1),
    };
    let mut m = Compound::new();
    m.insert("id".into(), Tag::String(JavaString::from(id)));
    m.insert("amplifier".into(), Tag::Int(amp));
    m.insert("duration".into(), Tag::Int(ticks));
    m.insert("ambient".into(), Tag::Byte(i8::from(flags & AMBIENT != 0)));
    m.insert("show_particles".into(), Tag::Byte(i8::from(flags & HIDDEN == 0)));
    m.insert("show_icon".into(), Tag::Byte(1));
    m
}

/// The brew an entry of `custom_effects` gives; None for an id the cloud does not know.
fn brew_of(m: &Compound) -> Option<Brew> {
    let Some(Tag::String(id)) = m.get(&JavaString::from("id")) else { return None };
    let amp = match m.get(&JavaString::from("amplifier")) {
        Some(Tag::Int(v)) => *v,
        _ => 0,
    };
    let ticks = match m.get(&JavaString::from("duration")) {
        Some(Tag::Int(v)) => *v,
        _ => 0,
    };
    match id.to_string_lossy().as_str() {
        "minecraft:instant_damage" => Some(Brew::Harm(amp)),
        "minecraft:instant_health" => Some(Brew::Heal(amp)),
        name => {
            let ambient = matches!(m.get(&JavaString::from("ambient")), Some(Tag::Byte(v)) if *v != 0);
            let hidden = matches!(m.get(&JavaString::from("show_particles")), Some(Tag::Byte(0)));
            let flags = if ambient { AMBIENT } else { 0 } | if hidden { HIDDEN } else { 0 };
            KINDS.iter().find(|(n, _)| *n == name).map(|(_, k)| Brew::Flagged(*k, amp, ticks, flags))
        }
    }
}

/// `potion_contents`: the potion's id, its colour when the cloud has one, and its custom effects when it has some.
fn write_contents(w: &World, e: Entity, c: &mut Compound) {
    let Some(s) = w.get::<CloudSpec>(e) else { return };
    let mut p = Compound::new();
    p.insert("potion".into(), Tag::String(JavaString::from(s.potion)));
    if let Some(color) = s.color {
        p.insert("custom_color".into(), Tag::Int(color));
    }
    if !s.custom.is_empty() {
        p.insert("custom_effects".into(), Tag::List(s.custom.iter().map(|b| Tag::Compound(brew_tag(*b))).collect()));
    }
    c.insert("potion_contents".into(), Tag::Compound(p));
}

fn read_contents(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Compound(p) = t else { return false };
    let Some(Tag::String(name)) = p.get(&JavaString::from("potion")) else { return false };
    let Some(potion) = potion_name(&name.to_string_lossy()) else { return false };
    let color = match p.get(&JavaString::from("custom_color")) {
        Some(Tag::Int(v)) => Some(*v),
        _ => None,
    };
    let custom = match p.get(&JavaString::from("custom_effects")) {
        Some(Tag::List(list)) => list.iter().filter_map(|t| if let Tag::Compound(m) = t { brew_of(m) } else { None }).collect(),
        _ => Vec::new(),
    };
    let Some(mut s) = w.get_mut::<CloudSpec>(e) else { return false };
    s.potion = potion;
    s.color = color;
    s.custom = custom;
    true
}

/// `potion_duration_scale`: written only when it is not the default 1.
fn write_scale(w: &World, e: Entity, c: &mut Compound) {
    if let Some(s) = w.get::<CloudSpec>(e).filter(|s| s.duration_scale != 1.0) {
        c.insert("potion_duration_scale".into(), Tag::Float(s.duration_scale.to_bits()));
    }
}

fn read_scale(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Float(v) = t else { return false };
    let Some(mut s) = w.get_mut::<CloudSpec>(e) else { return false };
    s.duration_scale = f32::from_bits(*v);
    true
}
