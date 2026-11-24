//! The state of a minecart that its physics does not hold (greffe S56): the marker `Cart` on every genre, the slots of a chest or a hopper
//! with the loot table not rolled yet, a hopper's `Enabled`, a TNT cart's blast powers, a command block's command, a custom display, a
//! generator's spawner keys. Each key is the one the jar writes (`AbstractMinecart`, `ContainerEntity`, `MinecartHopper`, `MinecartTNT`,
//! `BaseCommandBlock`, `BaseSpawner`). The furnace's `Fuel`, `PushX`, `PushZ` and the TNT's `fuse` stay on `Furnace` and `Fuse`, declared
//! by `nbt_fields::rides`. A cart on no rail gets no `Moving`: nothing visits it, so it has no deadline.
use crate::cow::*;
use crate::goals::{next_uid, Uid};
use crate::minecart::{Furnace, Fuse, Kind, Track};
use crate::nbt::{self, accept, owner, Build, Field, Recipe};
use crate::rules::minecart::bounds_at;
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;
use myriade_level::chunk::PendingContainerLoot;
use horizon_core::{geometry::Vec3, nbt::{Compound, JavaString, Tag}};
use horizon_items::item_nbt::{decode_items, encode_items};
use std::borrow::Cow;

/// The marker of a minecart of any genre (`AbstractMinecart`); the genre is its `Kind`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Cart;

/// A hopper cart's `Enabled` (`MinecartHopper`): an unpowered activator rail enables it. Only hoppers carry it.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(storage = "SparseSet")]
pub struct Enabled(pub bool);

/// The slots of a chest or a hopper cart (`ContainerEntity`), and the loot table they are rolled from: `loot` stays until the roll.
#[derive(Component, Default)]
#[component(storage = "SparseSet")]
pub struct Contents {
    pub slots: Vec<Option<ItemStack>>,
    pub loot: Option<PendingContainerLoot>,
}

/// A command cart's `BaseCommandBlock`: the command, its last output and the output tracking (`Command`, `LastOutput`, `TrackOutput`), the
/// success count (`SuccessCount`), the custom name as saved (`CustomName`, absent when null), and the last run (`UpdateLastExecution`, and
/// `LastExecution` once run). A new block tracks its output and updates its last run, as the jar's constructor sets them.
#[derive(Component, Clone, Debug, PartialEq)]
#[component(storage = "SparseSet")]
pub struct CommandBlock {
    pub command: String,
    pub last_output: String,
    pub track: bool,
    pub success: i32,
    pub custom_name: Option<Tag>,
    pub update_last_execution: bool,
    /// The game time of the last run (`LastExecution`), None until a run.
    pub last_execution: Option<i64>,
    /// The game time of the last activation (`lastActivated`, not saved): a powered rail runs the command at most every 4 ticks.
    pub last_activated: Option<i32>,
}

impl Default for CommandBlock {
    fn default() -> Self {
        Self {
            command: String::new(),
            last_output: String::new(),
            track: true,
            success: 0,
            custom_name: None,
            update_last_execution: true,
            last_execution: None,
            last_activated: None,
        }
    }
}

/// Whether the cart has ticked (`HasTicked`), false on a new cart.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Ticked(pub bool);

/// The flipped rotation of the cart (`FlippedRotation`), false on a new cart.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Flipped(pub bool);

/// A generator cart's `BaseSpawner` keys (`Delay`, `SpawnData`, `SpawnPotentials`...), kept as saved and written back as they are.
#[derive(Component, Clone, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Spawner {
    pub data: Compound,
}

/// `AbstractMinecart`'s custom display: the block state as its NBT (`DisplayState`), and its offset in sixteenths (`DisplayOffset`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
#[component(storage = "SparseSet")]
pub struct Display {
    pub block: Option<Tag>,
    pub offset: i32,
}

/// `MinecartTNT`'s blast: the power at rest (`explosion_power`) and its gain with the speed (`explosion_speed_factor`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[component(storage = "SparseSet")]
pub struct BlastPower {
    pub base: f32,
    pub speed_factor: f32,
}

/// The hurt state of a cart (`Hurt`, `dir`, `damage`): not saved by the jar, so it is not a key.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
#[component(storage = "SparseSet")]
pub struct CartHurt {
    pub time: i32,
    pub dir: i32,
    pub damage: f32,
}

