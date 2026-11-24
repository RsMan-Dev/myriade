//! The NBT of an entity, composed on demand. A component declares its keys as `Field`s in its world's `NbtFields`. A key may have several
//! fields, one per kind of entity: the one that applies to an entity is the one whose component it holds. `save` composes the compound
//! from the applicable fields and the free keys, `load` and `merge` distribute a compound back, in declaration order.
//! An entity has an NBT while it holds `Saved`. `NbtData` keeps only the free keys (those no applicable field declares), and exists only
//! while there is one. The `id` of an entity is its `TypeName`, declared with a `Recipe` (`declare`).
use crate::cow::{Air, Health, Pos, TypeName, Vel};
use crate::projectile::Flight;
use crate::mount::{Mount, Riding};
use crate::targeting::Who;
use bevy_ecs::{archetype::{Archetype, ArchetypeId}, component::ComponentId, prelude::*};
use horizon_core::{
    geometry::Vec3,
    nbt::{Compound, JavaString, Tag},
};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::{Arc, Mutex},
};

/// The entity is saved: it has an NBT, and its free keys and tags exist. Set at birth from `is_saved` of its identifier. Zero bytes.
/// Removing it removes the `NbtData` and the tags of the entity.
#[derive(Component, Default, Debug)]
#[component(storage = "Table", on_remove = clear_saved)]
pub struct Saved;

/// Queued, as the entity may be despawned by now: when it is alive and not `Saved` again, its free keys and tags go.
fn clear_saved(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let e = ctx.entity;
    world.commands().queue(move |w: &mut World| {
        if w.get_entity(e).is_ok() && w.get::<Saved>(e).is_none() {
            w.entity_mut(e).remove::<NbtData>();
            crate::nbt_tag::clear(w, e);
        }
    });
}

/// The free keys of an entity, as loaded. Present only while there is one.
#[derive(Component, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct NbtData(pub Box<Compound>);

/// A declared key and the component that owns it: `owner` registers that component and gives its id, and the field applies to an entity
/// whose archetype has it. No owner: the key of every `Saved` entity. `accepts` tests a tag before any change (`merge` checks the whole
/// patch with it first). `write` adds the key when the field applies, nothing otherwise; `read` sets the component from the tag, or gives
/// false and changes nothing when the tag is refused.
#[derive(Clone)]
pub struct Field {
    pub key: Cow<'static, str>,
    pub owner: Option<fn(&mut World) -> ComponentId>,
    pub accepts: fn(&Tag) -> bool,
    pub write: fn(&World, Entity, &mut Compound),
    pub read: fn(&mut World, Entity, &Tag) -> bool,
}

/// The tags `accepts` takes, as the vanilla types of the keys.
pub mod accept {
    use horizon_core::nbt::Tag;
    pub fn any(_: &Tag) -> bool {
        true
    }
    pub fn int(t: &Tag) -> bool {
        matches!(t, Tag::Int(_))
    }
    pub fn long(t: &Tag) -> bool {
        matches!(t, Tag::Long(_))
    }
    pub use super::short_of;
    pub fn short(t: &Tag) -> bool {
        matches!(t, Tag::Short(_))
    }
    /// Any number: `getShortOr` narrows it (`short_of`).
    pub fn numeric(t: &Tag) -> bool {
        short_of(t).is_some()
    }
    pub fn float_or_short(t: &Tag) -> bool {
        matches!(t, Tag::Float(_) | Tag::Short(_))
    }
    pub fn float(t: &Tag) -> bool {
        matches!(t, Tag::Float(_))
    }
    pub fn string(t: &Tag) -> bool {
        matches!(t, Tag::String(_))
    }
    /// Three doubles: a position, a motion.
    pub fn vec3(t: &Tag) -> bool {
        matches!(t, Tag::List(l) if matches!(l.as_slice(), [Tag::Double(_), Tag::Double(_), Tag::Double(_)]))
    }
}

/// A free key changed by `load`, `merge` or `remove`: sent once per key. Keys a field applies to never send it.
#[derive(Event, Clone, Debug)]
pub struct NbtChanged {
    pub entity: Entity,
    pub key: String,
}

/// The declared fields of a world, in declaration order, with the id of their owner; the fields of each key; and per archetype, the
/// fields that apply to it (computed once, then read by `save` and `distribute`).
#[derive(Resource, Default)]
pub struct NbtFields {
    fields: Vec<(Field, Option<ComponentId>)>,
    keys: BTreeMap<String, Vec<usize>>,
    /// The creators (`register_creators`): never applicable, only used to give a free key back to its component.
    creators: Vec<Field>,
    cache: Mutex<HashMap<ArchetypeId, Arc<[usize]>>>,
}

impl NbtFields {
    fn owns(&self, arch: &Archetype, i: usize) -> bool {
        match self.fields[i].1 {
            Some(id) => arch.contains(id),
            None => true,
        }
    }
    /// The creator of `key` when exactly one creator declares it; none when there are two (the key stays free).
    fn creator_of(&self, key: &str) -> Option<&Field> {
        let mut found = self.creators.iter().filter(|f| f.key.as_ref() == key);
        let first = found.next()?;
        if found.next().is_some() {
            return None;
        }
        Some(first)
    }
}

