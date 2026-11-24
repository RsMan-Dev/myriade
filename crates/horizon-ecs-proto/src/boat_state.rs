//! The state a boat keeps beside its physics (`boat.rs`): its wood and chest, the bubble-column wobble, the hurt shake, the hull's fire,
//! freezing, fall, ground, stuck speed and gravity, and the chest's container. Its spawn, its NBT fields (`FIELDS`) and the recipe of
//! each species (`register_nbt`). `Rotation`, `Pos`, `Motion` and the shared keys come from the common fields.
use crate::boat::{Boat, Kind};
use crate::cow::{Pos, TypeName, Vel, Yaw};
use crate::nbt::{self, accept, owner, Field};
use crate::species::boats::{entity_id, species, WOODS};
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;
use horizon_core::{geometry::Vec3, nbt::{Compound, JavaString, Tag}};
use horizon_items::item_nbt::{decode_items, encode_items};
use std::borrow::Cow;

/// `ChestBoat.getContainerSize()`: 27 slots (bytecode of `AbstractChestBoat`).
pub const CHEST_SLOTS: usize = 27;

/// The index of the boat's wood in `WOODS`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wood(pub u8);

/// The marker of a boat (`vehicle.rs` keeps the player's vehicle view under its own name).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct BoatMarker;

/// The wobble over a bubble column: set where the first bubble column is, absent otherwise.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet", on_insert = crate::bridge::view::touched, on_remove = crate::bridge::view::touched)]
pub struct Wobble {
    pub time: i32,
    pub above: bool,
    pub down: bool,
    pub multiplier: f32,
    pub angle: f32,
    pub angle_old: f32,
}

/// `hurtTime`, `hurtDir` and `damage`: the shake after a hit. Absent at rest.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet", on_insert = crate::bridge::view::touched, on_remove = crate::bridge::view::touched)]
pub struct Hurt {
    pub time: i32,
    pub dir: i32,
    pub damage: f32,
}

/// What the host keeps of the hull beside the physics.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Hull {
    pub fire: i32,
    pub frozen: i32,
    pub fall: f64,
    pub stuck: [f64; 3],
    pub on_ground: bool,
}

/// A loot table not rolled yet: the table's id and the seed it rolls with.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingLoot {
    pub table: String,
    pub seed: i64,
}

/// A chest boat's container. Present only on a chest boat.
#[derive(Component, Clone, Debug, Default)]
pub struct Chest {
    pub slots: Vec<Option<ItemStack>>,
    pub loot: Option<PendingLoot>,
}

/// A boat of wood `wood` (an index of `WOODS`), with a chest when `chest`, at `at`, facing `yaw`. It is still: no `Moving`, no due.
/// The entity is saved (`Saved`) when its species' recipe is declared (`register_nbt`).
pub fn spawn(w: &mut World, wood: u8, chest: bool, at: [f64; 3], yaw: f32) -> Entity {
    let id = entity_id(wood, chest);
    let kind = Kind { raft: WOODS[usize::from(wood)].3, chest };
    let e = w
        .spawn((Pos(Vec3::new(at[0], at[1], at[2])), Vel(Vec3::new(0.0, 0.0, 0.0)), Yaw(yaw), Boat::default(), kind, Wood(wood), Hull::default(), BoatMarker, TypeName(id)))
        .id();
    if chest {
        w.entity_mut(e).insert(Chest { slots: vec![None; CHEST_SLOTS], loot: None });
    }
    nbt::mark_saved(w, e, id);
    e
}

/// The NBT of boat `e`, by the declared fields (`nbt::save`), None when `e` is no saved entity.
pub fn save(w: &World, e: Entity) -> Option<Compound> {
    nbt::save(w, e)
}

/// The boat `c` describes, made by its id's recipe and loaded with its fields (`nbt::restore`). None when `c` names no boat.
pub fn load(w: &mut World, c: &Compound) -> Option<Entity> {
    nbt::restore(w, c)
}

