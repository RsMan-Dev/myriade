//! The saved states of a living creature that sit beside its physics (`LivingEntity` and `Entity` keys, lot 86): absorption, attribute
//! modifiers, the worn and held stacks, the drop chances, the name, gravity, the glide and the hand. Each key is written only when its
//! component exists, so a creature without state writes none of them (the living defaults of `common` are the game's own).
use crate::absorption::{self, Absorption};
use crate::attr_mod::{AttrMods, Modifier, Operation};
use crate::custom_name::{self, CustomName, NameVisible};
use crate::equip_stack::{Equipment, SLOTS};
use crate::equipment_drop::{DropChances, DEFAULT};
use crate::gliding::{self, FallFlying};
use crate::left_handed::{self, LeftHanded};
use crate::nbt::{accept, owner, Field};
use crate::no_gravity::{self, NoGravity};
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, JavaString, Tag};
use horizon_items::item_nbt::{item_from_nbt, item_to_nbt};
use horizon_client_lib::items::ItemStack;
use std::borrow::Cow;

/// No key of this module applies to an entity holding a component: every key poses its own.
pub const FIELDS: &[Field] = &[];

/// The keys whose presence poses their component (`nbt::register_creators`).
pub const CREATORS: &[Field] = &[
    Field { key: Cow::Borrowed("AbsorptionAmount"), write: write_absorption, read: read_absorption, accepts: accept::float, owner: Some(owner::<Absorption>) },
    Field { key: Cow::Borrowed("attributes"), write: write_attributes, read: read_attributes, accepts: is_list, owner: Some(owner::<AttrMods>) },
    Field { key: Cow::Borrowed("equipment"), write: write_equipment, read: read_equipment, accepts: is_compound, owner: Some(owner::<Equipment>) },
    Field { key: Cow::Borrowed("drop_chances"), write: write_drop_chances, read: read_drop_chances, accepts: is_compound, owner: Some(owner::<DropChances>) },
    Field { key: Cow::Borrowed("CustomName"), write: write_name, read: read_name, accepts: accept_name, owner: Some(owner::<CustomName>) },
    Field { key: Cow::Borrowed("CustomNameVisible"), write: write_name_visible, read: read_name_visible, accepts: is_byte, owner: Some(owner::<NameVisible>) },
    Field { key: Cow::Borrowed("NoGravity"), write: write_no_gravity, read: read_no_gravity, accepts: is_byte, owner: Some(owner::<NoGravity>) },
    Field { key: Cow::Borrowed("FallFlying"), write: write_fall_flying, read: read_fall_flying, accepts: is_byte, owner: Some(owner::<FallFlying>) },
    Field { key: Cow::Borrowed("LeftHanded"), write: write_left_handed, read: read_left_handed, accepts: is_byte, owner: Some(owner::<LeftHanded>) },
];

/// The keys of the worn and held stacks, in the order of `equip_stack::SLOTS`.
const SLOT_KEYS: [&str; SLOTS] = ["mainhand", "offhand", "head", "chest", "legs", "feet", "body", "saddle"];
/// The keys of the drop chances, in the order of `DropChances` (its index: main, off, feet, legs, chest, head, body, saddle).
const DROP_KEYS: [&str; SLOTS] = ["mainhand", "offhand", "feet", "legs", "chest", "head", "body", "saddle"];
const MINECRAFT: &str = "minecraft:";

fn is_byte(t: &Tag) -> bool {
    matches!(t, Tag::Byte(_))
}
fn is_list(t: &Tag) -> bool {
    matches!(t, Tag::List(_))
}
fn is_compound(t: &Tag) -> bool {
    matches!(t, Tag::Compound(_))
}
fn get<'a>(c: &'a Compound, k: &str) -> Option<&'a Tag> {
    c.get(&JavaString::from(k))
}

/// `AbsorptionAmount`: a float, from `Absorption` (absent at 0).
fn write_absorption(w: &World, e: Entity, c: &mut Compound) {
    if let Some(a) = w.get::<Absorption>(e) {
        c.insert("AbsorptionAmount".into(), Tag::Float(a.0.to_bits()));
    }
}
fn read_absorption(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Float(b) = t else { return false };
    absorption::set(w, e, f32::from_bits(*b));
    true
}

