//! General entities: the types the mods declare. A `General` keeps its registry slot (`kind`, the key table is the bridge's), its
//! payload bytes as the mod wrote them (opaque here) and the fields the save shares with vanilla. The mod's tick is for the types that
//! subscribed: a `Subscribed` marker (sparse) sends one `ModTick` per tick, and a deadline (`request_tick`) sends one when it is due.
//! An entity of a type nothing here runs is `Unknown`: its compound is kept as loaded and saved back unchanged. Its outputs wait in
//! `GeneralEvents` until the host takes them (`take_general_events`).
use crate::cow::Pos;
use crate::goals::sparse_any;
use crate::host_out_general::GeneralEvent;
use crate::mount::Riding;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_core::nbt::{Compound, JavaString, Tag};

/// A mod's general entity: its registry slot, the payload's schema version and bytes, its UUID (four ints) and its rotation (yaw,
/// pitch). Vanilla's keys `UUID` and `Rotation` are kept; `PayloadVersion` and `Payload` are Horizon's own keys.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct General {
    pub kind: u16,
    pub payload_version: u16,
    pub payload: Vec<u8>,
    pub uuid: [i32; 4],
    pub rotation: [f32; 2],
}

/// A general entity whose type asked for its tick: one `ModTick` per tick while it holds the marker. Sparse: absent costs nothing.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Subscribed;

/// The riders of a vehicle, in the order they rode (`Riding` on each). Sparse: only a vehicle that carries one holds it.
#[derive(Component, Clone, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Carries(pub Vec<Entity>);

/// An entity of a type nothing here runs: its `id` and the compound it was loaded with, saved back as it is.
#[derive(Component, Clone, Debug, PartialEq)]
#[component(storage = "SparseSet")]
pub struct Unknown {
    pub id: String,
    pub nbt: Compound,
}

/// The outputs of the general entities, in the order they were made, until the host takes them.
#[derive(Resource, Default)]
pub struct GeneralEvents(pub Vec<GeneralEvent>);

/// Appends one output to `GeneralEvents`.
pub fn emit(w: &mut World, event: GeneralEvent) {
    w.get_resource_or_insert_with(GeneralEvents::default).0.push(event);
}

/// Subscribes the general entity `e` to its tick (or unsubscribes it). False when `e` is not a general entity.
pub fn subscribe(w: &mut World, e: Entity, on: bool) -> bool {
    if w.get::<General>(e).is_none() {
        return false;
    }
    if on {
        w.entity_mut(e).insert(Subscribed);
    } else {
        w.entity_mut(e).remove::<Subscribed>();
    }
    true
}

/// Asks for one `ModTick` of `e` `ticks` ticks of its own time from now: a deadline on the wheel, nothing in between.
pub fn request_tick(w: &mut World, e: Entity, ticks: i32) {
    if w.get::<General>(e).is_some() {
        crate::cow::after(w, e, ticks, fire_tick);
    }
}

fn fire_tick(w: &mut World, e: Entity) {
    if let Some(kind) = w.get::<General>(e).map(|g| g.kind) {
        emit(w, GeneralEvent::ModTick { entity: e, kind });
    }
}

/// One `ModTick` per subscribed general entity, this tick.
fn tick_subscribed(w: &mut World) {
    let subscribed: Vec<(Entity, u16)> = crate::queries::with::<(Entity, &'static General), With<Subscribed>, _>(w, |q, w| q.iter(w).map(|(e, g)| (e, g.kind)).collect());
    for (entity, kind) in subscribed {
        emit(w, GeneralEvent::ModTick { entity, kind });
    }
}

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(tick_subscribed.run_if(|w: &World| sparse_any::<Subscribed>(w)));
}

/// A general entity at `at` with its payload, as the summon or the load makes it. Nothing is subscribed yet.
pub fn spawn(w: &mut World, at: Vec3, general: General) -> Entity {
    w.spawn((Pos(at), general)).id()
}

/// Makes `rider` ride `vehicle`: its `Riding` and the vehicle's `Carries`.
pub fn carry(w: &mut World, vehicle: Entity, rider: Entity) {
    w.entity_mut(rider).insert(Riding(vehicle));
    if let Some(mut c) = w.get_mut::<Carries>(vehicle) {
        c.0.push(rider);
    } else {
        w.entity_mut(vehicle).insert(Carries(vec![rider]));
    }
}