/// `MinecartTNT`'s default powers (jar: 4.0 and 1.0); a cart at them saves no key.
pub const DEFAULT_POWER: BlastPower = BlastPower { base: 4.0, speed_factor: 1.0 };

/// `AbstractMinecart`'s default display offset of a genre (jar: chest 8, hopper 1, the others 6).
pub fn default_offset(kind: Kind) -> i32 {
    match kind {
        Kind::Chest => 8,
        Kind::Hopper => 1,
        _ => 6,
    }
}

/// The slots of a container genre (`ContainerEntity`'s size: a chest 27, a hopper 5, none otherwise).
pub fn slot_count(kind: Kind) -> usize {
    match kind {
        Kind::Chest => 27,
        Kind::Hopper => 5,
        _ => 0,
    }
}

/// The species name of a genre (`species::minecart::GENRES`).
pub fn name_of(kind: Kind) -> &'static str {
    crate::species::minecart::GENRES.iter().find(|(_, k)| *k == kind).map_or("", |(d, _)| d.name)
}

/// The creation of a cart of `kind` at `at`, facing `yaw`, started: its variant's deadline is armed (`cart_start`). It is not `Moving`.
pub fn spawn(w: &mut World, kind: Kind, at: [f64; 3], yaw: f32) -> Entity {
    let e = spawn_idle(w, kind, at, yaw);
    crate::cart_variants::start_cart(w, e);
    e
}

/// The creation without the variant's deadline: a loaded cart gets it once its keys are read (`load`).
fn spawn_idle(w: &mut World, kind: Kind, at: [f64; 3], yaw: f32) -> Entity {
    let name = name_of(kind);
    let p = Vec3::new(at[0], at[1], at[2]);
    let e = w.spawn((Pos(p), crate::cow::Vel(Vec3::ZERO), Uid(next_uid()), TypeName(name), Cart, kind, Track::default(), Yaw(yaw), Bounds(bounds_at(p)), Cell(u32::MAX, [i32::MIN; 8], u32::MAX))).id();
    crate::nbt::mark_saved(w, e, name);
    w.entity_mut(e).insert((Ticked(false), Flipped(false)));
    let container = Contents { slots: vec![None; slot_count(kind)], loot: None };
    match kind {
        Kind::Chest => {
            w.entity_mut(e).insert(container);
        }
        Kind::Hopper => {
            w.entity_mut(e).insert((container, Enabled(true)));
        }
        Kind::Furnace => {
            w.entity_mut(e).insert(Furnace::default());
        }
        Kind::Tnt => {
            w.entity_mut(e).insert(Fuse(-1));
        }
        Kind::CommandBlock => {
            w.entity_mut(e).insert(CommandBlock::default());
        }
        Kind::Spawner => {
            w.entity_mut(e).insert(Spawner::default());
        }
        Kind::Rideable => {}
    }
    enter_index(w, e);
    e
}

/// The cart's compound as the framework composes it: the keys of its components and its free keys.
pub fn save(w: &World, e: Entity) -> Option<Compound> {
    crate::nbt::save(w, e)
}

/// The cart a compound saved as, restored by its declared builder (`nbt::restore`).
pub fn load(w: &mut World, c: &Compound) -> Option<Entity> {
    let e = crate::nbt::restore(w, c)?;
    crate::cart_variants::start_cart(w, e);
    Some(e)
}

/// Builds a saved cart of any genre: the genre is the compound's `id`, read from `GENRES`.
fn build(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    let Some(Tag::String(id)) = c.get(&JavaString::from("id")) else { return None };
    let name = id.to_string_lossy();
    let kind = crate::species::minecart::GENRES.iter().find(|(d, _)| d.name == name)?.1;
    Some(spawn_idle(w, kind, [at.x, at.y, at.z], 0.0))
}

/// Declares the seven genres for `nbt::restore`, each saved and built by `build`. Called once with `register`, at the world's init.
pub fn register_nbt(w: &mut World) {
    for (def, _) in crate::species::minecart::GENRES {
        nbt::declare(w, def.name, Recipe { saved: true, build: Build::With(build) });
    }
}

