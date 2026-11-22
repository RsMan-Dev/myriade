//! The keys of the vehicles, stands, frames and riders: the armor stand's flags, pose and disabled slots (`Stand`, `Pose`, `DisabledSlots`),
//! a hanging entity's item, rotation, facing and block (`Held`, `Attached`), a furnace or TNT cart (`Furnace`, `Fuse`), and the riders of
//! a mount (`Passengers`, written by the mount's compound, the riders not saved a second time by `save_all`).
use crate::frame_item::Held;
use crate::hanging::Attached;
use crate::minecart::{Fuse, Furnace};
use crate::mount::Mount;
use crate::nbt::{owner, Field};
use crate::seat;
use crate::stand::{set_pose, DisabledSlots, Stand, DEFAULT_POSE, INVISIBLE, MARKER, NO_BASE_PLATE, SHOW_ARMS, SMALL};
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, JavaString, Tag};
use horizon_items::item_nbt::{item_from_nbt, item_to_nbt};
use std::borrow::Cow;

/// The keys of the rides, in the jar's order of writing.
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("Small"), write: write_small, read: read_small, accepts: byte, owner: Some(owner::<Stand>) },
    Field { key: Cow::Borrowed("ShowArms"), write: write_show_arms, read: read_show_arms, accepts: byte, owner: Some(owner::<Stand>) },
    Field { key: Cow::Borrowed("NoBasePlate"), write: write_no_base_plate, read: read_no_base_plate, accepts: byte, owner: Some(owner::<Stand>) },
    Field { key: Cow::Borrowed("Marker"), write: write_marker, read: read_marker, accepts: byte, owner: Some(owner::<Stand>) },
    Field { key: Cow::Borrowed("Invisible"), write: write_invisible, read: read_invisible, accepts: byte, owner: Some(owner::<Stand>) },
    Field { key: Cow::Borrowed("Pose"), write: write_pose, read: read_pose, accepts: compound, owner: Some(owner::<Stand>) },
    Field { key: Cow::Borrowed("DisabledSlots"), write: write_disabled, read: read_disabled, accepts: crate::nbt::accept::int, owner: Some(owner::<DisabledSlots>) },
    Field { key: Cow::Borrowed("Item"), write: write_item, read: read_item, accepts: compound, owner: Some(owner::<Held>) },
    Field { key: Cow::Borrowed("ItemRotation"), write: write_rotation, read: read_rotation, accepts: byte, owner: Some(owner::<Held>) },
    Field { key: Cow::Borrowed("Facing"), write: write_facing, read: read_facing, accepts: byte, owner: Some(owner::<Attached>) },
    Field { key: Cow::Borrowed("block_pos"), write: write_block_pos, read: read_block_pos, accepts: int_array, owner: Some(owner::<Attached>) },
    Field { key: Cow::Borrowed("PushX"), write: write_push_x, read: read_push_x, accepts: double, owner: Some(owner::<Furnace>) },
    Field { key: Cow::Borrowed("PushZ"), write: write_push_z, read: read_push_z, accepts: double, owner: Some(owner::<Furnace>) },
    Field { key: Cow::Borrowed("Fuel"), write: write_fuel, read: read_fuel, accepts: crate::nbt::accept::short, owner: Some(owner::<Furnace>) },
    Field { key: Cow::Borrowed("fuse"), write: write_fuse, read: read_fuse, accepts: crate::nbt::accept::int, owner: Some(owner::<Fuse>) },
    Field { key: Cow::Borrowed("Passengers"), write: write_passengers, read: read_passengers, accepts: riders, owner: None },
];

/// The creators: a key of a component the entity lacks (a component added in play, then saved) gives it back to that component. Never
/// applicable: `register_creators` keeps them out of the keys that save and load use.
pub const CREATORS: &[Field] = &[
    Field { key: Cow::Borrowed("Pose"), write: write_pose, read: read_pose, accepts: compound, owner: None },
    Field { key: Cow::Borrowed("DisabledSlots"), write: write_disabled, read: read_disabled, accepts: crate::nbt::accept::int, owner: None },
    Field { key: Cow::Borrowed("PushX"), write: write_push_x, read: create_push_x, accepts: double, owner: None },
    Field { key: Cow::Borrowed("PushZ"), write: write_push_z, read: create_push_z, accepts: double, owner: None },
    Field { key: Cow::Borrowed("Fuel"), write: write_fuel, read: create_fuel, accepts: crate::nbt::accept::short, owner: None },
    Field { key: Cow::Borrowed("fuse"), write: write_fuse, read: create_fuse, accepts: crate::nbt::accept::int, owner: None },
];

fn furnace(w: &mut World, e: Entity) {
    if w.get::<Furnace>(e).is_none() {
        w.entity_mut(e).insert(Furnace::default());
    }
}

fn create_push_x(w: &mut World, e: Entity, t: &Tag) -> bool {
    double(t) && {
        furnace(w, e);
        read_push_x(w, e, t)
    }
}

