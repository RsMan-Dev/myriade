//! Les champs NBT propres aux créatures, déclarés par comportement (un composant, des clés). Un champ ne s'applique qu'à une entité qui
//! porte son composant : sans lui, la clé reste libre (rien n'est inventé).
use crate::behaviours::attached_face::Attached;
use crate::behaviours::bee::Bee;
use crate::behaviours::hunt_prey::{CannotHunt, Hunt};
use crate::behaviours::cat_state::Trusting;
use crate::behaviours::pufferfish_puff::Puff;
use crate::behaviours::long_jump::LongJumpState;
use crate::behaviours::ram::{Rammer, Shed};
use crate::goals::{Occupied, OCC_SITTING};
use crate::behaviours::trade_offers::{profession_name, variant_name, Merchant, UnresolvedProfession, UNRESOLVED};
use crate::behaviours::do_nothing::{counter, Invulnerable};
use crate::behaviours::forget_target::LlamaStrength;
use crate::cow::{arm_at, now_of, Due, GrowAt, TypeName};
use crate::explosion::{Charged, Fuse};
use crate::farm::{Egg, Wool};
use crate::behaviours::bat_flutter::{Flutter, Hanging};
use crate::behaviours::follow_caravan::Caravan;
use crate::behaviours::invulnerable_charge::Charge;
use crate::behaviours::swallow_item::Swallowable;
use crate::behaviours::swim_to_treasure::Trip;
use crate::behaviours::fox_state::{Crouch, FoxFlags, CROUCHING as FOX_CROUCHING, SITTING as FOX_SITTING};
use crate::behaviours::swim_to_treasure::GotFish;
use crate::behaviours::lay_egg::Carrying;
use crate::behaviours::peek::Peek;
use crate::behaviours::turtle_state::Turtle;
use crate::behaviours::still_under_riders::{Slot, StillUntil};
use crate::behaviours::swallow_hold::SearchAfter;
use crate::nbt::{accept, owner, Field};
use crate::size::Size;
use crate::species::goat::Screaming;
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, JavaString, Tag};
use std::borrow::Cow;

fn byte(t: &Tag) -> bool {
    matches!(t, Tag::Byte(_))
}
/// `CannotHunt` is read as a byte or an int (`AbstractPiglin.readAdditionalSaveData` takes either), written as a byte.
fn byte_or_int(t: &Tag) -> bool {
    matches!(t, Tag::Byte(_) | Tag::Int(_))
}

/// The declared fields, by behaviour: sized (`Size`), creeper (`Fuse`, `ExplosionRadius`, `powered`), wither (`Invul`), llama (`Strength`).
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("Size"), write: write_size, read: read_size, accepts: accept::int, owner: Some(owner::<Size>) },
    Field { key: Cow::Borrowed("Fuse"), write: write_fuse, read: read_fuse, accepts: accept::short, owner: Some(owner::<Fuse>) },
    Field { key: Cow::Borrowed("ExplosionRadius"), write: write_radius, read: read_radius, accepts: byte, owner: Some(owner::<Fuse>) },
    Field { key: Cow::Borrowed("ignited"), write: write_ignited, read: read_ignited, accepts: byte, owner: Some(owner::<Fuse>) },
    Field { key: Cow::Borrowed("powered"), write: write_powered, read: read_powered, accepts: byte, owner: Some(owner::<Fuse>) },
    Field { key: Cow::Borrowed("Invul"), write: write_invul, read: read_invul, accepts: accept::int, owner: Some(owner::<Charge>) },
    Field { key: Cow::Borrowed("IsScreamingGoat"), write: write_screaming, read: read_screaming, accepts: byte, owner: Some(owner::<Rammer>) },
    Field { key: Cow::Borrowed("HasLeftHorn"), write: write_left_horn, read: read_left_horn, accepts: byte, owner: Some(owner::<Rammer>) },
    Field { key: Cow::Borrowed("HasRightHorn"), write: write_right_horn, read: read_right_horn, accepts: byte, owner: Some(owner::<Rammer>) },
    Field { key: Cow::Borrowed("CannotHunt"), write: write_cannot_hunt, read: read_cannot_hunt, accepts: byte_or_int, owner: Some(owner::<Hunt>) },
    Field { key: Cow::Borrowed("Strength"), write: write_strength, read: read_strength, accepts: accept::int, owner: Some(owner::<Caravan>) },
    Field { key: Cow::Borrowed("DespawnDelay"), write: write_despawn_delay, read: read_despawn_delay, accepts: accept::int, owner: Some(owner::<crate::despawn_delay::DespawnDelay>) },
    Field { key: Cow::Borrowed("Age"), write: write_age, read: read_age, accepts: accept::int, owner: Some(owner::<GrowAt>) },
    Field { key: Cow::Borrowed("AttachFace"), write: write_face, read: read_face, accepts: byte, owner: Some(owner::<Peek>) },
    Field { key: Cow::Borrowed("Color"), write: write_color, read: read_color, accepts: byte, owner: Some(owner::<Peek>) },
    Field { key: Cow::Borrowed("Peek"), write: write_peek, read: read_peek, accepts: byte, owner: Some(owner::<Peek>) },
    Field { key: Cow::Borrowed("has_egg"), write: write_has_egg, read: read_has_egg, accepts: byte, owner: Some(owner::<Turtle>) },
    Field { key: Cow::Borrowed("VillagerData"), write: write_villager, read: read_villager, accepts: compound, owner: Some(owner::<Merchant>) },
    Field { key: Cow::Borrowed("AgeLocked"), write: write_age_locked, read: read_age_locked, accepts: byte, owner: Some(owner::<GrowAt>) },
    Field { key: Cow::Borrowed("IsBaby"), write: write_baby, read: read_baby, accepts: byte, owner: Some(owner::<GrowAt>) },
    Field { key: Cow::Borrowed("HasNectar"), write: write_nectar, read: read_nectar, accepts: byte, owner: Some(owner::<Bee>) },
    Field { key: Cow::Borrowed("HasStung"), write: write_stung, read: read_stung, accepts: byte, owner: Some(owner::<Bee>) },
    Field { key: Cow::Borrowed("CannotEnterHiveTicks"), write: write_stay_out, read: read_stay_out, accepts: accept::int, owner: Some(owner::<Bee>) },
    Field { key: Cow::Borrowed("hive_pos"), write: write_hive, read: read_hive, accepts: int_array, owner: Some(owner::<Bee>) },
    Field { key: Cow::Borrowed("Trusting"), write: write_trusting, read: read_trusting, accepts: byte, owner: None },
    Field { key: Cow::Borrowed("CropsGrownSincePollination"), write: write_crops, read: read_crops, accepts: accept::int, owner: Some(owner::<Bee>) },
    Field { key: Cow::Borrowed("PuffState"), write: write_puff, read: read_puff, accepts: accept::int, owner: Some(owner::<Puff>) },
    Field { key: Cow::Borrowed("Sitting"), write: write_sitting, read: read_sitting, accepts: byte, owner: Some(owner::<crate::behaviours::sit_when_ordered::SitRules>) },
    Field { key: Cow::Borrowed("flower_pos"), write: write_flower, read: read_flower, accepts: int_array, owner: Some(owner::<Bee>) },
    Field { key: Cow::Borrowed("EggLayTime"), write: write_egg_time, read: read_egg_time, accepts: accept::int, owner: Some(owner::<Egg>) },
    Field { key: Cow::Borrowed("scute_time"), write: write_scute_time, read: read_scute_time, accepts: accept::int, owner: Some(owner::<crate::gift_drop::GiftDue>) },
    Field { key: Cow::Borrowed("size"), write: write_phantom_size, read: read_phantom_size, accepts: accept::int, owner: None },
    Field { key: Cow::Borrowed("anchor_pos"), write: write_phantom_anchor, read: read_phantom_anchor, accepts: int_array, owner: None },
    Field { key: Cow::Borrowed("LastPoseTick"), write: write_last_pose, read: read_last_pose, accepts: accept::long, owner: Some(owner::<crate::behaviours::posture::Posture>) },
    Field { key: Cow::Borrowed("Color"), write: write_wool_color, read: read_wool_color, accepts: byte, owner: Some(owner::<Wool>) },
    Field { key: Cow::Borrowed("Sitting"), write: write_fox_sitting, read: read_fox_sitting, accepts: byte, owner: Some(owner::<FoxFlags>) },
    Field { key: Cow::Borrowed("Crouching"), write: write_fox_crouching, read: read_fox_crouching, accepts: byte, owner: Some(owner::<FoxFlags>) },
    Field { key: Cow::Borrowed("GotFish"), write: write_got_fish, read: read_got_fish, accepts: byte, owner: Some(owner::<Trip>) },
    Field { key: Cow::Borrowed("TicksSincePollination"), write: write_pollination, read: read_pollination, accepts: accept::int, owner: Some(owner::<Bee>) },
    Field { key: Cow::Borrowed("BatFlags"), write: write_bat_flags, read: read_bat_flags, accepts: byte, owner: Some(owner::<Flutter>) },
    Field { key: Cow::Borrowed("still_timeout"), write: write_still, read: read_still, accepts: accept::int, owner: Some(owner::<Slot>) },
    Field { key: Cow::Borrowed("pickup_timer"), write: write_pickup, read: read_pickup, accepts: accept::int, owner: Some(owner::<Swallowable>) },
];