/// The owner of a field: the component `T`, registered in the world.
pub fn owner<T: Component>(w: &mut World) -> ComponentId {
    w.register_component::<T>()
}

/// Declares `fields` in the world's registry, after the ones already declared. Forgets the archetype cache.
pub fn register(w: &mut World, fields: &[Field]) {
    register_with(w, fields, false);
}

/// Declares `fields` as creators: a key that no applicable field declares is given to the only creator of that key, whose `read` sets the
/// component back (`restore` of a component added in play). Two creators of one key: the key stays free.
pub fn register_creators(w: &mut World, fields: &[Field]) {
    register_with(w, fields, true);
}

fn register_with(w: &mut World, fields: &[Field], creators: bool) {
    w.init_resource::<NbtFields>();
    let ids: Vec<Option<ComponentId>> = if creators { Vec::new() } else { fields.iter().map(|f| f.owner.map(|o| o(w))).collect() };
    {
        let mut res = w.resource_mut::<NbtFields>();
        let reg = &mut *res;
        if creators {
            reg.creators.extend(fields.iter().cloned());
        }
        for (f, id) in fields.iter().zip(ids).filter(|_| !creators) {
            reg.keys.entry(f.key.to_string()).or_default().push(reg.fields.len());
            reg.fields.push((f.clone(), id));
        }
        reg.cache.get_mut().unwrap().clear();
    }
    // A late declaration claims the free keys of the entities that have some: only at registration, never per tick.
    let holders: Vec<Entity> = w.query_filtered::<Entity, With<NbtData>>().iter(w).collect();
    for e in holders {
        let Some(free) = w.get::<NbtData>(e).map(|t| (*t.0).clone()) else { continue };
        if free.keys().any(|k| fields.iter().any(|f| f.key.as_ref() == k.to_string_lossy())) {
            distribute(w, e, &free);
        }
    }
}

/// Declares one field at run time (a mod's component).
pub fn register_owned(w: &mut World, field: Field) {
    register(w, std::slice::from_ref(&field));
}

fn archetype(w: &World, e: Entity) -> Option<&Archetype> {
    let id = w.entities().get(e).ok().flatten()?.archetype_id;
    w.archetypes().get(id)
}

/// The field of `key` that applies to `e`. Two that apply to one entity are a mistake of the declarations: a panic.
fn applicable(w: &World, e: Entity, key: &str) -> Option<Field> {
    let reg = w.get_resource::<NbtFields>()?;
    let arch = archetype(w, e)?;
    let mut found: Option<Field> = None;
    for &i in reg.keys.get(key)?.iter().filter(|&&i| reg.owns(arch, i)) {
        assert!(found.is_none(), "NBT key `{key}` has two fields on one entity");
        found = Some(reg.fields[i].0.clone());
    }
    found
}

/// The indices of the fields that apply to `e`, in declaration order, from the cache of its archetype: computed once per archetype, where
/// two fields of one key are a mistake of the declarations (a panic).
fn applicable_indices(w: &World, e: Entity) -> Arc<[usize]> {
    let (Some(reg), Some(arch)) = (w.get_resource::<NbtFields>(), archetype(w, e)) else { return Arc::from(Vec::new()) };
    let mut cache = reg.cache.lock().unwrap();
    if let Some(v) = cache.get(&arch.id()) {
        return v.clone();
    }
    let list: Vec<usize> = (0..reg.fields.len()).filter(|&i| reg.owns(arch, i)).collect();
    let mut keys = BTreeSet::new();
    for &i in &list {
        let key = &reg.fields[i].0.key;
        assert!(keys.insert(key.as_ref()), "NBT key `{key}` has two fields on one entity");
    }
    let v: Arc<[usize]> = list.into();
    cache.insert(arch.id(), v.clone());
    v
}

fn custom(w: &World, e: Entity) -> Compound {
    w.get::<NbtData>(e).map_or_else(Compound::new, |t| (*t.0).clone())
}

/// The entity's compound: its free keys, then the keys of the applicable fields (a declared key wins), then the common keys left at their
/// game default (`DEFAULTS`, computed here and not stored). None when the entity is not `Saved`.
pub fn save(w: &World, e: Entity) -> Option<Compound> {
    if w.get::<Saved>(e).is_none() { return None; }
    let mut c = custom(w, e);
    // `Entity.saveWithoutId` writes `onGround()`: the contact of the last move, over any stale free key; a declared field (a boat's hull) wins below.
    if let Some(k) = w.get::<crate::cow::Contact>(e) {
        c.insert("OnGround".into(), Tag::Byte(i8::from(k.on_ground)));
    }
    if let Some(u) = w.get::<crate::bridge::uuids::EntityUuid>(e) {
        c.insert("UUID".into(), Tag::IntArray(crate::bridge::uuids::to_words(u.0).to_vec()));
    }
    if let Some(reg) = w.get_resource::<NbtFields>() {
        for &i in applicable_indices(w, e).iter() {
            (reg.fields[i].0.write)(w, e, &mut c);
        }
    }
    for d in crate::nbt_fields::common::tagged_of(w, e) {
        if c.get(&JavaString::from(d.key)).is_none() {
            c.insert(d.key.into(), (d.default)());
        }
    }
    for &(key, default) in crate::nbt_fields::common::DEFAULTS {
        if c.get(&JavaString::from(key)).is_none() {
            c.insert(key.into(), default());
        }
    }
    for (present, table) in [(w.get::<crate::cow::Health>(e).is_some() && !w.get::<crate::cow::Body>(e).is_some_and(|b| b.object), crate::nbt_fields::common::LIVING_DEFAULTS), (w.get::<crate::goals::GoalSet>(e).is_some(), crate::nbt_fields::common::MOB_DEFAULTS)] {
        if !present {
            continue;
        }
        for &(key, default) in table {
            if c.get(&JavaString::from(key)).is_none() {
                c.insert(key.into(), default());
            }
        }
    }
    Some(c)
}

