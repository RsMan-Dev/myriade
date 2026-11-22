//! The keys of the owners by uuid (`EntityReference`, the jar writes the uuid of an absent player too): the `Owner` of a thing (`things.rs`,
//! one generic declaration over the item target, the projectile and the tamed animal), the `owner` of a primed TNT, the `Thrower` of a stack.
//! A player is written by its uuid (`player_uuids`) and read back to the player, or waits for it. `FIELDS` applies to the entity holding
//! the component.
use crate::bridge::uuids::{entity_by_uuid, from_words, to_words, EntityUuid};
use crate::effect_cloud::CloudSpec;
use crate::item::{set_thrower, thrower, Item};
use crate::nbt::{owner, Field};
use crate::player_uuids::{id_of, uuid_of, wait, Slot};
use crate::projectile::Projectile;
use crate::tame::Owner;
use crate::targeting::Who;
use crate::tnt::{Primed, TAG_OWNER};
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, Tag};
use std::borrow::Cow;

/// The keys that apply to an entity holding their component.
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed(TAG_OWNER), write: write_tnt, read: read_tnt, accepts: uuid_tag, owner: Some(owner::<Primed>) },
    Field { key: Cow::Borrowed("Thrower"), write: write_thrower, read: read_thrower, accepts: uuid_tag, owner: Some(owner::<Item>) },
];

pub(crate) fn uuid_tag(t: &Tag) -> bool {
    words_tag(t).is_some()
}

fn words_tag(t: &Tag) -> Option<[i32; 4]> {
    // `UUIDUtil.CODEC` reads a numeric stream and keeps the first four words (`Util.fixedSize`): an int array, a long array (low 32 bits),
    // a byte array, or a list of numbers (floats cut toward zero, longs to their low 32 bits).
    let words: Vec<i32> = match t {
        Tag::IntArray(v) => v.iter().copied().take(4).collect(),
        Tag::LongArray(v) => v.iter().map(|&x| x as i32).take(4).collect(),
        Tag::ByteArray(v) => v.iter().map(|&x| i32::from(x as i8)).take(4).collect(),
        Tag::List(l) => l
            .iter()
            .map(|x| match x {
                Tag::Byte(v) => Some(i32::from(*v)),
                Tag::Short(v) => Some(i32::from(*v)),
                Tag::Int(v) => Some(*v),
                Tag::Long(v) => Some(*v as i32),
                Tag::Float(b) => Some(f32::from_bits(*b) as i32),
                Tag::Double(b) => Some(f64::from_bits(*b) as i32),
                _ => None,
            })
            .take(4)
            .collect::<Option<Vec<_>>>()?,
        _ => return None,
    };
    <[i32; 4]>::try_from(words.as_slice()).ok()
}

/// The four words of the uuid of `who`: a player's from the table, an entity's from its uuid; None while either is unknown.
fn words_of(w: &World, who: Who) -> Option<[i32; 4]> {
    let uuid = match who {
        Who::Player(id) => uuid_of(w, id)?,
        Who::Entity(e) => w.get::<EntityUuid>(e)?.0,
    };
    Some(to_words(uuid))
}

fn put(w: &World, who: Option<Who>, key: &str, c: &mut Compound) {
    if let Some(words) = who.and_then(|who| words_of(w, who)) {
        c.insert(key.into(), Tag::IntArray(words.to_vec()));
    }
}

/// The owner a uuid names: its player or entity when known; else `e` waits for that holder (`wait`) and has none for now.
fn take(w: &mut World, e: Entity, words: [i32; 4], slot: Slot) -> Option<Who> {
    if let Some(id) = id_of(w, from_words(words)) {
        return Some(Who::Player(id));
    }
    if let Some(h) = entity_by_uuid(w, words) {
        return Some(Who::Entity(h));
    }
    wait(w, from_words(words), e, slot);
    None
}

/// The `Owner` of an item (its raw target uuid), of a projectile, else of a tamed animal (created on read when absent).
pub(crate) fn write_owner(w: &World, e: Entity, c: &mut Compound) {
    if let Some(it) = w.get::<Item>(e) {
        if let Some(target) = it.target {
            c.insert("Owner".into(), Tag::IntArray(target.to_vec()));
        }
        return;
    }
    if let Some(k) = w.get::<CloudSpec>(e) {
        put(w, k.owner, "Owner", c);
        return;
    }
    let who = match w.get::<Projectile>(e) {
        Some(p) => p.owner,
        None => w.get::<Owner>(e).and_then(|o| o.0),
    };
    put(w, who, "Owner", c);
}

pub(crate) fn read_owner(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(words) = words_tag(t) else { return false };
    if let Some(mut it) = w.get_mut::<Item>(e) {
        it.target = Some(words);
        return true;
    }
    if w.get::<CloudSpec>(e).is_some() {
        let who = take(w, e, words, Slot::Cloud);
        if let Some(mut k) = w.get_mut::<CloudSpec>(e) {
            k.owner = who;
        }
        return true;
    }
    if w.get::<Projectile>(e).is_some() {
        let who = take(w, e, words, Slot::Projectile);
        if let Some(mut p) = w.get_mut::<Projectile>(e) {
            p.owner = who;
        }
        return true;
    }
    let who = take(w, e, words, Slot::Tame);
    w.entity_mut(e).insert(Owner(who));
    true
}

fn write_tnt(w: &World, e: Entity, c: &mut Compound) {
    put(w, w.get::<Primed>(e).and_then(|p| p.owner), TAG_OWNER, c);
}

fn read_tnt(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(words) = words_tag(t) else { return false };
    let who = take(w, e, words, Slot::Tnt);
    if let Some(mut p) = w.get_mut::<Primed>(e) {
        p.owner = who;
    }
    true
}

/// `ItemEntity.thrower`: the uuid as the four words it is kept in (no player to wait for).
fn write_thrower(w: &World, e: Entity, c: &mut Compound) {
    if let Some(t) = thrower(w, e) {
        c.insert("Thrower".into(), Tag::IntArray(t.to_vec()));
    }
}

fn read_thrower(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(words) = words_tag(t) else { return false };
    set_thrower(w, e, words);
    true
}
