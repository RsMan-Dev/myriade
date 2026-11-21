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
    pub fn short(t: &Tag) -> bool {
        matches!(t, Tag::Short(_))
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
    if let Some(reg) = w.get_resource::<NbtFields>() {
        for &i in applicable_indices(w, e).iter() {
            (reg.fields[i].0.write)(w, e, &mut c);
        }
    }
    for &(key, default) in crate::nbt_fields::common::DEFAULTS {
        if c.get(&JavaString::from(key)).is_none() {
            c.insert(key.into(), default());
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
            !taken.contains(&key) && !crate::nbt_fields::common::drop_default(&key, v)
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
    if w.get::<Saved>(e).is_none() || applicable(w, e, key).is_some() {
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
    Field { key: Cow::Borrowed("Health"), owner: Some(owner::<Health>), accepts: accept::float, write: write_health, read: read_health },
    Field { key: Cow::Borrowed("id"), owner: None, accepts: accept::string, write: write_id, read: read_id },
];

fn write_pos(w: &World, e: Entity, c: &mut Compound) {
    let Some(p) = w.get::<Pos>(e) else { return };
    c.insert("Pos".into(), vec3_tag(p.0));
}

fn read_pos(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(v) = tag_vec3(t) else { return false };
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

fn write_health(w: &World, e: Entity, c: &mut Compound) {
    if let Some(h) = w.get::<Health>(e) {
        c.insert("Health".into(), Tag::Float(h.0.to_bits()));
    }
}

fn read_health(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Float(bits) = t else { return false };
    w.entity_mut(e).insert(Health(f32::from_bits(*bits)));
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
    let saved: Vec<Entity> = w.query_filtered::<Entity, With<Saved>>().iter(w).collect();
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

/// A rider of a saved mount: its mount writes it under `Passengers`, so `save_all` does not write it a second time.
fn written_by_mount(w: &World, e: Entity) -> bool {
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

/// Whether `id` is declared.
pub fn declared(w: &World, id: &str) -> bool {
    recipe_of(w, id).is_some()
}

/// Restores the entity a saved compound describes, at its `Pos`, made by the recipe of its `id` (`Build`), then the rest of the compound is
/// loaded. An `id` no recipe declares, or an entity that cannot be made, is set aside (`set_aside`) and None is returned: nothing is
/// spawned. `Passengers` restores the riders and seats them (`nbt_fields::rides`).
pub fn restore(w: &mut World, c: &Compound) -> Option<Entity> {
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
        load(w, e, c);
    }
    w.trigger(Restored { entity: e });
    Some(e)
}