/// Writes `payload` into `e` when it differs, and emits `Changed`. False when nothing changed or `e` is not a general entity.
pub fn set_payload(w: &mut World, e: Entity, payload: Vec<u8>) -> bool {
    let Some(mut g) = w.get_mut::<General>(e) else { return false };
    if g.payload == payload {
        return false;
    }
    g.payload = payload;
    drop(g);
    emit(w, GeneralEvent::Changed { entity: e });
    true
}

/// The tag under `key` of a save compound.
fn field<'a>(c: &'a Compound, key: &str) -> Option<&'a Tag> {
    c.get(&JavaString::from(key))
}

fn doubles(v: Vec3) -> Tag {
    Tag::List([v.x, v.y, v.z].map(|c| Tag::Double(c.to_bits())).to_vec())
}

/// The three doubles of a list tag (`Pos`), None when the tag has another shape.
fn read_doubles(tag: Option<&Tag>) -> Option<[f64; 3]> {
    let Some(Tag::List(values)) = tag else { return None };
    let [Tag::Double(x), Tag::Double(y), Tag::Double(z)] = values.as_slice() else { return None };
    Some([f64::from_bits(*x), f64::from_bits(*y), f64::from_bits(*z)])
}

/// The compound of a general entity in the save format: `id` (its key, from the bridge's table by `kind`), `Pos`, `Rotation`, `UUID`,
/// `PayloadVersion`, `Payload`, and `Passengers` for its riders that are general entities. None when `e` is not a general entity or
/// its kind has no key.
pub fn save(w: &World, e: Entity, keys: &[&str]) -> Option<Compound> {
    let g = w.get::<General>(e)?;
    let key = keys.get(usize::from(g.kind))?;
    let pos = w.get::<Pos>(e)?.0;
    let mut map = Compound::new();
    map.insert("id".into(), Tag::String((*key).into()));
    map.insert("Pos".into(), doubles(pos));
    map.insert("Rotation".into(), Tag::List(g.rotation.map(|r| Tag::Float(r.to_bits())).to_vec()));
    map.insert("UUID".into(), Tag::IntArray(g.uuid.to_vec()));
    map.insert("PayloadVersion".into(), Tag::Short(g.payload_version as i16));
    map.insert("Payload".into(), Tag::ByteArray(g.payload.clone()));
    let riders: Vec<Tag> = w.get::<Carries>(e).map_or_else(Vec::new, |c| c.0.iter().filter_map(|&r| save(w, r, keys)).map(Tag::Compound).collect());
    if !riders.is_empty() {
        map.insert("Passengers".into(), Tag::List(riders));
    }
    Some(map)
}

/// The general entity a compound saved (`save`) describes, its `id` resolved against `keys`, and its riders loaded and carried. None
/// when the id has no key or a field has another shape.
pub fn load(w: &mut World, c: &Compound, keys: &[&str]) -> Option<Entity> {
    let id = match field(c, "id")? {
        Tag::String(s) => s.to_string_lossy(),
        _ => return None,
    };
    let kind = keys.iter().position(|k| *k == id)? as u16;
    let [x, y, z] = read_doubles(field(c, "Pos")).unwrap_or([0.0; 3]);
    let uuid = match field(c, "UUID")? {
        Tag::IntArray(v) => <[i32; 4]>::try_from(v.as_slice()).ok()?,
        _ => return None,
    };
    let rotation = match field(c, "Rotation") {
        None => [0.0; 2],
        Some(Tag::List(v)) => match v.as_slice() {
            [Tag::Float(a), Tag::Float(b)] => [f32::from_bits(*a), f32::from_bits(*b)],
            _ => return None,
        },
        Some(_) => return None,
    };
    let payload_version = match field(c, "PayloadVersion") {
        None => 0,
        Some(Tag::Short(v)) => *v as u16,
        Some(_) => return None,
    };
    let payload = match field(c, "Payload") {
        None => Vec::new(),
        Some(Tag::ByteArray(v)) => v.clone(),
        Some(_) => return None,
    };
    let general = General { kind, payload_version, payload, uuid, rotation };
    let e = spawn(w, Vec3::new(x, y, z), general);
    if let Some(Tag::List(riders)) = field(c, "Passengers") {
        for rider in riders {
            let Tag::Compound(sub) = rider else { return None };
            let rider = load(w, sub, keys)?;
            carry(w, e, rider);
        }
    }
    Some(e)
}

