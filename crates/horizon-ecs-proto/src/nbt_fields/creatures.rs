//! Les champs NBT propres aux créatures, déclarés par comportement (un composant, des clés). Un champ ne s'applique qu'à une entité qui
//! porte son composant : sans lui, la clé reste libre (rien n'est inventé). Pas de clé pour `ignited`, que la sauvegarde de référence
//! n'écrit pas.
use crate::behaviours::attached_face::Attached;
use crate::behaviours::bee::Bee;
use crate::behaviours::cat_state::Trusting;
use crate::behaviours::pufferfish_puff::Puff;
use crate::goals::{Occupied, OCC_SITTING};
use crate::behaviours::trade_offers::{profession_name, variant_name, Merchant};
use crate::behaviours::do_nothing::Invulnerable;
use crate::behaviours::forget_target::LlamaStrength;
use crate::cow::{now_of, GrowAt};
use crate::explosion::{Charged, Fuse};
use crate::nbt::{accept, owner, Field};
use crate::size::Size;
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, JavaString, Tag};
use std::borrow::Cow;

fn byte(t: &Tag) -> bool {
    matches!(t, Tag::Byte(_))
}

/// The declared fields, by behaviour: sized (`Size`), creeper (`Fuse`, `ExplosionRadius`, `powered`), wither (`Invul`), llama (`Strength`).
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("Size"), write: write_size, read: read_size, accepts: accept::int, owner: Some(owner::<Size>) },
    Field { key: Cow::Borrowed("Fuse"), write: write_fuse, read: read_fuse, accepts: accept::short, owner: Some(owner::<Fuse>) },
    Field { key: Cow::Borrowed("ExplosionRadius"), write: write_radius, read: read_radius, accepts: byte, owner: Some(owner::<Fuse>) },
    Field { key: Cow::Borrowed("powered"), write: write_powered, read: read_powered, accepts: byte, owner: Some(owner::<Charged>) },
    Field { key: Cow::Borrowed("Invul"), write: write_invul, read: read_invul, accepts: accept::int, owner: Some(owner::<Invulnerable>) },
    Field { key: Cow::Borrowed("Strength"), write: write_strength, read: read_strength, accepts: accept::int, owner: Some(owner::<LlamaStrength>) },
    Field { key: Cow::Borrowed("Age"), write: write_age, read: read_age, accepts: accept::int, owner: Some(owner::<GrowAt>) },
    Field { key: Cow::Borrowed("AttachFace"), write: write_face, read: read_face, accepts: byte, owner: Some(owner::<Attached>) },
    Field { key: Cow::Borrowed("Color"), write: write_color, read: read_color, accepts: byte, owner: Some(owner::<Attached>) },
    Field { key: Cow::Borrowed("VillagerData"), write: write_villager, read: read_villager, accepts: compound, owner: Some(owner::<Merchant>) },
    Field { key: Cow::Borrowed("IsBaby"), write: write_baby, read: read_baby, accepts: byte, owner: Some(owner::<GrowAt>) },
    Field { key: Cow::Borrowed("HasNectar"), write: write_nectar, read: read_nectar, accepts: byte, owner: Some(owner::<Bee>) },
    Field { key: Cow::Borrowed("HasStung"), write: write_stung, read: read_stung, accepts: byte, owner: Some(owner::<Bee>) },
    Field { key: Cow::Borrowed("CannotEnterHiveTicks"), write: write_stay_out, read: read_stay_out, accepts: accept::int, owner: Some(owner::<Bee>) },
    Field { key: Cow::Borrowed("hive_pos"), write: write_hive, read: read_hive, accepts: int_array, owner: Some(owner::<Bee>) },
    Field { key: Cow::Borrowed("Trusting"), write: write_trusting, read: read_trusting, accepts: byte, owner: None },
    Field { key: Cow::Borrowed("CropsGrownSincePollination"), write: write_crops, read: read_crops, accepts: accept::int, owner: Some(owner::<Bee>) },
    Field { key: Cow::Borrowed("PuffState"), write: write_puff, read: read_puff, accepts: accept::int, owner: Some(owner::<Puff>) },
    Field { key: Cow::Borrowed("Sitting"), write: write_sitting, read: read_sitting, accepts: byte, owner: Some(owner::<Occupied>) },
    Field { key: Cow::Borrowed("flower_pos"), write: write_flower, read: read_flower, accepts: int_array, owner: Some(owner::<Bee>) },
];

fn int_array(t: &Tag) -> bool {
    matches!(t, Tag::IntArray(_))
}

fn write_size(w: &World, e: Entity, c: &mut Compound) {
    if let Some(s) = w.get::<Size>(e) {
        c.insert("Size".into(), Tag::Int(s.0 - 1));
    }
}

/// The jar's `setSize(getInt + 1)`, clamped to 1..=127 as the Horizon client does.
fn read_size(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    w.entity_mut(e).insert(Size((v + 1).clamp(1, 127)));
    true
}