/// `attributes`: a list of { id, base (omitted when the species' base applies), modifiers: [{ id, amount, operation }] }, the entries of
/// `AttrMods` only.
fn write_attributes(w: &World, e: Entity, c: &mut Compound) {
    let Some(a) = w.get::<AttrMods>(e) else { return };
    let list = a
        .0
        .iter()
        .filter(|(_, base, mods)| !base.is_nan() || mods.iter().any(|md| md.permanent))
        .map(|(name, base, mods)| {
            let mut m = Compound::default();
            m.insert("id".into(), Tag::String(format!("{MINECRAFT}{name}").as_str().into()));
            if !base.is_nan() {
                m.insert("base".into(), Tag::Double(base.to_bits()));
            }
            let mods = mods.iter().filter(|md| md.permanent).map(|md| modifier_tag(md)).collect();
            m.insert("modifiers".into(), Tag::List(mods));
            Tag::Compound(m)
        })
        .collect();
    c.insert("attributes".into(), Tag::List(list));
}

fn modifier_tag(md: &Modifier) -> Tag {
    let op = match md.operation {
        Operation::AddValue => "add_value",
        Operation::AddMultipliedBase => "add_multiplied_base",
        Operation::AddMultipliedTotal => "add_multiplied_total",
    };
    let mut m = Compound::default();
    m.insert("id".into(), Tag::String(md.id.as_str().into()));
    m.insert("amount".into(), Tag::Double(md.amount.to_bits()));
    m.insert("operation".into(), Tag::String(op.into()));
    Tag::Compound(m)
}

fn modifier_of(t: &Tag) -> Option<Modifier> {
    let Tag::Compound(c) = t else { return None };
    let (Some(Tag::String(id)), Some(Tag::Double(amount)), Some(Tag::String(op))) = (get(c, "id"), get(c, "amount"), get(c, "operation")) else {
        return None;
    };
    let operation = match op.to_string_lossy().as_str() {
        "add_value" => Operation::AddValue,
        "add_multiplied_base" => Operation::AddMultipliedBase,
        "add_multiplied_total" => Operation::AddMultipliedTotal,
        _ => return None,
    };
    Some(Modifier { id: id.to_string_lossy(), amount: f64::from_bits(*amount), operation, permanent: true })
}

/// A refused list changes nothing; an empty one takes the component away.
fn read_attributes(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::List(items) = t else { return false };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let Tag::Compound(c) = item else { return false };
        let Some(Tag::String(id)) = get(c, "id") else { return false };
        let id = id.to_string_lossy();
        let name = id.strip_prefix(MINECRAFT).unwrap_or(&id).to_string();
        let base = match get(c, "base") {
            None => f64::NAN,
            Some(Tag::Double(b)) => f64::from_bits(*b),
            Some(_) => return false,
        };
        let mods: &[Tag] = match get(c, "modifiers") {
            None => &[],
            Some(Tag::List(l)) => l,
            Some(_) => return false,
        };
        let Some(mods) = mods.iter().map(modifier_of).collect::<Option<Vec<_>>>() else { return false };
        out.push((name, base, mods));
    }
    if out.is_empty() {
        w.entity_mut(e).remove::<AttrMods>();
    } else {
        let names: Vec<String> = out.iter().map(|x| x.0.clone()).collect();
        w.entity_mut(e).insert(AttrMods(out));
        for n in names {
            crate::attr_mod::touched(w, e, &n);
        }
    }
    true
}

/// `equipment`: a compound with a stack per worn or held slot that holds one, the stack as `item_to_nbt` writes it.
fn write_equipment(w: &World, e: Entity, c: &mut Compound) {
    let Some(q) = w.get::<Equipment>(e) else { return };
    let mut m = Compound::default();
    for (key, stack) in SLOT_KEYS.iter().zip(&q.slots) {
        if let Some(s) = stack {
            m.insert((*key).into(), item_to_nbt(s));
        }
    }
    c.insert("equipment".into(), Tag::Compound(m));
}

