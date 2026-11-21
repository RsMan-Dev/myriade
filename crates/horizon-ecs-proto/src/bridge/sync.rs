//! The synched data of a bridged entity (greffe L0, lot 04): the entries the tracker sends, read from the components when asked.
//! `sync_entries` gives the core's slots (`sync_core`, indices 0..=15) then the species' (`sync_a`, `sync_b`). A slot gives an
//! entry only when its value differs from the default; a missing source component gives none. Nothing is kept here: the last
//! state sent lives on the tracker's side (lot 08).
use super::{sync_a, sync_b, sync_core, view};
use bevy_ecs::prelude::*;
use horizon_core::nbt::Tag;
use horizon_protocol::entity_data::{Particle, Profile, Value, serializer};
use horizon_protocol::item::Stack;
use std::collections::HashMap;
use std::sync::OnceLock;

/// One entry's value, with what the packet needs to write it. `Floats` and `Id` carry their serializer: several share a shape.
#[derive(Clone, Debug, PartialEq)]
pub enum SyncValue {
    Byte(i8),
    Int(i32),
    Long(i64),
    Float(f32),
    String(String),
    Text(Tag),
    OptText(Option<Tag>),
    Item(Option<Stack>),
    Bool(bool),
    /// (serializer, values): ROTATIONS, VECTOR3 or QUATERNION.
    Floats(i32, Vec<f32>),
    Pos([i32; 3]),
    OptPos(Option<[i32; 3]>),
    OptUuid(Option<u128>),
    /// (serializer, VarInt): pose, direction, block state, variants...
    Id(i32, i32),
    Ids(Vec<i32>),
    Particle(Particle),
    Particles(Vec<Particle>),
    Profile(Profile),
}

/// The serializer of `v` and its value, as the entry list carries them.
pub fn to_entry(v: &SyncValue) -> (i32, Value) {
    use serializer as s;
    match v {
        SyncValue::Byte(b) => (s::BYTE, Value::Byte(*b)),
        SyncValue::Int(i) => (s::INT, Value::Int(*i)),
        SyncValue::Long(l) => (s::LONG, Value::Long(*l)),
        SyncValue::Float(f) => (s::FLOAT, Value::Float(*f)),
        SyncValue::String(t) => (s::STRING, Value::String(t.clone())),
        SyncValue::Text(t) => (s::COMPONENT, Value::Text(t.clone())),
        SyncValue::OptText(t) => (s::OPTIONAL_COMPONENT, Value::OptText(t.clone())),
        SyncValue::Item(t) => (s::ITEM_STACK, Value::Item(t.clone())),
        SyncValue::Bool(b) => (s::BOOLEAN, Value::Bool(*b)),
        SyncValue::Floats(k, f) => (*k, Value::Floats(f.clone())),
        SyncValue::Pos(p) => (s::BLOCK_POS, Value::Pos(*p)),
        SyncValue::OptPos(p) => (s::OPTIONAL_BLOCK_POS, Value::OptPos(*p)),
        SyncValue::OptUuid(u) => (s::OPTIONAL_LIVING_ENTITY_REFERENCE, Value::OptUuid(*u)),
        SyncValue::Id(k, i) => (*k, Value::Id(*i)),
        SyncValue::Ids(v) => (s::VILLAGER_DATA, Value::Ids(v.clone())),
        SyncValue::Particle(p) => (s::PARTICLE, Value::Particle(p.clone())),
        SyncValue::Particles(p) => (s::PARTICLES, Value::Particles(p.clone())),
        SyncValue::Profile(p) => (s::RESOLVABLE_PROFILE, Value::Profile(p.clone())),
    }
}

/// One entry the tracker may send: its index, and its value now (None: no entry, the default or no source component).
#[derive(Clone, Copy)]
pub struct SyncSlot {
    pub index: u8,
    pub read: fn(&World, Entity) -> Option<SyncValue>,
}

/// The species' slots for the registry id `kind` (flattened inheritance, built once from `sync_a::TABLE` then `sync_b::TABLE`);
/// an unknown species has none.
pub fn species_slots(kind: &str) -> &'static [SyncSlot] {
    static TABLES: OnceLock<HashMap<&'static str, &'static [SyncSlot]>> = OnceLock::new();
    TABLES
        .get_or_init(|| {
            let mut all: HashMap<&'static str, &'static [SyncSlot]> = sync_a::TABLE.iter().chain(sync_b::TABLE).copied().collect();
            all.extend(crate::species::boats::species().map(|(id, _, _)| (id, super::sync_boats::SLOTS)));
            all
        })
        .get(kind)
        .copied()
        .unwrap_or(&[])
}

/// Pushes the entries of `e` into `out` (not cleared): the core's slots, then its species'. Nothing for an unknown or sleeping
/// entity (`view` is None).
pub fn sync_entries(world: &World, e: Entity, out: &mut Vec<(u8, SyncValue)>) {
    let Some(v) = view::view(world, e) else {
        return;
    };
    for slot in sync_core::SLOTS.iter().chain(species_slots(v.kind)) {
        if let Some(value) = (slot.read)(world, e) {
            out.push((slot.index, value));
        }
    }
}