fn int_array(t: &Tag) -> bool {
    matches!(t, Tag::IntArray(_))
}

fn write_size(w: &World, e: Entity, c: &mut Compound) {
    if let Some(s) = w.get::<Size>(e) {
        c.insert("Size".into(), Tag::Int(s.0 - 1));
    }
}

/// The jar's `setSize(getInt + 1)`, clamped to 1..=127 as the Horizon client does.
fn read_size(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    w.entity_mut(e).insert(Size((v + 1).clamp(1, 127)));
    true
}

/// The maximum swell, as the creeper's `Fuse` short.
fn write_fuse(w: &World, e: Entity, c: &mut Compound) {
    if let Some(f) = w.get::<Fuse>(e) {
        c.insert("Fuse".into(), Tag::Short(f.max as i16));
    }
}

fn read_fuse(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Short(v) = t else { return false };
    let Some(mut f) = w.get_mut::<Fuse>(e) else { return false };
    f.max = i32::from(*v);
    true
}

/// The blast radius, a byte as the jar writes it.
fn write_radius(w: &World, e: Entity, c: &mut Compound) {
    if let Some(f) = w.get::<Fuse>(e) {
        c.insert("ExplosionRadius".into(), Tag::Byte(f.radius as i8));
    }
}

fn read_radius(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut f) = w.get_mut::<Fuse>(e) else { return false };
    f.radius = f32::from(*v);
    true
}

/// `ignited`: `Creeper.readAdditionalSaveData` calls `ignite()` when it is true, the fuse then runs toward the blast.
fn write_ignited(w: &World, e: Entity, c: &mut Compound) {
    c.insert("ignited".into(), Tag::Byte(i8::from(w.get::<Fuse>(e).is_some_and(|f| f.ignited))));
}

fn read_ignited(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut f) = w.get_mut::<Fuse>(e) else { return false };
    if *v == 0 {
        return true;
    }
    f.ignited = true;
    crate::explosion::set_dir(w, e, 1);
    true
}

/// `powered` is the `Charged` marker: a byte of one when it is there, zero otherwise (the creeper always writes it).
fn write_powered(w: &World, e: Entity, c: &mut Compound) {
    c.insert("powered".into(), Tag::Byte(i8::from(w.get::<Charged>(e).is_some())));
}

fn read_powered(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    if *v != 0 {
        w.entity_mut(e).insert(Charged);
    } else {
        w.entity_mut(e).remove::<Charged>();
    }
    true
}