/// `Build::With` of the boat species: the boat of the compound's `id`, at `at` (the fields are read by `nbt::restore`).
pub fn build(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    let Some(Tag::String(id)) = c.get(&JavaString::from("id")) else { return None };
    let (_, wood, chest) = species().find(|s| s.0 == id.to_string_lossy())?;
    Some(spawn(w, wood, chest, [at.x, at.y, at.z], 0.0))
}

/// Declares the boat state's fields and the recipe of every boat species. Called in the declaration window of the world.
pub fn register_nbt(w: &mut World) {
    nbt::register(w, FIELDS);
    for (id, _, _) in species() {
        nbt::declare(w, id, nbt::Recipe { saved: true, build: nbt::Build::With(build) });
    }
}

/// The keys of a boat's state. A key applies to an entity holding the component it names.
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("Fire"), write: write_fire, read: read_fire, accepts: accept::short, owner: Some(owner::<Hull>) },
    Field { key: Cow::Borrowed("TicksFrozen"), write: write_frozen, read: read_frozen, accepts: accept::int, owner: Some(owner::<Hull>) },
    Field { key: Cow::Borrowed("fall_distance"), write: write_fall, read: read_fall, accepts: accept::any, owner: Some(owner::<Hull>) },
    Field { key: Cow::Borrowed("OnGround"), write: write_on_ground, read: read_on_ground, accepts: accept::any, owner: Some(owner::<Hull>) },
    Field { key: Cow::Borrowed("Items"), write: write_items, read: read_items, accepts: accept::any, owner: Some(owner::<Chest>) },
    Field { key: Cow::Borrowed("LootTable"), write: write_loot_table, read: read_loot_table, accepts: accept::string, owner: Some(owner::<Chest>) },
    Field { key: Cow::Borrowed("LootTableSeed"), write: write_loot_seed, read: read_loot_seed, accepts: accept::long, owner: Some(owner::<Chest>) },
];

fn write_fire(w: &World, e: Entity, c: &mut Compound) {
    if let Some(h) = w.get::<Hull>(e) {
        c.insert("Fire".into(), Tag::Short(h.fire as i16));
    }
}
fn read_fire(w: &mut World, e: Entity, t: &Tag) -> bool {
    let (Some(mut h), Tag::Short(s)) = (w.get_mut::<Hull>(e), t) else { return false };
    h.fire = i32::from(*s);
    true
}

/// `TicksFrozen` is written only while positive.
fn write_frozen(w: &World, e: Entity, c: &mut Compound) {
    if let Some(h) = w.get::<Hull>(e).filter(|h| h.frozen > 0) {
        c.insert("TicksFrozen".into(), Tag::Int(h.frozen));
    }
}
fn read_frozen(w: &mut World, e: Entity, t: &Tag) -> bool {
    let (Some(mut h), Tag::Int(i)) = (w.get_mut::<Hull>(e), t) else { return false };
    h.frozen = *i;
    true
}

fn write_fall(w: &World, e: Entity, c: &mut Compound) {
    if let Some(h) = w.get::<Hull>(e) {
        c.insert("fall_distance".into(), Tag::Double(h.fall.to_bits()));
    }
}
fn read_fall(w: &mut World, e: Entity, t: &Tag) -> bool {
    let (Some(mut h), Tag::Double(d)) = (w.get_mut::<Hull>(e), t) else { return false };
    h.fall = f64::from_bits(*d);
    true
}

fn write_on_ground(w: &World, e: Entity, c: &mut Compound) {
    if let Some(h) = w.get::<Hull>(e) {
        c.insert("OnGround".into(), Tag::Byte(i8::from(h.on_ground)));
    }
}
fn read_on_ground(w: &mut World, e: Entity, t: &Tag) -> bool {
    let (Some(mut h), Tag::Byte(b)) = (w.get_mut::<Hull>(e), t) else { return false };
    h.on_ground = *b != 0;
    true
}

