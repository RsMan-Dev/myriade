//! The villager's memories in `Brain.memories` (`Villager.readAdditionalSaveData` through the brain): the golem seen recently, the last
//! sleep (`LastSlept`) and the potential job site. Each memory is `{value}` and, while it expires, its `ttl` (ticks left). An absent
//! memory writes nothing; a memory read back is set with its ttl.
use crate::behaviours::golem_recent::GOLEM_RECENT;
use crate::behaviours::summon_defender::LastSlept;
use crate::brain::{self, MemValue};
use crate::nbt::Field;
use crate::species::villager::POTENTIAL_JOB_SITE;
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, Tag};
use std::borrow::Cow;

/// `Brain` is one key with one field: every memory writes into the same compound and reads from it. The memories owned by a
/// component (long jump, liked player, last sleep) are only written and read on the entities that carry it.
pub const FIELDS: &[Field] = &[Field { key: Cow::Borrowed("Brain"), write: write_all, read: read_all, accepts: crate::nbt::accept::any, owner: None }];

fn write_all(w: &World, e: Entity, c: &mut Compound) {
    write_golem(w, e, c);
    write_slept(w, e, c);
    write_site(w, e, c);
    if w.get::<crate::behaviours::long_jump::LongJumpState>(e).is_some() {
        super::creatures::write_brain(w, e, c);
    }
    super::owners::write_liked(w, e, c);
}

/// A `Brain` that is not a compound is refused; a memory that is absent is not.
fn read_all(w: &mut World, e: Entity, t: &Tag) -> bool {
    if !matches!(t, Tag::Compound(_)) {
        return false;
    }
    read_golem(w, e, t);
    if w.get::<LastSlept>(e).is_some() {
        read_slept(w, e, t);
    }
    read_site(w, e, t);
    if w.get::<crate::behaviours::long_jump::LongJumpState>(e).is_some() {
        super::creatures::read_brain(w, e, t);
    }
    if w.get::<crate::hand_over::Liked>(e).is_some() {
        super::owners::read_liked(w, e, t);
    }
    true
}

/// The memory's tag: its value, and its ttl when it expires.
fn memory(value: Tag, ttl: Option<u64>) -> Tag {
    match ttl {
        Some(t) => Tag::compound([("value", value), ("ttl", Tag::Long(t as i64))]),
        None => Tag::compound([("value", value)]),
    }
}

/// Puts a memory under `Brain.memories` (the compound the brain's other keys share).
fn put(c: &mut Compound, key: &str, mem: Tag) {
    let Tag::Compound(brain) = c.entry("Brain".into()).or_insert_with(|| Tag::Compound(Default::default())) else { return };
    let Tag::Compound(memories) = brain.entry("memories".into()).or_insert_with(|| Tag::Compound(Default::default())) else { return };
    memories.insert(key.into(), mem);
}

/// The value and the ttl of a memory tag, or none when it has no value.
fn parts(t: &Tag, key: &str) -> Option<(Tag, Option<u64>)> {
    let mem = t.get("memories")?.get(key)?;
    let value = mem.get("value")?.clone();
    let ttl = match mem.get("ttl") {
        Some(Tag::Long(v)) if *v != i64::MAX => Some((*v).max(0) as u64),
        _ => None,
    };
    Some((value, ttl))
}

fn write_golem(w: &World, e: Entity, c: &mut Compound) {
    if brain::present(w, e, &GOLEM_RECENT) {
        put(c, "minecraft:golem_detected_recently", memory(Tag::Byte(1), brain::ttl_left(w, e, &GOLEM_RECENT)));
    }
}

fn read_golem(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some((_, ttl)) = parts(t, "minecraft:golem_detected_recently") else { return false };
    brain::set(w, e, &GOLEM_RECENT, MemValue::Unit, ttl);
    true
}

fn write_slept(w: &World, e: Entity, c: &mut Compound) {
    if let Some(s) = w.get::<LastSlept>(e) {
        put(c, "minecraft:last_slept", memory(Tag::Long(s.0 as i64), None));
    }
}

fn read_slept(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some((Tag::Long(v), _)) = parts(t, "minecraft:last_slept") else { return false };
    w.entity_mut(e).insert(LastSlept(v.max(0) as u64));
    true
}

fn write_site(w: &World, e: Entity, c: &mut Compound) {
    let Some(MemValue::Block(p)) = brain::get(w, e, &POTENTIAL_JOB_SITE) else { return };
    let dim = w.get_resource::<crate::bridge::host::WorldDimension>().map_or("minecraft:overworld", |d| d.0);
    let value = Tag::compound([("dimension", Tag::String(dim.into())), ("pos", Tag::IntArray(p.to_vec()))]);
    put(c, "minecraft:potential_job_site", memory(value, brain::ttl_left(w, e, &POTENTIAL_JOB_SITE)));
}

fn read_site(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some((value, ttl)) = parts(t, "minecraft:potential_job_site") else { return false };
    let Some(Tag::IntArray(v)) = value.get("pos") else { return false };
    let [x, y, z] = v.as_slice() else { return false };
    brain::set(w, e, &POTENTIAL_JOB_SITE, MemValue::Block([*x, *y, *z]), ttl);
    true
}