/// The maximum swell, as the creeper's `Fuse` short.
fn write_fuse(w: &World, e: Entity, c: &mut Compound) {
    if let Some(f) = w.get::<Fuse>(e) {
        c.insert("Fuse".into(), Tag::Short(f.max as i16));
    }
}

fn read_fuse(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Short(v) = t else { return false };
    let Some(mut f) = w.get_mut::<Fuse>(e) else { return false };
    f.max = i32::from(*v);
    true
}

/// The blast radius, a byte as the jar writes it.
fn write_radius(w: &World, e: Entity, c: &mut Compound) {
    if let Some(f) = w.get::<Fuse>(e) {
        c.insert("ExplosionRadius".into(), Tag::Byte(f.radius as i8));
    }
}

fn read_radius(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut f) = w.get_mut::<Fuse>(e) else { return false };
    f.radius = f32::from(*v);
    true
}

/// `powered` is the `Charged` marker: a byte of one when it is there.
fn write_powered(w: &World, e: Entity, c: &mut Compound) {
    if w.get::<Charged>(e).is_some() {
        c.insert("powered".into(), Tag::Byte(1));
    }
}

fn read_powered(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    if *v != 0 {
        w.entity_mut(e).insert(Charged);
    } else {
        w.entity_mut(e).remove::<Charged>();
    }
    true
}

/// The wither's invulnerable countdown, an int.
fn write_invul(w: &World, e: Entity, c: &mut Compound) {
    if let Some(i) = w.get::<Invulnerable>(e) {
        c.insert("Invul".into(), Tag::Int(i.0));
    }
}

fn read_invul(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    w.entity_mut(e).insert(Invulnerable(*v));
    true
}

/// The llama's strength, an int.
fn write_strength(w: &World, e: Entity, c: &mut Compound) {
    if let Some(s) = w.get::<LlamaStrength>(e) {
        c.insert("Strength".into(), Tag::Int(s.0));
    }
}

fn read_strength(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    w.entity_mut(e).insert(LlamaStrength(*v));
    true
}

/// `Age`: negative, the ticks left as a baby (`GrowAt` is the tick it grows at); zero or more, grown up (`GrowAt` goes).
fn write_age(w: &World, e: Entity, c: &mut Compound) {
    if let Some(g) = w.get::<GrowAt>(e) {
        let left = g.0.saturating_sub(now_of(w, e));
        c.insert("Age".into(), Tag::Int(-i32::try_from(left).unwrap_or(i32::MAX)));
    }
}

fn read_age(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    if *v < 0 {
        let at = now_of(w, e) + u64::from(v.unsigned_abs());
        w.entity_mut(e).insert(GrowAt(at));
    } else {
        w.entity_mut(e).remove::<GrowAt>();
    }
    true
}

/// The shulker's lid: the face it sits on, and its colour (bytes).
fn write_face(w: &World, e: Entity, c: &mut Compound) {
    if let Some(a) = w.get::<Attached>(e) {
        c.insert("AttachFace".into(), Tag::Byte(a.face as i8));
    }
}

fn read_face(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut a) = w.get_mut::<Attached>(e) else { return false };
    a.face = *v as u8;
    true
}

fn write_color(w: &World, e: Entity, c: &mut Compound) {
    if let Some(a) = w.get::<Attached>(e) {
        c.insert("Color".into(), Tag::Byte(a.color as i8));
    }
}

fn read_color(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut a) = w.get_mut::<Attached>(e) else { return false };
    a.color = *v as u8;
    true
}

/// The bee's flags: nectar and sting, as bytes.
fn write_nectar(w: &World, e: Entity, c: &mut Compound) {
    if let Some(b) = w.get::<Bee>(e) {
        c.insert("HasNectar".into(), Tag::Byte(i8::from(b.nectar)));
    }
}

fn read_nectar(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut b) = w.get_mut::<Bee>(e) else { return false };
    b.nectar = *v != 0;
    true
}

fn write_stung(w: &World, e: Entity, c: &mut Compound) {
    if let Some(b) = w.get::<Bee>(e) {
        c.insert("HasStung".into(), Tag::Byte(i8::from(b.stung)));
    }
}

fn read_stung(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut b) = w.get_mut::<Bee>(e) else { return false };
    b.stung = *v != 0;
    true
}

/// The ticks left before the bee may enter its hive: the own time it ends at, less now.
fn write_stay_out(w: &World, e: Entity, c: &mut Compound) {
    if let Some(b) = w.get::<Bee>(e) {
        let left = b.stay_out_until.saturating_sub(now_of(w, e));
        c.insert("CannotEnterHiveTicks".into(), Tag::Int(i32::try_from(left).unwrap_or(i32::MAX)));
    }
}

fn read_stay_out(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let now = now_of(w, e);
    let Some(mut b) = w.get_mut::<Bee>(e) else { return false };
    b.stay_out_until = now + u64::try_from(*v).unwrap_or(0);
    true
}