/// One key of the entity's NBT, without composing the rest. None when the entity is not `Saved`.
pub fn get(w: &World, e: Entity, key: &str) -> Option<Tag> {
    if w.get::<Saved>(e).is_none() { return None; }
    if let Some(f) = applicable(w, e, key) {
        let mut c = Compound::new();
        (f.write)(w, e, &mut c);
        return c.remove(&JavaString::from(key));
    }
    w.get::<NbtData>(e)?.0.get(&JavaString::from(key)).cloned()
}

/// Distributes `c` over the entity in declaration order: the keys an applicable field declares go to it (a refused one is ignored, never
/// kept free), the other keys become the free keys, exactly. Returns the refused keys and the free keys whose value changed.
fn distribute(w: &mut World, e: Entity, c: &Compound) -> (usize, Vec<String>) {
    let idx = applicable_indices(w, e);
    let fields: Vec<Field> = w.get_resource::<NbtFields>().map_or_else(Vec::new, |reg| idx.iter().map(|&i| reg.fields[i].0.clone()).collect());
    let mut refused = 0;
    let mut taken = BTreeSet::new();
    for f in fields {
        let Some(v) = c.get(&JavaString::from(f.key.as_ref())) else { continue };
        taken.insert(f.key.to_string());
        if !(f.read)(w, e, v) {
            refused += 1;
        }
    }
    let creating: Vec<Field> = match w.get_resource::<NbtFields>() {
        Some(reg) => c.keys().map(|k| k.to_string_lossy()).filter(|k| !taken.contains(k)).filter_map(|k| reg.creator_of(&k).cloned()).collect(),
        None => Vec::new(),
    };
    for f in creating {
        let Some(v) = c.get(&JavaString::from(f.key.as_ref())) else { continue };
        taken.insert(f.key.to_string());
        if !(f.read)(w, e, v) {
            refused += 1;
        }
    }
    // A common key left at its game default is dropped, not kept free (no `NbtData` on every loaded entity).
    let free: Compound = c
        .iter()
        .filter(|(k, v)| {
            let key = k.to_string_lossy();
            !taken.contains(&key) && !crate::nbt_fields::common::drop_default_of(w, e, &key, v)
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let old = custom(w, e);
    let keys: BTreeSet<&JavaString> = old.keys().chain(free.keys()).collect();
    let changed = keys.into_iter().filter(|k| old.get(*k) != free.get(*k)).map(|k| k.to_string_lossy()).collect();
    if free.is_empty() {
        w.entity_mut(e).remove::<NbtData>();
    } else {
        w.entity_mut(e).insert(NbtData(Box::new(free)));
    }
    // The one funnel of the loads and `/data merge`: whatever a field wrote, the synchronized data are read again.
    crate::bridge::view::mark(w, e);
    (refused, changed)
}

fn send(w: &mut World, e: Entity, keys: Vec<String>) {
    for key in keys {
        w.trigger(NbtChanged { entity: e, key });
    }
}

/// Distributes `c` over the entity (see `distribute`). None when the entity is not `Saved`. Returns the number of declared keys refused: they are
/// ignored. Sends `NbtChanged` for each free key that changed.
pub fn load(w: &mut World, e: Entity, c: &Compound) -> Option<usize> {
    if w.get::<Saved>(e).is_none() { return None; }
    let (refused, changed) = distribute(w, e, c);
    crate::attr_mod::apply_load(w, e);
    crate::bridge::leash_save::load(w, e, c);
    // `Entity.load` poses `OnGround` as the contact (the mirror of `save`).
    if let Some(Tag::Byte(on)) = c.get(&JavaString::from("OnGround")) && let Some(mut k) = w.get_mut::<crate::cow::Contact>(e) {
        k.on_ground = *on != 0;
    }
    send(w, e, changed);
    Some(refused)
}

/// `/data merge`: the free keys merge into the entity's (compounds recursively), the declared keys are distributed. Refused, and nothing
/// changed, when not `Saved` or when a tag of the patch is one its field refuses: the whole patch is checked before any change. Sends
/// `NbtChanged` for each free key that changed.
pub fn merge(w: &mut World, e: Entity, patch: &Compound) -> bool {
    let Some(mut full) = save(w, e) else { return false };
    for (k, v) in patch {
        if applicable(w, e, &k.to_string_lossy()).is_some() {
            full.insert(k.clone(), v.clone());
        } else {
            merge_pair(&mut full, k, v);
        }
    }
    for (k, v) in &full {
        if applicable(w, e, &k.to_string_lossy()).is_some_and(|f| !(f.accepts)(v)) {
            return false;
        }
    }
    let (_, changed) = distribute(w, e, &full);
    send(w, e, changed);
    true
}

/// Removes a free key: false when there is none, when the entity is not `Saved`, or when a field applies to the key (its component owns it).
pub fn remove(w: &mut World, e: Entity, key: &str) -> bool {
    if w.get::<Saved>(e).is_none() {
        return false;
    }
    // The key a creaking's link owns: removing it unlinks the heart (the only declared key a removal is allowed on).
    if crate::nbt_fields::common::remove_heart_key(w, e, key) {
        return true;
    }
    if applicable(w, e, key).is_some() {
        return false;
    }
    let Some(mut free) = w.get::<NbtData>(e).map(|t| (*t.0).clone()) else { return false };
    if free.remove(&JavaString::from(key)).is_none() {
        return false;
    }
    if free.is_empty() {
        w.entity_mut(e).remove::<NbtData>();
    } else {
        w.entity_mut(e).insert(NbtData(Box::new(free)));
    }
    w.trigger(NbtChanged { entity: e, key: key.into() });
    true
}

/// Merges `v` under `k` of `dst`: compounds recursively, any other value replaces the old one.
fn merge_pair(dst: &mut Compound, k: &JavaString, v: &Tag) {
    if let (Some(Tag::Compound(d)), Tag::Compound(s)) = (dst.get_mut(k), v) {
        for (sk, sv) in s {
            merge_pair(d, sk, sv);
        }
        return;
    }
    dst.insert(k.clone(), v.clone());
}

/// The keys the common components declare: the world's builder registers them.
pub const COMMON: &[Field] = &[
    Field { key: Cow::Borrowed("Pos"), owner: Some(owner::<Pos>), accepts: accept::vec3, write: write_pos, read: read_pos },
    Field { key: Cow::Borrowed("Motion"), owner: Some(owner::<Vel>), accepts: accept::vec3, write: write_motion, read: read_motion },
    Field { key: Cow::Borrowed("Air"), owner: Some(owner::<Air>), accepts: accept::short, write: write_air, read: read_air },
    Field { key: Cow::Borrowed("Health"), owner: Some(owner::<Health>), accepts: accept::numeric, write: write_health, read: read_health },
    Field { key: Cow::Borrowed("id"), owner: None, accepts: accept::string, write: write_id, read: read_id },
];

fn write_pos(w: &World, e: Entity, c: &mut Compound) {
    let Some(p) = w.get::<Pos>(e) else { return };
    c.insert("Pos".into(), vec3_tag(p.0));
}

fn read_pos(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(v) = tag_vec3(t) else { return false };
    // The box follows the position (`Entity.setPos`): a restored species is built at height 1 and placed here.
    let old = w.get::<Pos>(e).map(|p| p.0);
    if let (Some(old), Some(b)) = (old, w.get::<crate::cow::Bounds>(e).map(|b| b.0)) {
        let d = Vec3::new(v.x - old.x, v.y - old.y, v.z - old.z);
        let (mn, mx) = (b.min_position(), b.max_position());
        let moved = horizon_core::geometry::Aabb::new(Vec3::new(mn.x + d.x, mn.y + d.y, mn.z + d.z), Vec3::new(mx.x + d.x, mx.y + d.y, mx.z + d.z));
        w.entity_mut(e).insert(crate::cow::Bounds(moved));
    }
    w.entity_mut(e).insert(Pos(v));
    true
}

/// `Motion`: three doubles, as `Vel`.
fn write_motion(w: &World, e: Entity, c: &mut Compound) {
    let Some(v) = w.get::<Vel>(e) else { return };
    c.insert("Motion".into(), vec3_tag(v.0));
}

fn read_motion(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(v) = tag_vec3(t) else { return false };
    w.entity_mut(e).insert(Vel(v));
    true
}

fn vec3_tag(v: Vec3) -> Tag {
    Tag::List([v.x, v.y, v.z].map(|c| Tag::Double(c.to_bits())).to_vec())
}

fn tag_vec3(t: &Tag) -> Option<Vec3> {
    let Tag::List(l) = t else { return None };
    let [Tag::Double(x), Tag::Double(y), Tag::Double(z)] = l.as_slice() else { return None };
    Some(Vec3::new(f64::from_bits(*x), f64::from_bits(*y), f64::from_bits(*z)))
}

/// `Air`: a short, the breath left (`Entity.saveWithoutId` writes it as a short).
fn write_air(w: &World, e: Entity, c: &mut Compound) {
    if let Some(a) = w.get::<Air>(e) {
        c.insert("Air".into(), Tag::Short(a.0 as i16));
    }
}

fn read_air(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Short(v) = t else { return false };
    w.entity_mut(e).insert(Air(i32::from(*v)));
    true
}

/// `NumericTag.shortValue` as `CompoundTag.getShortOr` reads it: an integer cut to 16 bits, a float or double floored to an int first.
pub fn short_of(t: &Tag) -> Option<i16> {
    Some(match t {
        Tag::Byte(v) => i16::from(*v),
        Tag::Short(v) => *v,
        Tag::Int(v) => *v as i16,
        Tag::Long(v) => *v as i16,
        Tag::Float(b) => f64::from(f32::from_bits(*b)).floor() as i32 as i16,
        Tag::Double(b) => f64::from_bits(*b).floor() as i32 as i16,
        _ => return None,
    })
}

fn write_health(w: &World, e: Entity, c: &mut Compound) {
    if let Some(h) = w.get::<Health>(e) {
        // An object (a dropped stack, an orb) saves its health as a short (`ItemEntity`, `ExperienceOrb`).
        let tag = if w.get::<crate::cow::Body>(e).is_some_and(|b| b.object) { Tag::Short(h.0 as i16) } else { Tag::Float(h.0.to_bits()) };
        c.insert("Health".into(), tag);
    }
}

fn read_health(w: &mut World, e: Entity, t: &Tag) -> bool {
    let object = w.get::<crate::cow::Body>(e).is_some_and(|b| b.object);
    let health = match t {
        _ if object => match short_of(t) {
            Some(v) => f32::from(v),
            None => return false,
        },
        Tag::Float(bits) => f32::from_bits(*bits),
        Tag::Short(v) => f32::from(*v),
        _ => return false,
    };
    w.entity_mut(e).insert(Health(health));
    true
}

/// The identifier of an entity: its `TypeName`, else the id of its `Flight` (a projectile carries no `TypeName`, so its bytes are not doubled).
pub fn id_of_entity(w: &World, e: Entity) -> Option<&'static str> {
    w.get::<TypeName>(e).map(|t| t.0).or_else(|| w.get::<Flight>(e).map(|f| f.id))
}

/// The `id` of an entity is its identifier (`id_of_entity`), the one it was declared as.
fn write_id(w: &World, e: Entity, c: &mut Compound) {
    if let Some(id) = id_of_entity(w, e) {
        c.insert("id".into(), Tag::String(JavaString::from(id)));
    }
}

/// The `id` is taken by an entity that has an identifier (read is taken and ignored).
fn read_id(w: &mut World, e: Entity, t: &Tag) -> bool {
    matches!(t, Tag::String(_)) && id_of_entity(w, e).is_some()
}

/// Sent on an entity before its NBT is composed by `save_all`.
#[derive(Event, Clone, Copy, Debug)]
pub struct Saving {
    pub entity: Entity,
}

/// Sent on an entity once `restore` has given it back its components and free keys.
#[derive(Event, Clone, Copy, Debug)]
pub struct Restored {
    pub entity: Entity,
}

/// Composes the NBT of every `Saved` entity, in query order: `each` gets the entity and its compound. `Saving` is sent on each entity first.
pub fn save_all(w: &mut World, mut each: impl FnMut(Entity, Compound)) {
    // A sleeping entity (`Disabled`) is saved like any other: it is kept in the world, only out of the systems.
    let saved: Vec<Entity> = w.query_filtered::<Entity, (With<Saved>, Allow<bevy_ecs::entity_disabling::Disabled>)>().iter(w).collect();
    for e in saved {
        // A `Saved` entity with no identifier is not saved.
        if written_by_mount(w, e) || id_of_entity(w, e).is_none() {
            continue;
        }
        w.trigger(Saving { entity: e });
        if let Some(c) = save(w, e) {
            each(e, c);
        }
    }
}

/// A rider of a saved mount or of a saved vehicle with seats: its vehicle writes it under `Passengers`, so `save_all` does not write it a
/// second time.
fn written_by_mount(w: &World, e: Entity) -> bool {
    if let Some(crate::seat::Seated { vehicle, .. }) = w.get::<crate::seat::Seated>(e).copied() {
        return w.get::<Saved>(vehicle).is_some() && crate::seat::riders_of(w, vehicle).contains(&e);
    }
    let Some(Riding(m)) = w.get::<Riding>(e).copied() else { return false };
    w.get::<Saved>(m).is_some() && w.get::<Mount>(m).is_some_and(|t| t.rider == Some(Who::Entity(e)) || t.passenger == Some(Who::Entity(e)))
}

/// Makes the entity of an identifier that needs its own construction (a dropped item, an orb, an arrow...): the position, the compound;
/// None when it makes nothing, and the compound is set aside.
pub type Builder = fn(&mut World, Vec3, &Compound) -> Option<Entity>;

/// How `restore` makes the entity of an identifier: as its species (`cow::spawn_species`), as a passive one (`passive::spawn`), or with
/// its constructor.
#[derive(Clone, Copy)]
pub enum Build {
    Species,
    Passive,
    With(Builder),
}

/// What an identifier is: `saved` (its entities carry `Saved` and keep their NBT) and how `restore` makes them.
#[derive(Clone, Copy)]
pub struct Recipe {
    pub saved: bool,
    pub build: Build,
}

/// The declared identifiers of the world, each with its recipe.
#[derive(Resource, Default)]
pub struct EntityTypes(BTreeMap<String, Recipe>);

/// The saved NBT of the declaration window, by id then chunk: built when their id is declared, dropped at `end_declarations`.
#[derive(Resource, Default)]
pub struct Pending {
    open: bool,
    map: BTreeMap<String, BTreeMap<(i32, i32), Vec<Compound>>>,
}

/// The chunks that hold an NBT whose id is not declared, as flags only: the caller rereads the chunk's file to keep those entities.
#[derive(Resource, Default)]
pub struct UnknownChunks(BTreeSet<(i32, i32)>);

fn recipe_of(w: &World, id: &str) -> Option<Recipe> {
    w.get_resource::<EntityTypes>()?.0.get(id).copied()
}

fn is_open(w: &World) -> bool {
    w.get_resource::<Pending>().is_some_and(|p| p.open)
}

fn chunk_of(at: Vec3) -> (i32, i32) {
    ((at.x / 16.0).floor() as i32, (at.z / 16.0).floor() as i32)
}

fn id_of(c: &Compound) -> Option<String> {
    match c.get(&JavaString::from("id"))? {
        Tag::String(s) => Some(s.to_string_lossy()),
        _ => None,
    }
}

fn mark(w: &mut World, chunk: (i32, i32)) {
    w.init_resource::<UnknownChunks>();
    w.resource_mut::<UnknownChunks>().0.insert(chunk);
}

/// Sets aside a compound `restore` cannot make: in the window it is kept pending; otherwise it is not kept, and its chunk is flagged.
fn set_aside(w: &mut World, id: &str, at: Vec3, c: &Compound) {
    if is_open(w) {
        w.resource_mut::<Pending>().map.entry(id.to_string()).or_default().entry(chunk_of(at)).or_default().push(c.clone());
    } else {
        mark(w, chunk_of(at));
    }
}

/// Whether chunk (cx, cz) holds an NBT whose id is not declared: flagged by `restore` or `end_declarations`, until `keep_unknown` or
/// `populate_unknown` finds nothing left, or `forget_chunk`. The caller reads this flag first: a chunk without it is not reread at all.
pub fn has_unknown(w: &World, cx: i32, cz: i32) -> bool {
    w.get_resource::<UnknownChunks>().is_some_and(|u| u.0.contains(&(cx, cz)))
}

/// Forgets the flag of a chunk that unloads.
pub fn forget_chunk(w: &mut World, cx: i32, cz: i32) {
    if let Some(mut u) = w.get_resource_mut::<UnknownChunks>() {
        u.0.remove(&(cx, cz));
    }
}

/// The compounds of `disk` that `want` accepts by their id, whole. A root it does not accept is searched in its `Passengers`: a rider
/// of a known mount is taken out on its own, at its own `Pos` (kept without loss, its seat is not restored).
fn gather(c: &Compound, want: &dyn Fn(&str) -> bool, out: &mut Vec<Compound>) {
    if id_of(c).is_some_and(|id| want(id.as_str())) {
        out.push(c.clone());
        return;
    }
    if let Some(Tag::List(list)) = c.get(&JavaString::from("Passengers")) {
        for t in list {
            if let Tag::Compound(p) = t {
                gather(p, want, out);
            }
        }
    }
}

/// Of the entities the caller rereads from chunk (cx, cz)'s file (`disk`), those whose id is still not declared, intact, to write back with
/// the chunk. Nothing left: the flag goes. The caller reads the file only when `has_unknown` says so, and before the next save of the chunk.
pub fn keep_unknown(w: &mut World, cx: i32, cz: i32, disk: impl Iterator<Item = Compound>) -> Vec<Compound> {
    let disk: Vec<Compound> = disk.collect();
    let kept = {
        let world: &World = w;
        let mut out = Vec::new();
        for c in &disk {
            gather(c, &|id: &str| !declared(world, id), &mut out);
        }
        out
    };
    if kept.is_empty() {
        forget_chunk(w, cx, cz);
    }
    kept
}

/// After `id` was declared late (`declare` returned chunk (cx, cz)), builds from the reread `disk` of that chunk the entities of this `id`
/// only: the other identifiers were declared at load, so their entities are alive already and are not built again. A failed build keeps the
/// flag. Returns how many entities it made.
pub fn populate_unknown(w: &mut World, cx: i32, cz: i32, id: &str, disk: impl Iterator<Item = Compound>) -> usize {
    let disk: Vec<Compound> = disk.collect();
    let mut wanted = Vec::new();
    for c in &disk {
        gather(c, &|k: &str| k == id, &mut wanted);
    }
    let (mut made, mut failed) = (0, false);
    for c in &wanted {
        match restore(w, c) {
            Some(_) => made += 1,
            None => failed = true,
        }
    }
    let left = {
        let world: &World = w;
        let mut out = Vec::new();
        for c in &disk {
            gather(c, &|k: &str| !declared(world, k), &mut out);
        }
        out
    };
    if left.is_empty() && !failed {
        forget_chunk(w, cx, cz);
    }
    made
}

/// Opens the declaration window: until `end_declarations`, an NBT whose id is not declared is kept pending and built when it is declared.
pub fn begin_declarations(w: &mut World) {
    w.init_resource::<Pending>();
    w.resource_mut::<Pending>().open = true;
}

/// Closes the window: the pending NBT are dropped, and their chunks are flagged for the caller to reread.
pub fn end_declarations(w: &mut World) {
    let Some(p) = w.remove_resource::<Pending>() else { return };
    let chunks: BTreeSet<(i32, i32)> = p.map.values().flat_map(|chunks| chunks.keys()).copied().collect();
    for chunk in chunks {
        mark(w, chunk);
    }
}

/// Declares `id` with its recipe, at any time. In the window, the pending NBT of this id are built now. Outside it, nothing was kept: the
/// flagged chunks are returned: the caller logs a warning naming `id` and the count (no `log` dependency here), then rereads each one and
/// calls `populate_unknown` BEFORE its next save (after that save, `keep_unknown` no longer keeps them: they are declared). The chunks are
/// the flagged ones of any id, not only of `id`.
pub fn declare(w: &mut World, id: &str, recipe: Recipe) -> Vec<(i32, i32)> {
    w.init_resource::<EntityTypes>();
    w.resource_mut::<EntityTypes>().0.insert(id.to_string(), recipe);
    if is_open(w) {
        let taken: Vec<Compound> = match w.get_resource_mut::<Pending>() {
            Some(mut p) => p.map.remove(id).map_or_else(Vec::new, |chunks| chunks.into_values().flatten().collect()),
            None => Vec::new(),
        };
        for c in taken {
            restore(w, &c);
        }
        return Vec::new();
    }
    w.get_resource::<UnknownChunks>().map_or_else(Vec::new, |u| u.0.iter().copied().collect())
}

/// Whether an entity of `id` is saved: what a creation reads to pose `Saved` in its one insert.
pub fn is_saved(w: &World, id: &str) -> bool {
    recipe_of(w, id).is_some_and(|r| r.saved)
}

/// Poses `Saved` on `e` when `id` is saved (a bundle cannot hold an optional component: it is one more insert at the creation).
pub fn mark_saved(w: &mut World, e: Entity, id: &str) {
    if is_saved(w, id) {
        w.entity_mut(e).insert(Saved);
    }
}

/// Every declared identifier with its recipe, in identifier order.
pub fn recipes(w: &World) -> Vec<(String, Recipe)> {
    w.get_resource::<EntityTypes>().map_or_else(Vec::new, |t| t.0.iter().map(|(k, r)| (k.clone(), *r)).collect())
}

/// Whether `id` is declared.
pub fn declared(w: &World, id: &str) -> bool {
    recipe_of(w, id).is_some()
}

/// Restores the entity a saved compound describes, at its `Pos`, made by the recipe of its `id` (`Build`), then the rest of the compound is
/// loaded. An `id` no recipe declares, or an entity that cannot be made, is set aside (`set_aside`) and None is returned: nothing is
/// spawned. `Passengers` restores the riders and seats them (`nbt_fields::rides`).
pub fn restore(w: &mut World, c: &Compound) -> Option<Entity> {
    let slot = w.get_resource_mut::<RestoreTrail>().and_then(|mut t| {
        let v = t.0.as_mut()?;
        v.push(None);
        Some(v.len() - 1)
    });
    let made = restore_one(w, c);
    if let (Some(slot), Some(e)) = (slot, made)
        && let Some(mut t) = w.get_resource_mut::<RestoreTrail>()
        && let Some(v) = t.0.as_mut()
    {
        v[slot] = Some(e);
    }
    made
}

/// The entity of a sort with a bespoke constructor (`nbt_fields::things::builders`: the item, the orb, the falling block...) made at `at`
/// from the keys `nbt` holds, as `restore` makes it: a species entity alone would lack the sort's own components. None when `kind` is
/// not such a sort; Some(None) when its constructor refused the keys.
pub fn builds(kind: &str) -> bool {
    crate::nbt_fields::things::builders().iter().any(|(id, _)| *id == kind)
}

/// Whether `kind` is made by a builder of its own: a builder table entry, or a declared recipe that builds.
pub fn declared_with(w: &World, kind: &str) -> bool {
    builds(kind) || recipe_of(w, kind).is_some_and(|r| matches!(r.build, Build::With(_)))
}

/// Whether `kind` is a declared recipe that builds and is not saved with its chunk (a summon makes it; a load does not).
pub fn declared_unsaved(w: &World, kind: &str) -> bool {
    recipe_of(w, kind).is_some_and(|r| !r.saved && matches!(r.build, Build::With(_)))
}

pub fn spawn_built(w: &mut World, kind: &str, at: Vec3, nbt: Option<&Compound>) -> Option<Option<Entity>> {
    let declared = recipe_of(w, kind).filter(|r| matches!(r.build, Build::With(_)));
    if declared.is_none() && !crate::nbt_fields::things::builders().iter().any(|(id, _)| *id == kind) {
        return None;
    }
    let mut c = nbt.cloned().unwrap_or_default();
    c.insert("id".into(), Tag::String(kind.into()));
    c.insert("Pos".into(), Tag::List(vec![Tag::Double(at.x.to_bits()), Tag::Double(at.y.to_bits()), Tag::Double(at.z.to_bits())]));
    // A summoned kind not saved with its chunk is made by its builder all the same: `saved` only decides what a chunk keeps.
    if let Some(Recipe { build: Build::With(b), saved: false }) = declared {
        return Some(b(w, at, &c));
    }
    Some(restore(w, &c))
}

/// The entities a `restore_tree` is making, one slot per compound met in the order of the NBT (the root, then each rider and its own
/// riders); a slot stays None when its compound made nothing.
#[derive(Resource, Default)]
struct RestoreTrail(Option<Vec<Option<Entity>>>);

/// `restore` of `c` and of the riders in its `Passengers`: the slot of each compound in NBT order, root first.
pub fn restore_tree(w: &mut World, c: &Compound) -> Vec<Option<Entity>> {
    w.init_resource::<RestoreTrail>();
    let outer = w.resource_mut::<RestoreTrail>().0.replace(Vec::new());
    restore(w, c);
    let mut t = w.resource_mut::<RestoreTrail>();
    let done = t.0.take().unwrap_or_default();
    t.0 = outer;
    done
}

fn restore_one(w: &mut World, c: &Compound) -> Option<Entity> {
    let Some(Tag::String(id)) = c.get(&JavaString::from("id")) else { return None };
    let name = id.to_string_lossy();
    let at = c.get(&JavaString::from("Pos")).and_then(tag_vec3)?;
    let Some(recipe) = recipe_of(w, &name) else {
        set_aside(w, &name, at, c);
        return None;
    };
    if !recipe.saved {
        return None;
    }
    let made = match recipe.build {
        Build::Passive => crate::passive::kinds().find(|k| *k == name).map(|k| crate::passive::spawn(w, k, at, Tag::Compound(c.clone()))),
        Build::Species => {
            let sp = crate::cow::species_table().0.into_iter().find(|sp| sp.name == name)?;
            Some(crate::cow::spawn_species(w, &sp, at.x, at.z, 0))
        }
        Build::With(b) => b(w, at, c),
    };
    let Some(e) = made else {
        set_aside(w, &name, at, c);
        return None;
    };
    if !matches!(recipe.build, Build::Passive) {
        // A species with a variant table starts at the registry's default; the saved key then replaces it.
        if w.get::<crate::behaviours::variant::Variant>(e).is_none() && crate::behaviours::variant::table_of(w, e).is_some() {
            crate::behaviours::variant::set_variant(w, e, 0);
        }
        // `Entity.load` without `Rotation` sets yaw and pitch to 0 (`setYRot(0)`, `setXRot(0)`): the spawn's draw does not stay.
        if c.get(&JavaString::from("Rotation")).is_none() {
            w.entity_mut(e).insert(crate::cow::Yaw(0.0)).remove::<crate::nbt_fields::common::Pitch>();
        }
        load(w, e, c);
        // `createState` counts the loaded creature as a wild one (a persistent one is not counted), with its biome's charge.
        crate::spawn_cost::count(w, e);
        // The saved `UUID` names the entity in the table of `entity_by_uuid` (a summon or a load of a mob).
        if let Some(Tag::IntArray(v)) = c.get(&JavaString::from("UUID"))
            && let Ok(words) = <[i32; 4]>::try_from(v.as_slice())
        {
            crate::bridge::uuids::set_uuid(w, e, words);
        }
    }
    w.trigger(Restored { entity: e });
    Some(e)
}

#[cfg(test)]
mod tests {
    use crate::cow::{Herd, Yaw};
    use crate::nbt_fields::common::Pitch;
    use horizon_core::nbt::{parse_snbt, Tag};

    #[test]
    fn restore_without_rotation_is_yaw_and_pitch_zero() {
        let mut h = Herd::with_slab_awake(16, true, false);
        let (tag, _) = parse_snbt(r#"{id:"minecraft:pig",Pos:[1d,70d,2d],NoAI:1b}"#).unwrap();
        let Tag::Compound(c) = tag else { panic!() };
        let e = crate::nbt::restore(&mut h.world, &c).unwrap();
        assert_eq!(h.world.get::<Yaw>(e).map(|y| y.0), Some(0.0));
        assert_eq!(h.world.get::<Pitch>(e).map_or(0.0, |p| p.0), 0.0);
    }

    #[test]
    fn a_recipe_summon_keeps_the_keys_its_constructor_did_not_read() {
        let mut h = Herd::with_slab_awake(16, true, false);
        let (tag, _) = parse_snbt(r#"{Value:3s,Tags:["a"],UUID:[I;1,2,3,4],NoGravity:1b}"#).unwrap();
        let Tag::Compound(c) = tag else { panic!() };
        let e = super::spawn_built(&mut h.world, crate::species::lists::EXPERIENCE_ORB, super::Vec3::new(4.5, 64.0, 4.5), Some(&c)).flatten().unwrap();
        super::load(&mut h.world, e, &c).unwrap();
        let saved = super::save(&h.world, e).unwrap();
        assert_eq!(saved.get(&"Tags".into()), Some(&Tag::List(vec![Tag::String("a".into())])));
        assert_eq!(saved.get(&"UUID".into()), Some(&Tag::IntArray(vec![1, 2, 3, 4])));
    }
}
