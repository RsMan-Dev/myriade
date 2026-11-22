//! The NBT keys of the item frames and paintings (`frame_item::save`, `painting::save`). The placement (`block_pos`, the face) is the
//! `Attached` of both; a frame's state is its `Held` and flags, a painting's its `Motive`. Each write goes through the setter of its
//! state, so the wall and the support index hold as they do at placement.
use crate::frame_item::{self, Fixed, Held, Invisible};
use crate::hanging::{self, Attached};
use crate::nbt::{accept, owner, Field};
use crate::painting::{self, Motives};
use crate::species::painting::Motive;
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, Tag};
use horizon_items::item_nbt::{item_from_nbt, item_to_nbt};
use std::borrow::Cow;

/// The keys of the frames and paintings, in the order of their saves.
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("block_pos"), write: write_pos, read: read_pos, accepts: int_array, owner: Some(owner::<Attached>) },
    Field { key: Cow::Borrowed("Facing"), write: write_frame_facing, read: read_frame_facing, accepts: byte, owner: Some(owner::<Held>) },
    Field { key: Cow::Borrowed("ItemRotation"), write: write_rotation, read: read_rotation, accepts: byte, owner: Some(owner::<Held>) },
    Field { key: Cow::Borrowed("ItemDropChance"), write: write_drop_chance, read: read_drop_chance, accepts: accept::float, owner: Some(owner::<Held>) },
    Field { key: Cow::Borrowed("Item"), write: write_item, read: read_item, accepts: compound, owner: Some(owner::<Held>) },
    Field { key: Cow::Borrowed("Invisible"), write: write_invisible, read: read_invisible, accepts: byte, owner: Some(owner::<Held>) },
    Field { key: Cow::Borrowed("Fixed"), write: write_fixed, read: read_fixed, accepts: byte, owner: Some(owner::<Held>) },
    Field { key: Cow::Borrowed("facing"), write: write_facing, read: read_facing, accepts: byte, owner: Some(owner::<Motive>) },
    Field { key: Cow::Borrowed("variant"), write: write_variant, read: read_variant, accepts: string, owner: Some(owner::<Motive>) },
];

fn byte(t: &Tag) -> bool {
    matches!(t, Tag::Byte(_))
}

fn int_array(t: &Tag) -> bool {
    matches!(t, Tag::IntArray(_))
}

fn string(t: &Tag) -> bool {
    matches!(t, Tag::String(_))
}

fn compound(t: &Tag) -> bool {
    matches!(t, Tag::Compound(_))
}

/// Moves the hanging entity `e` to its own block, on `face`.
fn on_face(w: &mut World, e: Entity, face: u8) -> bool {
    let Some(pos) = w.get::<Attached>(e).map(|a| a.pos) else { return false };
    hanging::set_placement(w, e, pos, face)
}

fn write_pos(w: &World, e: Entity, c: &mut Compound) {
    if let Some(a) = w.get::<Attached>(e) {
        c.insert("block_pos".into(), Tag::IntArray(a.pos.to_vec()));
    }
}

fn read_pos(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::IntArray(v) = t else { return false };
    let Ok(pos) = <[i32; 3]>::try_from(v.as_slice()) else { return false };
    let Some(face) = w.get::<Attached>(e).map(|a| a.face) else { return false };
    hanging::set_placement(w, e, pos, face)
}

fn write_frame_facing(w: &World, e: Entity, c: &mut Compound) {
    if let Some(a) = w.get::<Attached>(e) {
        c.insert("Facing".into(), Tag::Byte(a.face as i8));
    }
}

fn read_frame_facing(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(face) = u8::try_from(*v).ok().filter(|&f| f < 6) else { return false };
    on_face(w, e, face)
}

fn write_rotation(w: &World, e: Entity, c: &mut Compound) {
    if let Some(h) = w.get::<Held>(e) {
        c.insert("ItemRotation".into(), Tag::Byte(h.rotation as i8));
    }
}

fn read_rotation(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    frame_item::set_rotation(w, e, v.rem_euclid(8) as u8)
}

fn write_drop_chance(w: &World, e: Entity, c: &mut Compound) {
    if let Some(h) = w.get::<Held>(e) {
        c.insert("ItemDropChance".into(), Tag::Float(h.drop_chance.to_bits()));
    }
}

fn read_drop_chance(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Float(b) = t else { return false };
    let Some(mut h) = w.get_mut::<Held>(e) else { return false };
    h.drop_chance = f32::from_bits(*b);
    true
}

fn write_item(w: &World, e: Entity, c: &mut Compound) {
    if let Some(stack) = w.get::<Held>(e).and_then(|h| h.stack.as_ref()) {
        c.insert("Item".into(), item_to_nbt(stack));
    }
}

fn read_item(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(stack) = item_from_nbt(t) else { return false };
    let Some(mut h) = w.get_mut::<Held>(e) else { return false };
    h.stack = Some(stack);
    true
}

fn write_invisible(w: &World, e: Entity, c: &mut Compound) {
    c.insert("Invisible".into(), Tag::Byte(i8::from(w.get::<Invisible>(e).is_some_and(|i| i.0))));
}

fn read_invisible(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    frame_item::set_invisible(w, e, *v != 0)
}

fn write_fixed(w: &World, e: Entity, c: &mut Compound) {
    c.insert("Fixed".into(), Tag::Byte(i8::from(w.get::<Fixed>(e).is_some_and(|f| f.0))));
}

fn read_fixed(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    frame_item::set_fixed(w, e, *v != 0)
}

fn write_facing(w: &World, e: Entity, c: &mut Compound) {
    if let Some(a) = w.get::<Attached>(e) {
        c.insert("facing".into(), Tag::Byte(painting::saved_facing(a.face) as i8));
    }
}

fn read_facing(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(face) = painting::face_of_saved(*v as u8) else { return false };
    on_face(w, e, face)
}

fn write_variant(w: &World, e: Entity, c: &mut Compound) {
    let (Some(m), Some(motives)) = (w.get::<Motive>(e), w.get_resource::<Motives>()) else { return };
    if let Some((name, _)) = motives.0.get(usize::from(m.0)) {
        c.insert("variant".into(), Tag::String((&**name).into()));
    }
}

fn read_variant(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::String(s) = t else { return false };
    painting::set_motive(w, e, &s.to_string_lossy())
}
