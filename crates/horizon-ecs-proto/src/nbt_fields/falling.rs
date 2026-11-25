//! The keys of a falling block the entity keeps: its `Time` (ticks in the air), `HurtEntities`, `FallHurtAmount`, `FallHurtMax`, `CancelDrop` and
//! `TileEntityData`. `BlockState` and `DropItem` are in `things.rs`.
use crate::cow::{after, now_of};
use crate::falling_block::{expire, FallingBlock, LIMIT};
use crate::nbt::{owner, Field};
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, Tag};
use std::borrow::Cow;

/// The keys of a falling block, in the jar's order of writing.
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("Time"), write: write_time, read: read_time, accepts: crate::nbt::accept::int, owner: Some(owner::<FallingBlock>) },
    Field { key: Cow::Borrowed("HurtEntities"), write: write_hurt, read: read_hurt, accepts: byte, owner: Some(owner::<FallingBlock>) },
    Field { key: Cow::Borrowed("FallHurtAmount"), write: write_amount, read: read_amount, accepts: crate::nbt::accept::float, owner: Some(owner::<FallingBlock>) },
    Field { key: Cow::Borrowed("FallHurtMax"), write: write_max, read: read_max, accepts: crate::nbt::accept::int, owner: Some(owner::<FallingBlock>) },
    Field { key: Cow::Borrowed("CancelDrop"), write: write_cancel, read: read_cancel, accepts: byte, owner: Some(owner::<FallingBlock>) },
    Field { key: Cow::Borrowed("TileEntityData"), write: write_data, read: read_data, accepts: compound, owner: Some(owner::<FallingBlock>) },
];

fn byte(t: &Tag) -> bool {
    matches!(t, Tag::Byte(_))
}

fn compound(t: &Tag) -> bool {
    matches!(t, Tag::Compound(_))
}

/// `Time`: the entity's own ticks since it fell. The load re-arms the deadline with what the jar's limit leaves of it.
fn write_time(w: &World, e: Entity, c: &mut Compound) {
    if let Some(f) = w.get::<FallingBlock>(e) {
        // An entity asleep (its chunk does not tick) stands at the tick it fell asleep: its `Time` does not run.
        let now = match w.get::<crate::zone::Dormant>(e) {
            Some(d) => now_of(w, e).saturating_sub(w.resource::<crate::cow::Clock>().0.saturating_sub(d.since)),
            None => now_of(w, e),
        };
        c.insert("Time".into(), Tag::Int(now.saturating_sub(f.born) as i32));
    }
}

fn read_time(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let now = now_of(w, e);
    let Some(mut f) = w.get_mut::<FallingBlock>(e) else { return false };
    f.born = now.saturating_sub(i64::from(*v).max(0) as u64);
    // The deadline `fall` armed for a new block is the stale one: the earlier one ends the fall, and `expire` ignores a gone entity.
    after(w, e, (LIMIT + 1 - *v).max(0), expire);
    true
}

fn write_hurt(w: &World, e: Entity, c: &mut Compound) {
    if let Some(f) = w.get::<FallingBlock>(e) {
        c.insert("HurtEntities".into(), Tag::Byte(i8::from(f.settings.hurt_entities)));
    }
}

fn read_hurt(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut f) = w.get_mut::<FallingBlock>(e) else { return false };
    f.settings.hurt_entities = *v != 0;
    true
}

fn write_amount(w: &World, e: Entity, c: &mut Compound) {
    if let Some(f) = w.get::<FallingBlock>(e) {
        c.insert("FallHurtAmount".into(), Tag::Float(f.settings.fall_hurt_amount.to_bits()));
    }
}

fn read_amount(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Float(v) = t else { return false };
    let Some(mut f) = w.get_mut::<FallingBlock>(e) else { return false };
    f.settings.fall_hurt_amount = f32::from_bits(*v);
    true
}

fn write_max(w: &World, e: Entity, c: &mut Compound) {
    if let Some(f) = w.get::<FallingBlock>(e) {
        c.insert("FallHurtMax".into(), Tag::Int(f.settings.fall_hurt_max));
    }
}

fn read_max(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let Some(mut f) = w.get_mut::<FallingBlock>(e) else { return false };
    f.settings.fall_hurt_max = *v;
    true
}

fn write_cancel(w: &World, e: Entity, c: &mut Compound) {
    if let Some(f) = w.get::<FallingBlock>(e) {
        c.insert("CancelDrop".into(), Tag::Byte(i8::from(f.settings.cancel_drop)));
    }
}

fn read_cancel(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut f) = w.get_mut::<FallingBlock>(e) else { return false };
    f.settings.cancel_drop = *v != 0;
    true
}

/// `TileEntityData`: the block entity data the block lands with, written only when it has some.
fn write_data(w: &World, e: Entity, c: &mut Compound) {
    if let Some(d) = w.get::<FallingBlock>(e).and_then(|f| f.data.clone()) {
        c.insert("TileEntityData".into(), Tag::Compound(d));
    }
}

fn read_data(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Compound(d) = t else { return false };
    let Some(mut f) = w.get_mut::<FallingBlock>(e) else { return false };
    f.data = Some(d.clone());
    true
}