/// `Items` is written when no table is pending: a pending table replaces it.
fn write_items(w: &World, e: Entity, c: &mut Compound) {
    if let Some(k) = w.get::<Chest>(e).filter(|k| k.loot.is_none()) {
        c.insert("Items".into(), encode_items(&k.slots));
    }
}
fn read_items(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(mut k) = w.get_mut::<Chest>(e) else { return false };
    k.slots = decode_items(Some(t), CHEST_SLOTS);
    true
}

fn write_loot_table(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<Chest>(e).and_then(|k| k.loot.as_ref()) {
        c.insert("LootTable".into(), Tag::String(p.table.as_str().into()));
    }
}
fn read_loot_table(w: &mut World, e: Entity, t: &Tag) -> bool {
    let (Some(mut k), Tag::String(s)) = (w.get_mut::<Chest>(e), t) else { return false };
    k.loot = Some(PendingLoot { table: s.to_string_lossy(), seed: 0 });
    true
}

fn write_loot_seed(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<Chest>(e).and_then(|k| k.loot.as_ref()) {
        c.insert("LootTableSeed".into(), Tag::Long(p.seed));
    }
}
fn read_loot_seed(w: &mut World, e: Entity, t: &Tag) -> bool {
    let (Some(mut k), Tag::Long(seed)) = (w.get_mut::<Chest>(e), t) else { return false };
    let Some(p) = k.loot.as_mut() else { return false };
    p.seed = *seed;
    true
}

/// The wood name and chest flag of boat `e`, None when `e` is no boat.
pub fn wood_of(w: &World, e: Entity) -> Option<(&'static str, bool)> {
    let wood = w.get::<Wood>(e)?.0;
    let chest = w.get::<Kind>(e)?.chest;
    Some((WOODS[usize::from(wood)].0, chest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::boats::index;

    fn world() -> World {
        let w = crate::cow::Herd::new(0, 1.0, true, false, 1).world;
        w
    }

    #[test]
    fn a_summoned_oak_boat_is_made_by_its_declared_recipe() {
        let mut w = world();
        let e = crate::nbt::spawn_built(&mut w, "minecraft:oak_boat", Vec3::new(0.0, 64.0, 0.0), None).flatten();
        assert!(crate::nbt::declared_with(&w, "minecraft:oak_boat"));
        assert!(e.is_some(), "the oak boat is summoned");
    }

    fn state(w: &World, e: Entity) -> ([f64; 3], Hull, u8, bool) {
        let p = w.get::<Pos>(e).unwrap().0;
        (
            [p.x, p.y, p.z],
            *w.get::<Hull>(e).unwrap(),
            w.get::<Wood>(e).unwrap().0,
            w.get::<Kind>(e).unwrap().chest,
        )
    }

    #[test]
    fn save_then_load_gives_back_every_boat_state() {
        for wood in 0..WOODS.len() as u8 {
            for chest in [false, true] {
                let mut a = world();
                let e = spawn(&mut a, wood, chest, [1.5, 64.0, -3.25], 90.0);
                *a.get_mut::<Hull>(e).unwrap() = Hull { fire: 40, frozen: 3, fall: 2.5, stuck: [0.0; 3], on_ground: true };
                let c = save(&a, e).expect("a boat is saved");
                let mut b = world();
                let f = load(&mut b, &c).expect("the saved boat loads");
                assert_eq!(state(&a, e), state(&b, f));
            }
        }
    }

    #[test]
    fn a_chest_boat_has_27_slots_and_a_raft_without_chest_has_none() {
        let mut w = world();
        let chest = spawn(&mut w, index("oak").unwrap(), true, [0.0; 3], 0.0);
        let raft = spawn(&mut w, index("bamboo").unwrap(), false, [0.0; 3], 0.0);
        assert_eq!(w.get::<Chest>(chest).unwrap().slots.len(), 27);
        assert!(w.get::<Chest>(raft).is_none());
    }

    #[test]
    fn a_still_boat_is_not_moving() {
        let mut w = world();
        let e = spawn(&mut w, 0, false, [0.0; 3], 0.0);
        assert!(w.get::<crate::cow::Moving>(e).is_none());
    }
}