fn create_push_z(w: &mut World, e: Entity, t: &Tag) -> bool {
    double(t) && {
        furnace(w, e);
        read_push_z(w, e, t)
    }
}

fn create_fuel(w: &mut World, e: Entity, t: &Tag) -> bool {
    crate::nbt::accept::short(t) && {
        furnace(w, e);
        read_fuel(w, e, t)
    }
}

fn create_fuse(w: &mut World, e: Entity, t: &Tag) -> bool {
    crate::nbt::accept::int(t) && {
        if w.get::<Fuse>(e).is_none() {
            w.entity_mut(e).insert(Fuse(-1));
        }
        read_fuse(w, e, t)
    }
}

fn byte(t: &Tag) -> bool {
    matches!(t, Tag::Byte(_))
}
fn compound(t: &Tag) -> bool {
    matches!(t, Tag::Compound(_))
}
fn double(t: &Tag) -> bool {
    matches!(t, Tag::Double(_))
}
fn int_array(t: &Tag) -> bool {
    matches!(t, Tag::IntArray(_))
}
fn riders(t: &Tag) -> bool {
    matches!(t, Tag::List(l) if l.iter().all(|c| matches!(c, Tag::Compound(_))))
}

/// `Stand` bit `bit`: the byte the jar writes as 0 or 1.
fn set_bit(w: &mut World, e: Entity, bit: u8, on: bool) {
    if let Some(mut s) = w.get_mut::<Stand>(e) {
        s.flags = if on { s.flags | bit } else { s.flags & !bit };
    }
}

fn bit_tag(w: &World, e: Entity, bit: u8) -> Option<Tag> {
    w.get::<Stand>(e).map(|s| Tag::Byte(i8::from(s.flags & bit != 0)))
}

macro_rules! stand_bits {
    ($($write:ident, $read:ident, $bit:ident, $key:literal;)*) => { $(
        fn $write(w: &World, e: Entity, c: &mut Compound) {
            if let Some(t) = bit_tag(w, e, $bit) {
                c.insert($key.into(), t);
            }
        }
        fn $read(w: &mut World, e: Entity, t: &Tag) -> bool {
            let Tag::Byte(v) = t else { return false };
            set_bit(w, e, $bit, *v != 0);
            true
        }
    )* };
}

stand_bits! {
    write_small, read_small, SMALL, "Small";
    write_show_arms, read_show_arms, SHOW_ARMS, "ShowArms";
    write_no_base_plate, read_no_base_plate, NO_BASE_PLATE, "NoBasePlate";
    write_invisible, read_invisible, INVISIBLE, "Invisible";
}

/// `Marker`: written only when set, as the jar does.
fn write_marker(w: &World, e: Entity, c: &mut Compound) {
    if w.get::<Stand>(e).is_some_and(|s| s.flags & MARKER != 0) {
        c.insert("Marker".into(), Tag::Byte(1));
    }
}

fn read_marker(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    set_bit(w, e, MARKER, *v != 0);
    true
}

/// `Pose`: a compound of the parts that differ from the default, as `ArmorStand.addAdditionalSaveData` writes them.
fn write_pose(w: &World, e: Entity, c: &mut Compound) {
    // Always written for a stand (the game stores the codec's compound, empty for the default pose).
    if w.get::<Stand>(e).is_none() {
        return;
    }
    let pose = crate::stand::pose_of(w, e);
    let parts: Vec<(&str, Tag)> = POSE_KEYS.into_iter().zip(pose).zip(DEFAULT_POSE).filter(|((_, r), d)| r != d).map(|((k, r), _)| (k, floats(&r))).collect();
    c.insert("Pose".into(), Tag::compound(parts));
}

const POSE_KEYS: [&str; 6] = ["Head", "Body", "LeftArm", "RightArm", "LeftLeg", "RightLeg"];

fn floats(v: &[f32; 3]) -> Tag {
    Tag::List(v.iter().map(|&x| Tag::Float(if x == 0.0 { 0 } else { x.to_bits() })).collect())
}

fn read_pose(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Compound(c) = t else { return false };
    let mut pose = DEFAULT_POSE;
    for (key, part) in POSE_KEYS.into_iter().zip(pose.iter_mut()) {
        let Some(v) = c.get(&JavaString::from(key)) else { continue };
        let Tag::List(l) = v else { return false };
        let [Tag::Float(x), Tag::Float(y), Tag::Float(z)] = l.as_slice() else { return false };
        *part = [f32::from_bits(*x), f32::from_bits(*y), f32::from_bits(*z)];
    }
    set_pose(w, e, pose);
    true
}

fn write_disabled(w: &World, e: Entity, c: &mut Compound) {
    if let Some(d) = w.get::<DisabledSlots>(e) {
        c.insert("DisabledSlots".into(), Tag::Int(d.0));
    }
}

fn read_disabled(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    w.entity_mut(e).insert(DisabledSlots(*v));
    true
}