/// The remembered hive and flower: three ints when there is one.
fn write_hive(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<Bee>(e).and_then(|b| b.hive) {
        c.insert("hive_pos".into(), Tag::IntArray(p.to_vec()));
    }
}

fn read_hive(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::IntArray(v) = t else { return false };
    let Ok([x, y, z]) = <[i32; 3]>::try_from(v.as_slice()) else { return false };
    let Some(mut b) = w.get_mut::<Bee>(e) else { return false };
    b.hive = Some([x, y, z]);
    true
}

fn write_flower(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<Bee>(e).and_then(|b| b.flower) {
        c.insert("flower_pos".into(), Tag::IntArray(p.to_vec()));
    }
}

fn read_flower(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::IntArray(v) = t else { return false };
    let Ok([x, y, z]) = <[i32; 3]>::try_from(v.as_slice()) else { return false };
    let Some(mut b) = w.get_mut::<Bee>(e) else { return false };
    b.flower = Some([x, y, z]);
    true
}

fn compound(t: &Tag) -> bool {
    matches!(t, Tag::Compound(_))
}

/// `VillagerData`: the type, the profession by name, and the level. A wandering trader has none.
fn write_villager(w: &World, e: Entity, c: &mut Compound) {
    let Some(m) = w.get::<Merchant>(e) else { return };
    if m.wandering {
        return;
    }
    let mut d = Compound::new();
    d.insert("type".into(), Tag::String(JavaString::from(variant_name(m.variant).as_str())));
    d.insert("profession".into(), Tag::String(JavaString::from(profession_name(w, m.profession).as_str())));
    d.insert("level".into(), Tag::Int(i32::from(m.level)));
    c.insert("VillagerData".into(), Tag::Compound(d));
}

/// The index of a type or a profession from its name (the tables are short: a scan is at load only).
fn read_villager(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Compound(d) = t else { return false };
    let text = |k: &str| match d.get(&JavaString::from(k)) {
        Some(Tag::String(s)) => Some(s.to_string_lossy()),
        _ => None,
    };
    let (Some(kind), Some(job)) = (text("type"), text("profession")) else { return false };
    let Some(Tag::Int(level)) = d.get(&JavaString::from("level")) else { return false };
    let (variant, profession) = {
        let ww: &World = w;
        ((0..=u8::MAX).find(|&i| variant_name(i) == kind), (0..=u8::MAX).find(|&p| profession_name(ww, p) == job))
    };
    let (Some(variant), Some(profession)) = (variant, profession) else { return false };
    let Some(mut m) = w.get_mut::<Merchant>(e) else { return false };
    m.variant = variant;
    m.profession = profession;
    m.level = (*level).clamp(1, 5) as u8;
    true
}

/// `IsBaby` is `GrowAt` (the baby's `Age` carries the time): written as a byte, read as given, the age does the work.
fn write_baby(w: &World, e: Entity, c: &mut Compound) {
    if w.get::<GrowAt>(e).is_some() {
        c.insert("IsBaby".into(), Tag::Byte(1));
    }
}

fn read_baby(_: &mut World, _: Entity, t: &Tag) -> bool {
    matches!(t, Tag::Byte(_))
}

/// `Trusting` is a marker: written when present; the read of 1 sets it (an ocelot only).
fn write_trusting(w: &World, e: Entity, c: &mut Compound) {
    if w.get::<Trusting>(e).is_some() {
        c.insert("Trusting".into(), Tag::Byte(1));
    }
}

fn read_trusting(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    if *v != 0 {
        w.entity_mut(e).insert(Trusting);
    }
    true
}

fn write_crops(w: &World, e: Entity, c: &mut Compound) {
    if let Some(b) = w.get::<Bee>(e) {
        c.insert("CropsGrownSincePollination".into(), Tag::Int(b.crops));
    }
}

fn read_crops(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let Some(mut b) = w.get_mut::<Bee>(e) else { return false };
    b.crops = *v;
    true
}

/// The pufferfish's `PuffState` (0 deflated, 1 half, 2 full).
fn write_puff(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<Puff>(e) {
        c.insert("PuffState".into(), Tag::Int(i32::from(p.size)));
    }
}

fn read_puff(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let Some(mut p) = w.get_mut::<Puff>(e) else { return false };
    p.size = (*v).clamp(0, 2) as u8;
    true
}

/// `TamableAnimal.Sitting`, the sitting bit of the occupations.
fn write_sitting(w: &World, e: Entity, c: &mut Compound) {
    if let Some(o) = w.get::<Occupied>(e) {
        c.insert("Sitting".into(), Tag::Byte(i8::from(o.0 & OCC_SITTING != 0)));
    }
}

fn read_sitting(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut o) = w.get_mut::<Occupied>(e) else { return false };
    if *v != 0 { o.0 |= OCC_SITTING } else { o.0 &= !OCC_SITTING }
    true
}