/// The wither's invulnerable countdown, an int.
fn write_invul(w: &World, e: Entity, c: &mut Compound) {
    c.insert("Invul".into(), Tag::Int(counter(w, e)));
}

fn read_invul(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let end = crate::cow::now_of(w, e) + (*v).max(0) as u64;
    w.entity_mut(e).insert(Invulnerable(end));
    true
}

/// `AbstractPiglin.addAdditionalSaveData`: `CannotHunt` is always written, 0 or 1 (the marker), for a hunter.
fn write_cannot_hunt(w: &World, e: Entity, c: &mut Compound) {
    c.insert("CannotHunt".into(), Tag::Byte(w.get::<CannotHunt>(e).is_some() as i8));
}

fn read_cannot_hunt(w: &mut World, e: Entity, t: &Tag) -> bool {
    let v = match t {
        Tag::Byte(v) => *v as i32,
        Tag::Int(v) => *v,
        _ => return false,
    };
    if v != 0 {
        w.entity_mut(e).insert(CannotHunt);
    }
    true
}
/// `Goat.addAdditionalSaveData`: `IsScreamingGoat` is written as a byte only when the marker is there (`false` is the default).
/// `IsScreamingGoat`: a byte, 1 for the screaming goat (its marker), 0 otherwise (the goat always writes it).
fn write_screaming(w: &World, e: Entity, c: &mut Compound) {
    c.insert("IsScreamingGoat".into(), Tag::Byte(i8::from(w.get::<Screaming>(e).is_some())));
}

/// `Goat.addAdditionalSaveData`: `HasLeftHorn` / `HasRightHorn` only while a part is off (`Shed`); a whole goat writes none.
/// `Brain` memories, each the ticks left of its cooldown (`long_jump_cooling_down`, `ram_cooldown_ticks`, `attack_target_cooldown`): written
/// only while each runs.
fn is_phantom(w: &World, e: Entity) -> bool {
    crate::nbt::id_of_entity(w, e) == Some("minecraft:phantom")
}

/// `Phantom.addAdditionalSaveData`: the raw `size` (an int, 0 at the birth) and the `anchor_pos` the phantom circles.
fn write_phantom_size(w: &World, e: Entity, c: &mut Compound) {
    if let Some(s) = w.get::<Size>(e).filter(|_| is_phantom(w, e)) {
        c.insert("size".into(), Tag::Int(s.0));
    }
}

fn read_phantom_size(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    if is_phantom(w, e) {
        w.entity_mut(e).insert(Size(*v));
    }
    true
}

fn write_phantom_anchor(w: &World, e: Entity, c: &mut Compound) {
    if let Some(a) = w.get::<crate::cow::Anchor>(e).filter(|_| is_phantom(w, e)) {
        c.insert("anchor_pos".into(), Tag::IntArray(a.0.to_vec()));
    }
}

fn read_phantom_anchor(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::IntArray(v) = t else { return false };
    if let (true, [x, y, z]) = (is_phantom(w, e), v.as_slice()) {
        w.entity_mut(e).insert(crate::cow::Anchor([*x, *y, *z]));
    }
    true
}

pub(super) fn write_brain(w: &World, e: Entity, c: &mut Compound) {
    use crate::behaviours::{long_jump, nearest_target, ram};
    let now = now_of(w, e);
    let dig = w.get::<crate::behaviours::act_phase::ActPhase>(e).map(|a| a.dig_cooldown_until.saturating_sub(now)).filter(|&t| t > 0);
    let left = [
        ("minecraft:long_jump_cooling_down", long_jump::cooldown_left(w, e)),
        ("minecraft:ram_cooldown_ticks", ram::cooldown_left(w, e)),
        ("minecraft:attack_target_cooldown", nearest_target::attack_cooldown_left(w, e)),
        ("minecraft:dig_cooldown", dig),
    ];
    // `GlobalPos` of the brute's home: its dimension and its block (`PiglinBruteAi.initMemories`).
    let home = w.get::<crate::behaviours::brute_stroll_home::Home>(e).map(|h| {
        let dim = w.get_resource::<crate::bridge::host::WorldDimension>().map_or("minecraft:overworld", |d| d.0);
        ("minecraft:home", Tag::compound([("dimension", Tag::String(dim.into())), ("pos", Tag::IntArray(h.block.to_vec()))]))
    });
    if left.iter().all(|(_, t)| t.is_none()) && home.is_none() {
        return;
    }
    let Tag::Compound(brain) = c.entry("Brain".into()).or_insert_with(|| Tag::Compound(Default::default())) else { return };
    let Tag::Compound(memories) = brain.entry("memories".into()).or_insert_with(|| Tag::Compound(Default::default())) else { return };
    for (key, t) in left {
        if let Some(t) = t {
            memories.insert(key.into(), Tag::compound([("value", Tag::Int(t as i32))]));
        }
    }
    if let Some((key, pos)) = home {
        memories.insert(key.into(), Tag::compound([("value", pos)]));
    }
}

/// Reads the memories back: their ticks left, or none (the cooldown is over) when a memory is absent.
pub(super) fn read_brain(w: &mut World, e: Entity, t: &Tag) -> bool {
    use crate::behaviours::{long_jump, nearest_target, ram};
    if !matches!(t, Tag::Compound(_)) {
        return false;
    }
    let mem = |key: &str| t.get("memories").and_then(|m| m.get(key)).and_then(|m| m.get("value"));
    let left = |key: &str| match mem(key) {
        Some(Tag::Int(v)) => Some(i64::from(*v).max(0) as u64),
        _ => None,
    };
    long_jump::set_cooldown_left(w, e, left("minecraft:long_jump_cooling_down"));
    ram::set_cooldown_left(w, e, left("minecraft:ram_cooldown_ticks"));
    nearest_target::set_attack_cooldown_left(w, e, left("minecraft:attack_target_cooldown"));
    let now = now_of(w, e);
    if let Some(mut a) = w.get_mut::<crate::behaviours::act_phase::ActPhase>(e) {
        a.dig_cooldown_until = now + left("minecraft:dig_cooldown").unwrap_or(0);
    }
    let pos = mem("minecraft:home").and_then(|h| h.get("pos"));
    if let (Some(Tag::IntArray(p)), Some(mut h)) = (pos, w.get_mut::<crate::behaviours::brute_stroll_home::Home>(e)) {
        if let [x, y, z] = p.as_slice() {
            h.block = [*x, *y, *z];
        }
    }
    true
}