fn write_item(w: &World, e: Entity, c: &mut Compound) {
    if let Some(item) = w.get::<Held>(e).and_then(|h| h.stack.as_ref()) {
        c.insert("Item".into(), item_to_nbt(item));
    }
}

fn read_item(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(item) = item_from_nbt(t) else { return false };
    let Some(mut h) = w.get_mut::<Held>(e) else { return false };
    h.stack = Some(item);
    true
}

fn write_rotation(w: &World, e: Entity, c: &mut Compound) {
    if let Some(h) = w.get::<Held>(e) {
        c.insert("ItemRotation".into(), Tag::Byte(h.rotation as i8));
    }
}

fn read_rotation(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut h) = w.get_mut::<Held>(e) else { return false };
    h.rotation = v.rem_euclid(8) as u8;
    true
}

fn write_facing(w: &World, e: Entity, c: &mut Compound) {
    if let Some(a) = w.get::<Attached>(e) {
        c.insert("Facing".into(), Tag::Byte(a.face as i8));
    }
}

fn read_facing(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Ok(face) = u8::try_from(*v) else { return false };
    let Some(mut a) = w.get_mut::<Attached>(e) else { return false };
    if face >= 6 {
        return false;
    }
    a.face = face;
    true
}

fn write_block_pos(w: &World, e: Entity, c: &mut Compound) {
    if let Some(a) = w.get::<Attached>(e) {
        c.insert("block_pos".into(), Tag::IntArray(a.pos.to_vec()));
    }
}

fn read_block_pos(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::IntArray(v) = t else { return false };
    let Ok(pos) = <[i32; 3]>::try_from(v.as_slice()) else { return false };
    let Some(mut a) = w.get_mut::<Attached>(e) else { return false };
    a.pos = pos;
    true
}

fn write_push_x(w: &World, e: Entity, c: &mut Compound) {
    if let Some(f) = w.get::<Furnace>(e) {
        c.insert("PushX".into(), Tag::Double(f.push.x.to_bits()));
    }
}

fn read_push_x(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Double(v) = t else { return false };
    let Some(mut f) = w.get_mut::<Furnace>(e) else { return false };
    f.push.x = f64::from_bits(*v);
    true
}

fn write_push_z(w: &World, e: Entity, c: &mut Compound) {
    if let Some(f) = w.get::<Furnace>(e) {
        c.insert("PushZ".into(), Tag::Double(f.push.z.to_bits()));
    }
}

fn read_push_z(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Double(v) = t else { return false };
    let Some(mut f) = w.get_mut::<Furnace>(e) else { return false };
    f.push.z = f64::from_bits(*v);
    true
}

fn write_fuel(w: &World, e: Entity, c: &mut Compound) {
    if let Some(f) = w.get::<Furnace>(e) {
        c.insert("Fuel".into(), Tag::Short(f.fuel as i16));
    }
}

fn read_fuel(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Short(v) = t else { return false };
    let Some(mut f) = w.get_mut::<Furnace>(e) else { return false };
    f.fuel = i32::from(*v);
    true
}

fn write_fuse(w: &World, e: Entity, c: &mut Compound) {
    if let Some(f) = w.get::<Fuse>(e) {
        c.insert("fuse".into(), Tag::Int(f.0));
    }
}

fn read_fuse(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let Some(mut f) = w.get_mut::<Fuse>(e) else { return false };
    f.0 = *v;
    true
}

/// The creature riders of `e` in save order: its seats' riders, then those of its `Mount`.
pub fn passengers_of(w: &World, e: Entity) -> Vec<Entity> {
    let mut riders = seat::riders_of(w, e);
    if let Some(m) = w.get::<Mount>(e) {
        for who in [m.rider, m.passenger].into_iter().flatten() {
            if let Who::Entity(r) = who && !riders.contains(&r) {
                riders.push(r);
            }
        }
    }
    riders
}

/// The creature riders of a vehicle (its seats, or its mount's riders), each as its own compound (`save`: their keys and theirs,
/// recursively). A player is not written: it is saved with its player data.
fn write_passengers(w: &World, e: Entity, c: &mut Compound) {
    let list: Vec<Tag> = passengers_of(w, e).into_iter().filter_map(|r| crate::nbt::save(w, r).map(Tag::Compound)).collect();
    if !list.is_empty() {
        c.insert("Passengers".into(), Tag::List(list));
    }
}

/// Restores each rider compound as an entity and seats it in the vehicle's seats (or its mount): a rider that is refused is not seated.
fn read_passengers(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::List(list) = t else { return false };
    let seated = crate::seat::seats_of(w, e).is_some();
    for c in list {
        let Tag::Compound(c) = c else { continue };
        if let Some(r) = crate::nbt::restore(w, c) {
            if seated {
                seat::board(w, r, e, None);
            } else {
                crate::mount::ride(w, e, Who::Entity(r));
            }
        }
    }
    true
}