fn byte(t: &Tag) -> bool {
    matches!(t, Tag::Byte(_))
}
fn list(t: &Tag) -> bool {
    matches!(t, Tag::List(_))
}
fn compound(t: &Tag) -> bool {
    matches!(t, Tag::Compound(_))
}
fn text(s: &JavaString) -> String {
    s.to_string_lossy().to_string()
}

fn write_enabled(w: &World, e: Entity, c: &mut Compound) {
    if let Some(v) = w.get::<Enabled>(e) {
        c.insert("Enabled".into(), Tag::Byte(i8::from(v.0)));
    }
}
fn read_enabled(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut en) = w.get_mut::<Enabled>(e) else { return false };
    en.0 = *v != 0;
    true
}

fn write_items(w: &World, e: Entity, c: &mut Compound) {
    if let Some(k) = w.get::<Contents>(e) {
        c.insert("Items".into(), encode_items(&k.slots));
    }
}
fn read_items(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(mut k) = w.get_mut::<Contents>(e) else { return false };
    k.slots = decode_items(Some(t), k.slots.len());
    true
}

fn write_loot_table(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<Contents>(e).and_then(|k| k.loot.as_ref()) {
        c.insert("LootTable".into(), p.table.clone());
    }
}
fn read_loot_table(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(mut k) = w.get_mut::<Contents>(e) else { return false };
    k.loot = Some(PendingContainerLoot { table: t.clone(), seed: None });
    true
}
fn write_loot_seed(w: &World, e: Entity, c: &mut Compound) {
    if let Some(seed) = w.get::<Contents>(e).and_then(|k| k.loot.as_ref()).and_then(|p| p.seed.clone()) {
        c.insert("LootTableSeed".into(), seed);
    }
}
fn read_loot_seed(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(mut k) = w.get_mut::<Contents>(e) else { return false };
    let Some(p) = k.loot.as_mut() else { return false };
    p.seed = Some(t.clone());
    true
}

fn write_display_state(w: &World, e: Entity, c: &mut Compound) {
    if let Some(block) = w.get::<Display>(e).and_then(|d| d.block.as_ref()) {
        c.insert("DisplayState".into(), block.clone());
    }
}
fn read_display_state(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(mut d) = w.get_mut::<Display>(e) else { return false };
    d.block = Some(t.clone());
    true
}
fn write_display_offset(w: &World, e: Entity, c: &mut Compound) {
    let kind = w.get::<Kind>(e).copied().unwrap_or(Kind::Rideable);
    if let Some(d) = w.get::<Display>(e).filter(|d| d.offset != default_offset(kind)) {
        c.insert("DisplayOffset".into(), Tag::Int(d.offset));
    }
}
fn read_display_offset(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let Some(mut d) = w.get_mut::<Display>(e) else { return false };
    d.offset = *v;
    true
}

fn write_power_base(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<BlastPower>(e).filter(|p| p.base != DEFAULT_POWER.base) {
        c.insert("explosion_power".into(), Tag::Float(p.base.to_bits()));
    }
}
fn read_power_base(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Float(v) = t else { return false };
    let Some(mut p) = w.get_mut::<BlastPower>(e) else { return false };
    p.base = f32::from_bits(*v).clamp(0.0, 128.0);
    true
}
fn write_power_speed(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<BlastPower>(e).filter(|p| p.speed_factor != DEFAULT_POWER.speed_factor) {
        c.insert("explosion_speed_factor".into(), Tag::Float(p.speed_factor.to_bits()));
    }
}
fn read_power_speed(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Float(v) = t else { return false };
    let Some(mut p) = w.get_mut::<BlastPower>(e) else { return false };
    p.speed_factor = f32::from_bits(*v).clamp(0.0, 128.0);
    true
}