fn write_left_horn(w: &World, e: Entity, c: &mut Compound) {
    c.insert("HasLeftHorn".into(), Tag::Byte(i8::from(w.get::<Shed>(e).map_or(0, |s| s.0) & 1 == 0)));
}

fn write_right_horn(w: &World, e: Entity, c: &mut Compound) {
    c.insert("HasRightHorn".into(), Tag::Byte(i8::from(w.get::<Shed>(e).map_or(0, |s| s.0) & 2 == 0)));
}

fn read_left_horn(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    set_horn(w, e, 1, *v == 0);
    true
}

fn read_right_horn(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    set_horn(w, e, 2, *v == 0);
    true
}

/// Sets (`lost`) or clears the bit of one part in `Shed`, and takes the component away when no part is off.
fn set_horn(w: &mut World, e: Entity, bit: u8, lost: bool) {
    let s = w.get::<Shed>(e).map_or(0, |s| s.0);
    let s = if lost { s | bit } else { s & !bit };
    if s == 0 {
        w.entity_mut(e).remove::<Shed>();
    } else {
        w.entity_mut(e).insert(Shed(s));
    }
}

fn read_screaming(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    if *v != 0 {
        w.entity_mut(e).insert(Screaming);
    } else {
        w.entity_mut(e).remove::<Screaming>();
    }
    true
}

/// The llama's strength, an int: `LlamaStrength`, drawn at the pose for a llama and a trader's llama (`cow::spawn::spawn_declared`).
/// Written only while the component is there: no default is invented.
fn write_strength(w: &World, e: Entity, c: &mut Compound) {
    if let Some(s) = w.get::<LlamaStrength>(e) {
        c.insert("Strength".into(), Tag::Int(s.0));
    }
}

fn read_strength(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    w.entity_mut(e).insert(LlamaStrength(*v));
    true
}

/// `WanderingTrader.despawnDelay`: the ticks left, written only while the trader carries its delay.
fn write_despawn_delay(w: &World, e: Entity, c: &mut Compound) {
    if let Some(d) = w.get::<crate::despawn_delay::DespawnDelay>(e) {
        c.insert("DespawnDelay".into(), Tag::Int(i32::try_from(d.due.saturating_sub(now_of(w, e))).unwrap_or(i32::MAX)));
    }
}

fn read_despawn_delay(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    crate::despawn_delay::restore(w, e, *v);
    true
}

/// `Age`: negative, the ticks left as a baby (`GrowAt` is the tick it grows at); zero or more, grown up (`GrowAt` goes).
fn write_age(w: &World, e: Entity, c: &mut Compound) {
    if let Some(g) = w.get::<GrowAt>(e) {
        let left = w.get::<crate::age_lock::AgeLocked>(e).map_or_else(|| g.0.saturating_sub(now_of(w, e)), |l| l.0);
        c.insert("Age".into(), Tag::Int(-i32::try_from(left).unwrap_or(i32::MAX)));
    }
}

/// The keys whose presence poses `GrowAt` on an adult (`nbt::register_creators`): a baby made by `/summon` or `/data merge`.
pub const CREATORS: &[Field] = &[
    Field { key: Cow::Borrowed("VillagerData"), write: write_zombie_data, read: read_zombie_data, accepts: compound, owner: Some(owner::<crate::bridge::sync_a::ZombieVillagerData>) },
    Field { key: Cow::Borrowed("MainGene"), write: write_main_gene, read: read_main_gene, accepts: is_string, owner: Some(owner::<crate::behaviours::spawn_draw::Genes>) },
    Field { key: Cow::Borrowed("HiddenGene"), write: write_hidden_gene, read: read_hidden_gene, accepts: is_string, owner: Some(owner::<crate::behaviours::spawn_draw::Genes>) },
    Field { key: Cow::Borrowed("IsImmuneToZombification"), write: write_immune, read: read_immune, accepts: byte, owner: Some(owner::<crate::behaviours::convert_timer::ZombImmune>) },
    Field { key: Cow::Borrowed("Age"), write: write_age, read: create_age, accepts: accept::int, owner: Some(owner::<GrowAt>) },
    Field { key: Cow::Borrowed("IsBaby"), write: write_baby, read: read_baby, accepts: byte, owner: Some(owner::<GrowAt>) },
];

/// `ZombieVillager.readAdditionalSaveData`: `VillagerData` (type, profession, level), by registry name.
fn write_zombie_data(w: &World, e: Entity, c: &mut Compound) {
    use crate::bridge::sync_a::{PROFESSION_IDS, VILLAGER_TYPE_IDS};
    let Some(d) = w.get::<crate::bridge::sync_a::ZombieVillagerData>(e) else { return };
    let mut m = Compound::new();
    m.insert("type".into(), Tag::String(JavaString::from(format!("minecraft:{}", VILLAGER_TYPE_IDS[d.0[0] as usize]).as_str())));
    m.insert("profession".into(), Tag::String(JavaString::from(format!("minecraft:{}", PROFESSION_IDS[d.0[1] as usize]).as_str())));
    m.insert("level".into(), Tag::Int(d.0[2]));
    c.insert("VillagerData".into(), Tag::Compound(m));
}