/// A refused stack changes nothing; the component goes with the last piece (as `equip_stack::set`).
fn read_equipment(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Compound(c) = t else { return false };
    let mut slots: [Option<ItemStack>; SLOTS] = Default::default();
    for (key, slot) in SLOT_KEYS.iter().zip(slots.iter_mut()) {
        if let Some(v) = get(c, key) {
            let Some(stack) = item_from_nbt(v) else { return false };
            *slot = Some(stack);
        }
    }
    if slots.iter().all(Option::is_none) {
        w.entity_mut(e).remove::<Equipment>();
        w.entity_mut(e).remove::<crate::equip_attrs::WornModifiers>();
    } else {
        for (i, s) in slots.iter().enumerate() {
            crate::equip_attrs::changed(w, e, i, s.as_ref());
        }
        w.entity_mut(e).insert(Equipment { slots });
    }
    true
}

/// `drop_chances`: a compound of the floats that differ from `DEFAULT` only.
fn write_drop_chances(w: &World, e: Entity, c: &mut Compound) {
    let Some(d) = w.get::<DropChances>(e) else { return };
    if d.0.iter().all(|&v| v == DEFAULT) {
        return;
    }
    let mut m = Compound::default();
    for (key, v) in DROP_KEYS.iter().zip(d.0) {
        if v != DEFAULT {
            m.insert((*key).into(), Tag::Float(v.to_bits()));
        }
    }
    c.insert("drop_chances".into(), Tag::Compound(m));
}

fn read_drop_chances(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Compound(c) = t else { return false };
    let mut chances = [DEFAULT; SLOTS];
    for (key, v) in DROP_KEYS.iter().zip(chances.iter_mut()) {
        match get(c, key) {
            None => {}
            Some(Tag::Float(b)) => *v = f32::from_bits(*b),
            Some(_) => return false,
        }
    }
    if chances.iter().all(|&v| v == DEFAULT) {
        w.entity_mut(e).remove::<DropChances>();
    } else {
        w.entity_mut(e).insert(DropChances(chances));
    }
    true
}

/// `CustomName`: the whole text component, a string or a styled compound, as the host gave it.
fn write_name(w: &World, e: Entity, c: &mut Compound) {
    if let Some(t) = custom_name::tag(w, e) {
        c.insert("CustomName".into(), t);
    }
}
fn read_name(w: &mut World, e: Entity, t: &Tag) -> bool {
    match t {
        Tag::String(s) => custom_name::set(w, e, Some(s.to_string_lossy())),
        Tag::Compound(_) => custom_name::set_component(w, e, t.clone()),
        _ => return false,
    }
    true
}
fn accept_name(t: &Tag) -> bool {
    matches!(t, Tag::String(_) | Tag::Compound(_))
}

/// `CustomNameVisible`: a byte, 1 while the marker is there.
fn write_name_visible(w: &World, e: Entity, c: &mut Compound) {
    if w.get::<NameVisible>(e).is_some() {
        c.insert("CustomNameVisible".into(), Tag::Byte(1));
    }
}
fn read_name_visible(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    custom_name::set_visible(w, e, *v != 0);
    true
}

/// `NoGravity`: a byte, 1 while the marker is there (a boat keeps its own `NoGravity` in `Hull`).
fn write_no_gravity(w: &World, e: Entity, c: &mut Compound) {
    if w.get::<NoGravity>(e).is_some() {
        c.insert("NoGravity".into(), Tag::Byte(1));
    }
}
fn read_no_gravity(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    no_gravity::set(w, e, *v != 0);
    true
}

/// `FallFlying`: a byte, 1 while the glide runs. Its tick is the time of the load (`gliding::set`).
fn write_fall_flying(w: &World, e: Entity, c: &mut Compound) {
    if w.get::<FallFlying>(e).is_some() {
        c.insert("FallFlying".into(), Tag::Byte(1));
    }
}
fn read_fall_flying(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    gliding::set(w, e, *v != 0);
    true
}

/// `LeftHanded`: a byte, 1 while the marker is there.
fn write_left_handed(w: &World, e: Entity, c: &mut Compound) {
    if w.get::<LeftHanded>(e).is_some() {
        c.insert("LeftHanded".into(), Tag::Byte(1));
    }
}
fn read_left_handed(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    left_handed::set(w, e, *v != 0);
    true
}