fn write_command(w: &World, e: Entity, c: &mut Compound) {
    if let Some(b) = w.get::<CommandBlock>(e) {
        c.insert("Command".into(), Tag::String(b.command.as_str().into()));
    }
}
fn read_command(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::String(s) = t else { return false };
    let Some(mut b) = w.get_mut::<CommandBlock>(e) else { return false };
    b.command = text(s);
    true
}
fn write_custom_name(w: &World, e: Entity, c: &mut Compound) {
    if let Some(n) = w.get::<CommandBlock>(e).and_then(|b| b.custom_name.as_ref()) {
        c.insert("CustomName".into(), n.clone());
    }
}
fn read_custom_name(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(mut b) = w.get_mut::<CommandBlock>(e) else { return false };
    b.custom_name = Some(t.clone());
    true
}
fn write_last_output(w: &World, e: Entity, c: &mut Compound) {
    if let Some(b) = w.get::<CommandBlock>(e).filter(|b| b.track) {
        c.insert("LastOutput".into(), Tag::String(b.last_output.as_str().into()));
    }
}
fn read_last_output(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::String(s) = t else { return false };
    let Some(mut b) = w.get_mut::<CommandBlock>(e) else { return false };
    b.last_output = text(s);
    true
}
fn write_success(w: &World, e: Entity, c: &mut Compound) {
    if let Some(b) = w.get::<CommandBlock>(e) {
        c.insert("SuccessCount".into(), Tag::Int(b.success));
    }
}
fn read_success(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let Some(mut b) = w.get_mut::<CommandBlock>(e) else { return false };
    b.success = *v;
    true
}
fn write_update_last(w: &World, e: Entity, c: &mut Compound) {
    if let Some(b) = w.get::<CommandBlock>(e) {
        c.insert("UpdateLastExecution".into(), Tag::Byte(i8::from(b.update_last_execution)));
    }
}
fn read_update_last(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut b) = w.get_mut::<CommandBlock>(e) else { return false };
    b.update_last_execution = *v != 0;
    true
}
fn write_last_execution(w: &World, e: Entity, c: &mut Compound) {
    if let Some(v) = w.get::<CommandBlock>(e).filter(|b| b.update_last_execution).and_then(|b| b.last_execution) {
        c.insert("LastExecution".into(), Tag::Long(v));
    }
}
fn read_last_execution(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Long(v) = t else { return false };
    let Some(mut b) = w.get_mut::<CommandBlock>(e) else { return false };
    b.last_execution = (*v != -1).then_some(*v);
    true
}
fn write_ticked(w: &World, e: Entity, c: &mut Compound) {
    if let Some(v) = w.get::<Ticked>(e) {
        c.insert("HasTicked".into(), Tag::Byte(i8::from(v.0)));
    }
}
fn read_ticked(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut k) = w.get_mut::<Ticked>(e) else { return false };
    k.0 = *v != 0;
    true
}
fn write_flipped(w: &World, e: Entity, c: &mut Compound) {
    if let Some(v) = w.get::<Flipped>(e) {
        c.insert("FlippedRotation".into(), Tag::Byte(i8::from(v.0)));
    }
}
fn read_flipped(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut k) = w.get_mut::<Flipped>(e) else { return false };
    k.0 = *v != 0;
    true
}
fn write_track(w: &World, e: Entity, c: &mut Compound) {
    if let Some(b) = w.get::<CommandBlock>(e) {
        c.insert("TrackOutput".into(), Tag::Byte(i8::from(b.track)));
    }
}
fn read_track(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut b) = w.get_mut::<CommandBlock>(e) else { return false };
    b.track = *v != 0;
    true
}

/// The spawner keys: each is kept in `Spawner::data` as saved, and written back under its own name.
macro_rules! spawner_key {
    ($key:literal, $write:ident, $read:ident) => {
        fn $write(w: &World, e: Entity, c: &mut Compound) {
            if let Some(v) = w.get::<Spawner>(e).and_then(|s| s.data.get(&JavaString::from($key))) {
                c.insert($key.into(), v.clone());
            }
        }
        fn $read(w: &mut World, e: Entity, t: &Tag) -> bool {
            let Some(mut s) = w.get_mut::<Spawner>(e) else { return false };
            s.data.insert(JavaString::from($key), t.clone());
            true
        }
    };
}
spawner_key!("Delay", write_delay, read_delay);
spawner_key!("SpawnData", write_spawn_data, read_spawn_data);
spawner_key!("SpawnPotentials", write_spawn_potentials, read_spawn_potentials);
spawner_key!("SpawnCount", write_spawn_count, read_spawn_count);
spawner_key!("SpawnRange", write_spawn_range, read_spawn_range);
spawner_key!("MinSpawnDelay", write_min_delay, read_min_delay);
spawner_key!("MaxSpawnDelay", write_max_delay, read_max_delay);
spawner_key!("MaxNearbyEntities", write_max_nearby, read_max_nearby);
spawner_key!("RequiredPlayerRange", write_required_range, read_required_range);