fn read_zombie_data(w: &mut World, e: Entity, t: &Tag) -> bool {
    use crate::bridge::sync_a::{PROFESSION_IDS, VILLAGER_TYPE_IDS};
    let Tag::Compound(d) = t else { return false };
    if !w.get::<TypeName>(e).is_some_and(|n| n.0 == "minecraft:zombie_villager") {
        return false;
    }
    let id = |k: &str, list: &[&str], default: i32| match d.get(&JavaString::from(k)) {
        Some(Tag::String(s)) => s.to_string_lossy().strip_prefix("minecraft:").and_then(|n| list.iter().position(|x| *x == n)).map_or(default, |i| i as i32),
        _ => default,
    };
    let level = match d.get(&JavaString::from("level")) {
        Some(Tag::Int(l)) => (*l).clamp(1, 5),
        _ => 1,
    };
    w.entity_mut(e).insert(crate::bridge::sync_a::ZombieVillagerData([id("type", &VILLAGER_TYPE_IDS, 2), id("profession", &PROFESSION_IDS, 0), level]));
    true
}

fn write_immune(w: &World, e: Entity, c: &mut Compound) {
    if w.get::<crate::behaviours::convert_timer::ZombImmune>(e).is_some() {
        c.insert("IsImmuneToZombification".into(), Tag::Byte(1));
    }
}

/// `IsImmuneToZombification`: for the species that can be zombified (they carry a `Convert`).
fn read_immune(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    if w.get::<crate::behaviours::convert_timer::Convert>(e).is_none() {
        return false;
    }
    if *v != 0 {
        w.entity_mut(e).insert(crate::behaviours::convert_timer::ZombImmune);
    } else {
        w.entity_mut(e).remove::<crate::behaviours::convert_timer::ZombImmune>();
    }
    true
}

const GENES: [&str; 7] = ["normal", "lazy", "worried", "playful", "brown", "weak", "aggressive"];

fn is_string(t: &Tag) -> bool {
    matches!(t, Tag::String(_))
}

fn write_main_gene(w: &World, e: Entity, c: &mut Compound) {
    if let Some(g) = w.get::<crate::behaviours::spawn_draw::Genes>(e) {
        c.insert("MainGene".into(), Tag::String(GENES[gene_index(g.main)].into()));
    }
}

fn write_hidden_gene(w: &World, e: Entity, c: &mut Compound) {
    if let Some(g) = w.get::<crate::behaviours::spawn_draw::Genes>(e) {
        c.insert("HiddenGene".into(), Tag::String(GENES[gene_index(g.hidden)].into()));
    }
}

fn gene_index(g: crate::behaviours::panda_state::Gene) -> usize {
    (0..7).find(|&i| crate::behaviours::spawn_draw::gene_of(i as u32) == g).unwrap_or(0)
}

/// `Panda.readAdditionalSaveData`: a gene by its name, then the shown gene and the attributes from the two.
fn read_gene(w: &mut World, e: Entity, t: &Tag, main: bool) -> bool {
    use crate::behaviours::spawn_draw::{gene_of, set_attributes, shown, Genes};
    let Tag::String(s) = t else { return false };
    if !w.get::<TypeName>(e).is_some_and(|t| t.0 == "minecraft:panda") {
        return false;
    }
    let name = s.to_string_lossy();
    let Some(i) = GENES.iter().position(|n| *n == name) else { return false };
    let mut g = w.get::<Genes>(e).copied().unwrap_or_default();
    if main {
        g.main = gene_of(i as u32);
    } else {
        g.hidden = gene_of(i as u32);
    }
    w.entity_mut(e).insert((g, shown(g)));
    set_attributes(w, e);
    true
}

fn read_main_gene(w: &mut World, e: Entity, t: &Tag) -> bool {
    read_gene(w, e, t, true)
}

fn read_hidden_gene(w: &mut World, e: Entity, t: &Tag) -> bool {
    read_gene(w, e, t, false)
}

fn species_tags(w: &World, e: Entity) -> u32 {
    w.get::<crate::cow::SpeciesId>(e).map_or(0, |s| w.resource::<crate::cow::SpeciesTable>().0[s.0 as usize].tags)
}

/// `Age` of a species that grows up; any other species leaves the key free, as before.
fn create_age(w: &mut World, e: Entity, t: &Tag) -> bool {
    species_tags(w, e) & crate::cow::tag::AGEING != 0 && read_age(w, e, t)
}

fn read_age(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    if *v < 0 {
        let ticks = u64::from(v.unsigned_abs());
        let at = now_of(w, e) + ticks;
        // `AgeableMob.readAdditionalSaveData`/`setAge`: the baby box and the deadline on the wheel, not only the age.
        if w.get::<crate::cow::Sizes>(e).is_some() && w.get::<crate::cow::Body>(e).is_some() && w.get::<crate::cow::Pos>(e).is_some() {
            crate::behaviours::follow_parent::make_baby(w, e, ticks);
        }
        w.entity_mut(e).insert(GrowAt(at));
        if w.get::<crate::age_lock::AgeLocked>(e).is_some() {
            w.entity_mut(e).insert(crate::age_lock::AgeLocked(u64::from(v.unsigned_abs())));
        }
    } else {
        w.entity_mut(e).remove::<GrowAt>();
    }
    true
}

/// The shulker's lid: the face it sits on, and its colour (bytes).
fn write_face(w: &World, e: Entity, c: &mut Compound) {
    if let Some(a) = w.get::<Attached>(e) {
        c.insert("AttachFace".into(), Tag::Byte(a.face as i8));
    }
}

fn read_face(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut a) = w.get_mut::<Attached>(e) else { return false };
    a.face = *v as u8;
    true
}

/// `Peek` of a shulker: the raw lid opening (0 to 100), from `Attached` (`peek` moves it).
fn write_peek(w: &World, e: Entity, c: &mut Compound) {
    if let Some(a) = w.get::<Attached>(e) {
        c.insert("Peek".into(), Tag::Byte(a.raw as i8));
    }
}

fn read_peek(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut a) = w.get_mut::<Attached>(e) else { return false };
    a.raw = i32::from(*v);
    true
}

