//! The keys of a dragon's breath cloud (`AreaEffectCloud.addAdditionalSaveData`, same keys as the jar): `Age`, `Duration`, `Radius`,
//! `RadiusPerTick`. `Age` is the total age: the cloud's age while `Live`, else its age at the load plus the ticks since. A load re-arms the
//! wake-up and the end of the cloud (`breath_cloud::resume`). The owner is not kept: the dragon is the source of the damage.
use crate::breath_cloud::{resume, Live};
use crate::dragon::breath::Cloud;
use crate::cow::now_of;
use crate::nbt::{accept, owner, Field};
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, Tag};
use std::borrow::Cow;

/// The keys of a breath cloud, in the jar's order of writing.
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("Age"), write: write_age, read: read_age, accepts: accept::int, owner: Some(owner::<Cloud>) },
    Field { key: Cow::Borrowed("Duration"), write: write_duration, read: read_duration, accepts: accept::int, owner: Some(owner::<Cloud>) },
    Field { key: Cow::Borrowed("Radius"), write: write_radius, read: read_radius, accepts: accept::float, owner: Some(owner::<Cloud>) },
    Field { key: Cow::Borrowed("RadiusPerTick"), write: write_radius_per_tick, read: read_radius_per_tick, accepts: accept::float, owner: Some(owner::<Cloud>) },
];

/// `Age`: the total age, the inert clouds' ticks counted from their load (`now_of`), the live ones' own age.
fn write_age(w: &World, e: Entity, c: &mut Compound) {
    let Some(cl) = w.get::<Cloud>(e) else { return };
    let age = if w.get::<Live>(e).is_some() { cl.age } else { cl.age + now_of(w, e) as i32 };
    c.insert("Age".into(), Tag::Int(age));
}

fn read_age(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let Some(mut cl) = w.get_mut::<Cloud>(e) else { return false };
    cl.age = *v;
    drop(cl);
    resume(w, e, *v);
    true
}

fn write_duration(w: &World, e: Entity, c: &mut Compound) {
    if let Some(cl) = w.get::<Cloud>(e) {
        c.insert("Duration".into(), Tag::Int(cl.duration));
    }
}

fn read_duration(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let Some(mut cl) = w.get_mut::<Cloud>(e) else { return false };
    cl.duration = *v;
    true
}

/// The floats are kept as their bits, as the NBT floats are.
fn write_radius(w: &World, e: Entity, c: &mut Compound) {
    if let Some(cl) = w.get::<Cloud>(e) {
        c.insert("Radius".into(), Tag::Float(cl.radius.to_bits()));
    }
}

fn read_radius(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Float(v) = t else { return false };
    let Some(mut cl) = w.get_mut::<Cloud>(e) else { return false };
    cl.radius = f32::from_bits(*v);
    true
}

fn write_radius_per_tick(w: &World, e: Entity, c: &mut Compound) {
    if let Some(cl) = w.get::<Cloud>(e) {
        c.insert("RadiusPerTick".into(), Tag::Float(cl.radius_per_tick.to_bits()));
    }
}

fn read_radius_per_tick(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Float(v) = t else { return false };
    let Some(mut cl) = w.get_mut::<Cloud>(e) else { return false };
    cl.radius_per_tick = f32::from_bits(*v);
    true
}