/// The keys of the carts' state. A key is applied to an entity whose component it names; a component the entity lacks leaves its key
/// free (kept as saved, as the framework keeps any unknown key).
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("Enabled"), write: write_enabled, read: read_enabled, accepts: byte, owner: Some(owner::<Enabled>) },
    Field { key: Cow::Borrowed("Items"), write: write_items, read: read_items, accepts: list, owner: Some(owner::<Contents>) },
    Field { key: Cow::Borrowed("LootTable"), write: write_loot_table, read: read_loot_table, accepts: accept::string, owner: Some(owner::<Contents>) },
    Field { key: Cow::Borrowed("LootTableSeed"), write: write_loot_seed, read: read_loot_seed, accepts: accept::long, owner: Some(owner::<Contents>) },
    Field { key: Cow::Borrowed("DisplayState"), write: write_display_state, read: read_display_state, accepts: compound, owner: Some(owner::<Display>) },
    Field { key: Cow::Borrowed("DisplayOffset"), write: write_display_offset, read: read_display_offset, accepts: accept::int, owner: Some(owner::<Display>) },
    Field { key: Cow::Borrowed("explosion_power"), write: write_power_base, read: read_power_base, accepts: accept::float, owner: Some(owner::<BlastPower>) },
    Field { key: Cow::Borrowed("explosion_speed_factor"), write: write_power_speed, read: read_power_speed, accepts: accept::float, owner: Some(owner::<BlastPower>) },
    Field { key: Cow::Borrowed("Command"), write: write_command, read: read_command, accepts: accept::string, owner: Some(owner::<CommandBlock>) },
    Field { key: Cow::Borrowed("LastOutput"), write: write_last_output, read: read_last_output, accepts: accept::string, owner: Some(owner::<CommandBlock>) },
    Field { key: Cow::Borrowed("TrackOutput"), write: write_track, read: read_track, accepts: byte, owner: Some(owner::<CommandBlock>) },
    Field { key: Cow::Borrowed("SuccessCount"), write: write_success, read: read_success, accepts: accept::int, owner: Some(owner::<CommandBlock>) },
    Field { key: Cow::Borrowed("CustomName"), write: write_custom_name, read: read_custom_name, accepts: accept::any, owner: Some(owner::<CommandBlock>) },
    Field { key: Cow::Borrowed("UpdateLastExecution"), write: write_update_last, read: read_update_last, accepts: byte, owner: Some(owner::<CommandBlock>) },
    Field { key: Cow::Borrowed("LastExecution"), write: write_last_execution, read: read_last_execution, accepts: accept::long, owner: Some(owner::<CommandBlock>) },
    Field { key: Cow::Borrowed("HasTicked"), write: write_ticked, read: read_ticked, accepts: byte, owner: Some(owner::<Ticked>) },
    Field { key: Cow::Borrowed("FlippedRotation"), write: write_flipped, read: read_flipped, accepts: byte, owner: Some(owner::<Flipped>) },
    Field { key: Cow::Borrowed("Delay"), write: write_delay, read: read_delay, accepts: accept::any, owner: Some(owner::<Spawner>) },
    Field { key: Cow::Borrowed("SpawnData"), write: write_spawn_data, read: read_spawn_data, accepts: accept::any, owner: Some(owner::<Spawner>) },
    Field { key: Cow::Borrowed("SpawnPotentials"), write: write_spawn_potentials, read: read_spawn_potentials, accepts: accept::any, owner: Some(owner::<Spawner>) },
    Field { key: Cow::Borrowed("SpawnCount"), write: write_spawn_count, read: read_spawn_count, accepts: accept::any, owner: Some(owner::<Spawner>) },
    Field { key: Cow::Borrowed("SpawnRange"), write: write_spawn_range, read: read_spawn_range, accepts: accept::any, owner: Some(owner::<Spawner>) },
    Field { key: Cow::Borrowed("MinSpawnDelay"), write: write_min_delay, read: read_min_delay, accepts: accept::any, owner: Some(owner::<Spawner>) },
    Field { key: Cow::Borrowed("MaxSpawnDelay"), write: write_max_delay, read: read_max_delay, accepts: accept::any, owner: Some(owner::<Spawner>) },
    Field { key: Cow::Borrowed("MaxNearbyEntities"), write: write_max_nearby, read: read_max_nearby, accepts: accept::any, owner: Some(owner::<Spawner>) },
    Field { key: Cow::Borrowed("RequiredPlayerRange"), write: write_required_range, read: read_required_range, accepts: accept::any, owner: Some(owner::<Spawner>) },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::Herd;
    use crate::nbt;

    fn world() -> Herd {
        Herd::new(0, 1.0, true, false, 1)
    }

    #[test]
    fn each_genre_saves_its_keys_and_loads_them_back() {
        let mut h = world();
        for (_, kind) in crate::species::minecart::GENRES {
            let e = spawn(&mut h.world, kind, [1.0, 2.0, 3.0], 90.0);
            let c = save(&h.world, e).expect("saved");
            assert_eq!(c.get(&JavaString::from("id")), Some(&Tag::String(name_of(kind).into())));
            let twin = spawn(&mut h.world, kind, [1.0, 2.0, 3.0], 0.0);
            assert_eq!(nbt::load(&mut h.world, twin, &c), Some(0));
            assert_eq!(save(&h.world, twin), Some(c));
        }
    }

    #[test]
    fn a_chest_has_27_slots_and_a_hopper_saves_its_enabled_flag() {
        let mut h = world();
        let chest = spawn(&mut h.world, Kind::Chest, [0.5, 1.0, 0.5], 0.0);
        assert_eq!(h.world.get::<Contents>(chest).map(|k| k.slots.len()), Some(27));
        let hopper = spawn(&mut h.world, Kind::Hopper, [0.5, 1.0, 0.5], 0.0);
        h.world.get_mut::<Enabled>(hopper).expect("enabled").0 = false;
        let c = save(&h.world, hopper).expect("saved");
        assert_eq!(c.get(&JavaString::from("Enabled")), Some(&Tag::Byte(0)));
        assert_eq!(c.get(&JavaString::from("Items")), Some(&Tag::List(vec![])));
    }

    #[test]
    fn a_generator_cart_saves_its_spawner_keys() {
        let mut h = world();
        let e = spawn(&mut h.world, Kind::Spawner, [0.5, 1.0, 0.5], 0.0);
        h.world.get_mut::<Spawner>(e).expect("spawner").data.insert(JavaString::from("Delay"), Tag::Short(20));
        let c = save(&h.world, e).expect("saved");
        assert_eq!(c.get(&JavaString::from("Delay")), Some(&Tag::Short(20)));
    }

    #[test]
    fn a_command_cart_saves_its_seven_keys_and_reads_them_back() {
        let mut h = world();
        let e = spawn(&mut h.world, Kind::CommandBlock, [0.5, 1.0, 0.5], 0.0);
        let state = CommandBlock {
            command: "say hi".into(),
            last_output: "done".into(),
            track: true,
            success: 3,
            custom_name: Some(Tag::String("Bob".into())),
            update_last_execution: true,
            last_execution: Some(1200),
            last_activated: None,
        };
        h.world.entity_mut(e).insert(state.clone());
        let c = save(&h.world, e).expect("saved");
        let keys = [
            ("Command", Tag::String("say hi".into())),
            ("SuccessCount", Tag::Int(3)),
            ("CustomName", Tag::String("Bob".into())),
            ("TrackOutput", Tag::Byte(1)),
            ("LastOutput", Tag::String("done".into())),
            ("UpdateLastExecution", Tag::Byte(1)),
            ("LastExecution", Tag::Long(1200)),
        ];
        for (k, v) in keys {
            assert_eq!(c.get(&JavaString::from(k)), Some(&v), "{k}");
        }
        let twin = spawn(&mut h.world, Kind::CommandBlock, [0.5, 1.0, 0.5], 0.0);
        assert_eq!(nbt::load(&mut h.world, twin, &c), Some(0));
        assert_eq!(h.world.get::<CommandBlock>(twin), Some(&state));
    }

    #[test]
    fn a_cart_off_the_rails_is_not_moving_so_it_has_no_deadline() {
        let mut h = world();
        let e = spawn(&mut h.world, Kind::Furnace, [0.5, 1.0, 0.5], 0.0);
        assert!(h.world.get::<Moving>(e).is_none());
    }
}