/// `has_egg` of a turtle: a byte of one while it carries its egg (`Carrying`, the lay load).
fn write_has_egg(w: &World, e: Entity, c: &mut Compound) {
    c.insert("has_egg".into(), Tag::Byte(i8::from(w.get::<Carrying>(e).is_some())));
}

fn read_has_egg(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    if *v != 0 {
        w.entity_mut(e).insert(Carrying);
    } else {
        w.entity_mut(e).remove::<Carrying>();
    }
    true
}

fn write_color(w: &World, e: Entity, c: &mut Compound) {
    if let Some(a) = w.get::<Attached>(e) {
        c.insert("Color".into(), Tag::Byte(a.color as i8));
    }
}

fn read_color(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut a) = w.get_mut::<Attached>(e) else { return false };
    a.color = *v as u8;
    true
}

/// The bee's flags: nectar and sting, as bytes.
fn write_nectar(w: &World, e: Entity, c: &mut Compound) {
    if let Some(b) = w.get::<Bee>(e) {
        c.insert("HasNectar".into(), Tag::Byte(i8::from(b.nectar)));
    }
}

fn read_nectar(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut b) = w.get_mut::<Bee>(e) else { return false };
    b.nectar = *v != 0;
    true
}

fn write_stung(w: &World, e: Entity, c: &mut Compound) {
    if let Some(b) = w.get::<Bee>(e) {
        c.insert("HasStung".into(), Tag::Byte(i8::from(b.stung)));
    }
}

fn read_stung(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut b) = w.get_mut::<Bee>(e) else { return false };
    b.stung = *v != 0;
    true
}

/// The ticks left before the bee may enter its hive: the own time it ends at, less now.
fn write_stay_out(w: &World, e: Entity, c: &mut Compound) {
    if let Some(b) = w.get::<Bee>(e) {
        let left = b.stay_out_until.saturating_sub(now_of(w, e));
        c.insert("CannotEnterHiveTicks".into(), Tag::Int(i32::try_from(left).unwrap_or(i32::MAX)));
    }
}

fn read_stay_out(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let now = now_of(w, e);
    let Some(mut b) = w.get_mut::<Bee>(e) else { return false };
    b.stay_out_until = now + u64::try_from(*v).unwrap_or(0);
    true
}

/// The remembered hive and flower: three ints when there is one.
fn write_hive(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<Bee>(e).and_then(|b| b.hive) {
        c.insert("hive_pos".into(), Tag::IntArray(p.to_vec()));
    }
}

fn read_hive(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::IntArray(v) = t else { return false };
    let Ok([x, y, z]) = <[i32; 3]>::try_from(v.as_slice()) else { return false };
    let Some(mut b) = w.get_mut::<Bee>(e) else { return false };
    b.hive = Some([x, y, z]);
    true
}

fn write_flower(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<Bee>(e).and_then(|b| b.flower) {
        c.insert("flower_pos".into(), Tag::IntArray(p.to_vec()));
    }
}

fn read_flower(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::IntArray(v) = t else { return false };
    let Ok([x, y, z]) = <[i32; 3]>::try_from(v.as_slice()) else { return false };
    let Some(mut b) = w.get_mut::<Bee>(e) else { return false };
    b.flower = Some([x, y, z]);
    true
}

fn compound(t: &Tag) -> bool {
    matches!(t, Tag::Compound(_))
}

/// `VillagerData`: the type, the profession by name, and the level. A wandering trader has none.
fn write_villager(w: &World, e: Entity, c: &mut Compound) {
    let Some(m) = w.get::<Merchant>(e) else { return };
    if m.wandering {
        return;
    }
    let mut d = Compound::new();
    d.insert("type".into(), Tag::String(JavaString::from(variant_name(m.variant).as_str())));
    let job = match w.get::<UnresolvedProfession>(e) {
        Some(u) => u.0.clone(),
        None => profession_name(w, m.profession),
    };
    d.insert("profession".into(), Tag::String(JavaString::from(job.as_str())));
    d.insert("level".into(), Tag::Int(i32::from(m.level)));
    c.insert("VillagerData".into(), Tag::Compound(d));
}

/// The index of a type or a profession from its name (the tables are short: a scan is at load only).
fn read_villager(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Compound(d) = t else { return false };
    let text = |k: &str| match d.get(&JavaString::from(k)) {
        Some(Tag::String(s)) => Some(s.to_string_lossy()),
        _ => None,
    };
    let (Some(kind), Some(job)) = (text("type"), text("profession")) else { return false };
    w.entity_mut(e).insert(crate::bridge::sync_a::VillagerFinalized);
    let Some(Tag::Int(level)) = d.get(&JavaString::from("level")) else { return false };
    let (variant, profession) = {
        let ww: &World = w;
        ((0..=u8::MAX).find(|&i| variant_name(i) == kind), (0..=u8::MAX).find(|&p| profession_name(ww, p) == job))
    };
    let Some(variant) = variant else { return false };
    // A profession the table lacks (a mod's) is kept by name and the merchant left opaque, not refused.
    let profession = match profession {
        Some(p) => {
            w.entity_mut(e).remove::<UnresolvedProfession>();
            p
        }
        None => {
            w.entity_mut(e).insert(UnresolvedProfession(job));
            UNRESOLVED
        }
    };
    let Some(mut m) = w.get_mut::<Merchant>(e) else { return false };
    m.variant = variant;
    m.profession = profession;
    m.level = (*level).clamp(1, 5) as u8;
    true
}

/// `IsBaby` is `GrowAt` (the baby's `Age` carries the time): written as a byte, read as given, the age does the work.
fn write_baby(w: &World, e: Entity, c: &mut Compound) {
    if w.get::<GrowAt>(e).is_some() {
        c.insert("IsBaby".into(), Tag::Byte(1));
    }
}

/// `AgeLocked`: the marker that freezes the growth; written for a locked baby only.
fn write_age_locked(w: &World, e: Entity, c: &mut Compound) {
    if w.get::<crate::age_lock::AgeLocked>(e).is_some() {
        c.insert("AgeLocked".into(), Tag::Byte(1));
    }
}