/// An entity of a type nothing here runs, kept as `c` (its `id` is its type). None when `c` has no string `id`.
pub fn spawn_unknown(w: &mut World, c: Compound) -> Option<Entity> {
    let Tag::String(id) = field(&c, "id")? else { return None };
    let id = id.to_string_lossy();
    let [x, y, z] = read_doubles(field(&c, "Pos")).unwrap_or([0.0; 3]);
    Some(w.spawn((Pos(Vec3::new(x, y, z)), Unknown { id, nbt: c })).id())
}

/// The compound an `Unknown` was loaded with, unchanged. None when `e` is not an unknown entity.
pub fn save_unknown(w: &World, e: Entity) -> Option<Compound> {
    w.get::<Unknown>(e).map(|u| u.nbt.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> [&'static str; 2] {
        ["mymod:robot", "mymod:lamp"]
    }

    fn general(kind: u16, payload: Vec<u8>) -> General {
        General { kind, payload_version: 3, payload, uuid: [1, -2, 3, 4], rotation: [90.0, -15.5] }
    }

    #[test]
    fn general_round_trips_through_its_save_compound() {
        let mut w = World::new();
        let g = general(1, vec![9, 8, 255]);
        let e = spawn(&mut w, Vec3::new(1.5, 64.0, -3.25), g.clone());
        let c = save(&w, e, &keys()).expect("a general entity saves");
        assert_eq!(field(&c, "id"), Some(&Tag::String("mymod:lamp".into())));

        let mut back = World::new();
        let e2 = load(&mut back, &c, &keys()).expect("the save loads");
        assert_eq!(back.get::<General>(e2), Some(&g));
        assert_eq!(back.get::<Pos>(e2).map(|p| p.0.x), Some(1.5));
        assert_eq!(save(&back, e2, &keys()), Some(c));
    }

    #[test]
    fn riders_are_saved_under_passengers_and_carried_back() {
        let mut w = World::new();
        let root = spawn(&mut w, Vec3::new(0.0, 60.0, 0.0), general(0, vec![1]));
        let rider = spawn(&mut w, Vec3::new(0.0, 61.0, 0.0), general(1, vec![2, 3]));
        carry(&mut w, root, rider);
        let c = save(&w, root, &keys()).expect("the vehicle saves");
        assert!(matches!(field(&c, "Passengers"), Some(Tag::List(v)) if v.len() == 1));

        let mut back = World::new();
        let e = load(&mut back, &c, &keys()).expect("the tree loads");
        let riders = back.get::<Carries>(e).expect("the vehicle carries its rider").0.clone();
        assert_eq!(riders.len(), 1);
        assert_eq!(back.get::<Riding>(riders[0]).map(|r| r.0), Some(e));
        assert_eq!(back.get::<General>(riders[0]).map(|g| g.payload.clone()), Some(vec![2, 3]));
    }

    #[test]
    fn unknown_entity_saves_back_identically() {
        let mut w = World::new();
        let mut c = Compound::new();
        c.insert("id".into(), Tag::String("otherteam:thing".into()));
        c.insert("Pos".into(), doubles(Vec3::new(4.0, 70.0, 9.0)));
        c.insert("Extra".into(), Tag::Int(42));
        let e = spawn_unknown(&mut w, c.clone()).expect("an unknown entity spawns");
        assert_eq!(save_unknown(&w, e), Some(c));
    }

    #[test]
    fn subscribed_marker_is_the_only_tick_cost() {
        let mut w = World::new();
        let e = spawn(&mut w, Vec3::new(0.0, 0.0, 0.0), general(0, Vec::new()));
        assert!(!w.entity(e).contains::<Subscribed>());
        assert!(subscribe(&mut w, e, true));
        assert!(w.entity(e).contains::<Subscribed>());
        assert!(!subscribe(&mut World::new(), e, true));
    }
}