/// Read in either order with `Age`: the ticks left come from `GrowAt` when it is already there, else `read_age` fills them.
fn read_age_locked(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    if *v == 0 {
        w.entity_mut(e).remove::<crate::age_lock::AgeLocked>();
    } else {
        let left = w.get::<GrowAt>(e).map_or(0, |g| g.0.saturating_sub(now_of(w, e)));
        w.entity_mut(e).insert(crate::age_lock::AgeLocked(left));
    }
    true
}

/// `Zombie`/`Piglin.readAdditionalSaveData` `setBaby(IsBaby)`: the baby form that never grows up (the ageing species have `Age`).
fn read_baby(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let flag = species_tags(w, e) & crate::cow::tag::BABY_FLAG != 0;
    if *v != 0 && flag && w.get::<GrowAt>(e).is_none() {
        crate::spawn_finalize::set_baby(w, e, crate::spawn_finalize::NEVER);
    } else if *v == 0 && flag && w.get::<GrowAt>(e).is_some() {
        // `setBaby(false)`: the adult box at once, the flag back to false.
        if let (Some(s), Some(p)) = (w.get::<crate::cow::Sizes>(e).copied(), w.get::<crate::cow::Pos>(e).map(|p| p.0)) {
            if let Some(mut b) = w.get_mut::<crate::cow::Body>(e) {
                b.width = s.adult.0;
                b.height = s.adult.1;
            }
            if let Some(mut b) = w.get_mut::<crate::cow::Bounds>(e) {
                b.0 = crate::cow::box_at(p, s.adult.0, s.adult.1);
            }
        }
        w.entity_mut(e).remove::<GrowAt>();
        crate::attr_mod::grow_up(w, e);
    }
    true
}

/// `Trusting` is a marker: written when present; the read of 1 sets it (an ocelot only).
fn write_trusting(w: &World, e: Entity, c: &mut Compound) {
    if w.get::<Trusting>(e).is_some() {
        c.insert("Trusting".into(), Tag::Byte(1));
    }
}

fn read_trusting(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    if *v != 0 {
        w.entity_mut(e).insert(Trusting);
    }
    true
}

fn write_crops(w: &World, e: Entity, c: &mut Compound) {
    if let Some(b) = w.get::<Bee>(e) {
        c.insert("CropsGrownSincePollination".into(), Tag::Int(b.crops));
    }
}

fn read_crops(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let Some(mut b) = w.get_mut::<Bee>(e) else { return false };
    b.crops = *v;
    true
}

/// The pufferfish's `PuffState` (0 deflated, 1 half, 2 full).
fn write_puff(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<Puff>(e) {
        c.insert("PuffState".into(), Tag::Int(i32::from(p.size)));
    }
}

fn read_puff(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let Some(mut p) = w.get_mut::<Puff>(e) else { return false };
    p.size = (*v).clamp(0, 2) as u8;
    true
}

/// `TamableAnimal.addAdditionalSaveData`: `Sitting` is written by the tamable species only, from the sitting occupation.
fn write_sitting(w: &World, e: Entity, c: &mut Compound) {
    let tamable = w.get::<TypeName>(e).and_then(|n| crate::species::registry().by_name(n.0).ok()).is_some_and(|r| r.caps.contains(&crate::behaviours::Cap::Tamable));
    if tamable {
        let sitting = w.get::<Occupied>(e).is_some_and(|o| o.0 & OCC_SITTING != 0);
        c.insert("Sitting".into(), Tag::Byte(i8::from(sitting)));
    }
}

fn read_sitting(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let now = w.get::<Occupied>(e).map_or(0, |o| o.0);
    let next = if *v != 0 { now | OCC_SITTING } else { now & !OCC_SITTING };
    if next != now || w.get::<Occupied>(e).is_some() {
        w.entity_mut(e).insert(Occupied(next));
    }
    // TamableAnimal.readAdditionalSaveData: `setInSittingPose(Sitting)` as well as the order.
    if *v != 0 {
        w.entity_mut(e).insert(crate::behaviours::sit_when_ordered::SitPose);
    } else {
        w.entity_mut(e).remove::<crate::behaviours::sit_when_ordered::SitPose>();
    }
    true
}

/// `EggLayTime`: the ticks until the next egg (an int), the due tick less now when the timer runs, else the ticks left (`Egg.left`).
fn write_egg_time(w: &World, e: Entity, c: &mut Compound) {
    let Some(g) = w.get::<Egg>(e) else { return };
    let left = if g.at == u64::MAX { g.left } else { g.at.saturating_sub(now_of(w, e)) };
    c.insert("EggLayTime".into(), Tag::Int(i32::try_from(left).unwrap_or(i32::MAX)));
}

/// `scute_time` of an armadillo: the ticks until its next scute (`Armadillo.scuteTime`), the due tick less now.
fn write_scute_time(w: &World, e: Entity, c: &mut Compound) {
    let Some(d) = w.get::<crate::gift_drop::GiftDue>(e) else { return };
    let left = d.0.saturating_sub(now_of(w, e));
    c.insert("scute_time".into(), Tag::Int(i32::try_from(left).unwrap_or(i32::MAX)));
}
fn read_scute_time(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    crate::gift_drop::rearm_gift(w, e, *v);
    true
}

/// A read of a running timer re-arms it at the new due tick; an idle one keeps the ticks left until it runs.
fn read_egg_time(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let ticks = u64::try_from(*v).unwrap_or(0);
    let now = now_of(w, e);
    let at = {
        let Some(mut g) = w.get_mut::<Egg>(e) else { return false };
        if g.at == u64::MAX {
            g.left = ticks;
            return true;
        }
        g.at = now + ticks;
        g.at
    };
    arm_at(w, e, at, Due::Egg);
    true
}

/// `LastPoseTick` of a camel: the game time of its last change of pose, negative while it sits (a long).
fn write_last_pose(w: &World, e: Entity, c: &mut Compound) {
    if let Some(t) = crate::behaviours::posture::last_pose_tick(w, e) {
        c.insert("LastPoseTick".into(), Tag::Long(t));
    }
}
fn read_last_pose(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Long(v) = t else { return false };
    crate::behaviours::posture::load_pose_tick(w, e, *v);
    true
}

/// `Color` of a sheep: the dye of its wool (a byte, 0 to 15).
fn write_wool_color(w: &World, e: Entity, c: &mut Compound) {
    if let Some(wool) = w.get::<Wool>(e) {
        c.insert("Color".into(), Tag::Byte(wool.color as i8));
    }
}

fn read_wool_color(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut wool) = w.get_mut::<Wool>(e) else { return false };
    wool.color = (*v as u8) & 15;
    true
}

/// A bit of the flags of a fox (`fox_state`): written as a byte, read back by setting or clearing the bit.
fn put_fox_bit(w: &World, e: Entity, c: &mut Compound, key: &str, bit: u8) {
    if let Some(f) = w.get::<FoxFlags>(e) {
        c.insert(key.into(), Tag::Byte(i8::from(f.0 & bit != 0)));
    }
}

fn set_fox_bit(w: &mut World, e: Entity, t: &Tag, bit: u8) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut f) = w.get_mut::<FoxFlags>(e) else { return false };
    if *v != 0 { f.0 |= bit } else { f.0 &= !bit }
    true
}

/// `Sitting` of a fox: the sitting bit of its flags.
fn write_fox_sitting(w: &World, e: Entity, c: &mut Compound) {
    put_fox_bit(w, e, c, "Sitting", FOX_SITTING);
}

fn read_fox_sitting(w: &mut World, e: Entity, t: &Tag) -> bool {
    set_fox_bit(w, e, t, FOX_SITTING)
}

/// `Crouching` of a fox: the crouching bit; a load that sets it starts the crouch clock at the load's time.
fn write_fox_crouching(w: &World, e: Entity, c: &mut Compound) {
    put_fox_bit(w, e, c, "Crouching", FOX_CROUCHING);
}

fn read_fox_crouching(w: &mut World, e: Entity, t: &Tag) -> bool {
    let now = now_of(w, e);
    if !set_fox_bit(w, e, t, FOX_CROUCHING) {
        return false;
    }
    if let (Tag::Byte(v), Some(mut c)) = (t, w.get_mut::<Crouch>(e)) {
        if *v != 0 && c.0.is_none() {
            c.0 = Some(now);
        } else if *v == 0 {
            c.0 = None;
        }
    }
    true
}

/// `GotFish` of a dolphin: a byte of one while the marker is there.
/// `GotFish`: a byte, 1 while the dolphin has eaten its fish (the marker), 0 otherwise (`Trip` is its always-present component).
fn write_got_fish(w: &World, e: Entity, c: &mut Compound) {
    c.insert("GotFish".into(), Tag::Byte(i8::from(w.get::<GotFish>(e).is_some())));
}

fn read_got_fish(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    if *v != 0 {
        w.entity_mut(e).insert(GotFish);
    } else {
        w.entity_mut(e).remove::<GotFish>();
    }
    true
}

/// `TicksSincePollination`: the ticks since the bee last had nectar (an int), from `Bee.no_nectar_since` (its own time).
fn write_pollination(w: &World, e: Entity, c: &mut Compound) {
    let Some(b) = w.get::<Bee>(e) else { return };
    let ticks = now_of(w, e).saturating_sub(b.no_nectar_since);
    c.insert("TicksSincePollination".into(), Tag::Int(i32::try_from(ticks).unwrap_or(i32::MAX)));
}

fn read_pollination(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let since = now_of(w, e).saturating_sub(u64::try_from(*v).unwrap_or(0));
    let Some(mut b) = w.get_mut::<Bee>(e) else { return false };
    b.no_nectar_since = since;
    true
}

/// `BatFlags`: bit 0 while the bat hangs (its `Hanging` marker). The perch is the bat's own state: a load keeps it.
fn write_bat_flags(w: &World, e: Entity, c: &mut Compound) {
    c.insert("BatFlags".into(), Tag::Byte(i8::from(w.get::<Hanging>(e).is_some())));
}

fn read_bat_flags(_: &mut World, _: Entity, t: &Tag) -> bool {
    matches!(t, Tag::Byte(_))
}

/// `still_timeout`: the ticks left of the stillness (an int), from `StillUntil` (its own time); 0 without it. `Slot` (the goal's
/// always-present component on the happy ghast) holds the key.
fn write_still(w: &World, e: Entity, c: &mut Compound) {
    let left = w.get::<StillUntil>(e).map_or(0, |s| s.0.saturating_sub(now_of(w, e)));
    c.insert("still_timeout".into(), Tag::Int(i32::try_from(left).unwrap_or(i32::MAX)));
}

fn read_still(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let ticks = u64::try_from(*v).unwrap_or(0);
    if ticks == 0 {
        w.entity_mut(e).remove::<StillUntil>();
    } else {
        let until = now_of(w, e) + ticks;
        w.entity_mut(e).insert(StillUntil(until));
    }
    true
}

/// `pickup_timer`: the ticks left before the cube searches again (an int), from `SearchAfter` (its own time); 0 without the timer, and
/// `Swallowable` (the cube's always-present component) holds the key.
fn write_pickup(w: &World, e: Entity, c: &mut Compound) {
    let left = w.get::<SearchAfter>(e).map_or(0, |s| s.0.saturating_sub(now_of(w, e)));
    c.insert("pickup_timer".into(), Tag::Int(i32::try_from(left).unwrap_or(i32::MAX)));
}

fn read_pickup(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let ticks = u64::try_from(*v).unwrap_or(0);
    if ticks == 0 {
        w.entity_mut(e).remove::<SearchAfter>();
    } else {
        let until = now_of(w, e) + ticks;
        w.entity_mut(e).insert(SearchAfter(until));
    }
    true
}
