//! The keys of the common state of every living entity (`Entity.saveWithoutId`, `LivingEntity`, `Mob`). `Pos`, `Motion`, `Air`, `Health` and
//! `id` are in `nbt::COMMON`. `CREATORS`: a key whose presence poses its component (registered by `nbt::register_creators`). `FIELDS`: a key
//! that applies only to an entity holding its component. Times stored as a tick are written as the ticks left.
use crate::behaviours::breed::Love;
use crate::behaviours::elder_curse::Silent;
use crate::behaviours::neutral_anger::Anger;
use crate::cow::{tag, now_of, Burning, Frozen, HurtAt, LastHurtBy, Yaw};
use crate::despawn::Persistent;
use crate::effects::{self, Effects, Kind, AMBIENT, HIDDEN};
use crate::nbt::{accept, owner, Field};
use crate::spots::Home;
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, JavaString, Tag};
use std::borrow::Cow;

/// The keys that apply to an entity holding their component.
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("HurtTime"), write: write_hurt_time, read: read_hurt_time, accepts: accept::short, owner: Some(owner::<HurtAt>) },
    Field { key: Cow::Borrowed("HurtByTimestamp"), write: write_hurt_by, read: read_hurt_by, accepts: accept::int, owner: Some(owner::<LastHurtBy>) },
    Field { key: Cow::Borrowed("anger_end_time"), write: write_anger_end, read: read_anger_end, accepts: accept::long, owner: Some(owner::<Anger>) },
];

/// The keys whose presence poses their component (`nbt::register_creators`).
pub const CREATORS: &[Field] = &[
    Field { key: Cow::Borrowed("Fire"), write: write_fire, read: read_fire, accepts: accept::short, owner: Some(owner::<Burning>) },
    Field { key: Cow::Borrowed("TicksFrozen"), write: write_frozen, read: read_frozen, accepts: accept::int, owner: Some(owner::<Frozen>) },
    Field { key: Cow::Borrowed("InLove"), write: write_love, read: read_love, accepts: accept::int, owner: Some(owner::<Love>) },
    Field { key: Cow::Borrowed("PersistenceRequired"), write: write_persistent, read: read_persistent, accepts: is_byte, owner: None },
    Field { key: Cow::Borrowed("Rotation"), write: write_rotation, read: read_rotation, accepts: is_list, owner: Some(owner::<Yaw>) },
    Field { key: Cow::Borrowed("Silent"), write: write_silent, read: read_silent, accepts: is_byte, owner: Some(owner::<Silent>) },
    Field { key: Cow::Borrowed("home_pos"), write: write_home_pos, read: read_home_pos, accepts: is_int_array, owner: Some(owner::<Home>) },
    Field { key: Cow::Borrowed("home_radius"), write: write_home_radius, read: read_home_radius, accepts: accept::int, owner: Some(owner::<Home>) },
    Field { key: Cow::Borrowed("active_effects"), write: write_effects, read: read_effects, accepts: is_list, owner: Some(owner::<Effects>) },
];

/// The free keys of common state whose default is the game's: loaded equal to it, they are dropped (`drop_default`).
pub const DEFAULTS: &[(&str, fn() -> Tag)] = &[
    ("Rotation", zero_rotation),
    ("OnGround", zero_byte),
    ("Invulnerable", zero_byte),
    ("PortalCooldown", zero_int),
    ("fall_distance", zero_double),
    ("Fire", zero_short),
];

/// What a living creature's save writes when its state is at the game's default (`LivingEntity` and `Mob.addAdditionalSaveData` write these
/// whatever their value): written only when no field of the entity gave the key. `NoGravity`, `Glowing` and `TicksFrozen` are not here:
/// `Entity.saveWithoutId` writes them only when set.
pub const LIVING_DEFAULTS: &[(&str, fn() -> Tag)] = &[
    ("AbsorptionAmount", zero_float),
    ("DeathTime", zero_short),
    ("HurtTime", zero_short),
    ("FallFlying", zero_byte),
    ("current_impulse_context_reset_grace_time", zero_int),
    ("attributes", empty_list),
    ("Brain", empty_brain),
];

/// What a `Mob` (a creature with goals) writes on top: `Mob.addAdditionalSaveData`.
pub const MOB_DEFAULTS: &[(&str, fn() -> Tag)] = &[("LeftHanded", zero_byte), ("CanPickUpLoot", zero_byte), ("PersistenceRequired", zero_byte)];

/// What a class of creature saves at the game's default, chosen by the class bits of its species (`cow::tag`), never by name: written only
/// when no field of the entity gave the key; a free key equal to it is dropped on load. `gate` lets a state the ecs does hold keep its place.
pub struct Tagged {
    pub tag: u32,
    pub key: &'static str,
    pub default: fn() -> Tag,
    pub gate: fn(&World, Entity) -> bool,
}

fn always(_: &World, _: Entity) -> bool {
    true
}
/// A horse without an owner (a tamed one saves its own `Tame`).
fn untamed(w: &World, e: Entity) -> bool {
    w.get::<crate::cow::SpeciesId>(e).is_none_or(|id| crate::cow::tags_of_id(*id) & tag::CAMEL == 0) && w.get::<crate::tame::Owner>(e).is_none_or(|o| o.0.is_none())
}

pub const TAGGED_DEFAULTS: &[Tagged] = &[
    Tagged { tag: tag::AGEING, key: "Age", default: zero_int, gate: always },
    Tagged { tag: tag::AGEING, key: "AgeLocked", default: zero_byte, gate: always },
    Tagged { tag: tag::FORCED_AGE, key: "ForcedAge", default: zero_int, gate: always },
    Tagged { tag: tag::ANIMAL, key: "InLove", default: zero_int, gate: always },
    Tagged { tag: tag::EQUINE, key: "Tame", default: zero_byte, gate: untamed },
    Tagged { tag: tag::CAMEL, key: "Tame", default: one_byte, gate: always },
    Tagged { tag: tag::EQUINE, key: "Bred", default: zero_byte, gate: always },
    Tagged { tag: tag::EQUINE, key: "EatingHaystack", default: zero_byte, gate: always },
    Tagged { tag: tag::CHESTED, key: "ChestedHorse", default: zero_byte, gate: always },
    Tagged { tag: tag::ZOMBIE, key: "CanBreakDoors", default: zero_byte, gate: always },
    Tagged { tag: tag::ZOMBIE, key: "InWaterTime", default: minus_one, gate: always },
    Tagged { tag: tag::ZOMBIE, key: "DrownedConversionTime", default: minus_one, gate: always },
    Tagged { tag: tag::BABY_FLAG, key: "IsBaby", default: zero_byte, gate: always },
    Tagged { tag: tag::RAIDER, key: "CanJoinRaid", default: zero_byte, gate: always },
    Tagged { tag: tag::RAIDER, key: "PatrolLeader", default: zero_byte, gate: always },
    Tagged { tag: tag::RAIDER, key: "Patrolling", default: zero_byte, gate: always },
    Tagged { tag: tag::RAIDER, key: "Wave", default: zero_int, gate: always },
    Tagged { tag: tag::PICKS_UP, key: "CanPickUpLoot", default: one_byte, gate: always },
    Tagged { tag: tag::ANGER_ZERO, key: "anger_end_time", default: zero_long, gate: always },
    Tagged { tag: tag::ANGER_UNSET, key: "anger_end_time", default: minus_one_long, gate: always },
    Tagged { tag: tag::TADPOLE, key: "FromBucket", default: one_byte, gate: always },
    Tagged { tag: tag::BUCKETABLE, key: "FromBucket", default: zero_byte, gate: always },
    Tagged { tag: tag::SCREAMER, key: "IsScreamingGoat", default: zero_byte, gate: always },
];

/// The tagged defaults that apply to the entity (its species' class bits and the gate).
pub fn tagged_of(w: &World, e: Entity) -> impl Iterator<Item = &'static Tagged> + '_ {
    let bits = w.get::<crate::cow::SpeciesId>(e).map_or(0, |id| crate::cow::tags_of_id(*id));
    TAGGED_DEFAULTS.iter().filter(move |t| bits & t.tag != 0 && (t.gate)(w, e))
}

fn zero_long() -> Tag {
    Tag::Long(0)
}
fn minus_one_long() -> Tag {
    Tag::Long(-1)
}
fn one_byte() -> Tag {
    Tag::Byte(1)
}
fn minus_one() -> Tag {
    Tag::Int(-1)
}

fn zero_rotation() -> Tag {
    Tag::List(vec![Tag::Float(0.0f32.to_bits()), Tag::Float(0.0f32.to_bits())])
}

fn zero_short() -> Tag {
    Tag::Short(0)
}
fn empty_list() -> Tag {
    Tag::List(Vec::new())
}
fn empty_brain() -> Tag {
    Tag::compound([("memories", Tag::Compound(Default::default()))])
}
fn zero_byte() -> Tag {
    Tag::Byte(0)
}
fn zero_int() -> Tag {
    Tag::Int(0)
}
fn zero_double() -> Tag {
    Tag::Double(0.0f64.to_bits())
}
fn zero_float() -> Tag {
    Tag::Float(0.0f32.to_bits())
}

/// True when `key` has a default in `DEFAULTS` and `t` equals it.
pub fn drop_default(key: &str, t: &Tag) -> bool {
    DEFAULTS.iter().any(|(k, d)| *k == key && d() == *t)
}

/// True when `key` has a default that `e` writes (`DEFAULTS`, and the tables of a living creature or a mob when it is one) and `t` equals it:
/// the free key is then not kept, the save writes it again.
pub fn drop_default_of(w: &World, e: Entity, key: &str, t: &Tag) -> bool {
    // A key the species' class gives its own default is judged against that one only.
    if let Some(d) = tagged_of(w, e).find(|d| d.key == key) {
        return (d.default)() == *t;
    }
    let living = w.get::<crate::cow::Health>(e).is_some().then_some(LIVING_DEFAULTS);
    let mob = w.get::<crate::goals::GoalSet>(e).is_some().then_some(MOB_DEFAULTS);
    DEFAULTS.iter().chain(living.into_iter().flatten()).chain(mob.into_iter().flatten()).any(|(k, d)| *k == key && d() == *t)
}

fn is_byte(t: &Tag) -> bool {
    matches!(t, Tag::Byte(_))
}

fn is_list(t: &Tag) -> bool {
    matches!(t, Tag::List(_))
}

fn is_int_array(t: &Tag) -> bool {
    matches!(t, Tag::IntArray(_))
}

/// `Fire`: the ticks left on fire (a short), from `Burning.until`.
fn write_fire(w: &World, e: Entity, c: &mut Compound) {
    let Some(b) = w.get::<Burning>(e) else { return };
    let left = b.until.saturating_sub(now_of(w, e));
    c.insert("Fire".into(), Tag::Short(i16::try_from(left).unwrap_or(i16::MAX)));
}

/// Not on fire when the ticks are not positive: `Burning` goes.
fn read_fire(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Short(v) = t else { return false };
    let left = u64::try_from(*v).unwrap_or(0);
    if left == 0 {
        w.entity_mut(e).remove::<Burning>();
    } else {
        let until = now_of(w, e) + left;
        w.entity_mut(e).insert(Burning { until });
    }
    true
}

fn write_frozen(w: &World, e: Entity, c: &mut Compound) {
    if let Some(f) = w.get::<Frozen>(e).filter(|f| f.0 > 0) {
        c.insert("TicksFrozen".into(), Tag::Int(f.0));
    }
}

fn read_frozen(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    w.entity_mut(e).insert(Frozen(*v));
    true
}

/// `InLove`: the ticks left in love (an int), from `Love` (the tick it ends at).
fn write_love(w: &World, e: Entity, c: &mut Compound) {
    let Some(l) = w.get::<Love>(e) else { return };
    let left = l.0.saturating_sub(now_of(w, e));
    c.insert("InLove".into(), Tag::Int(i32::try_from(left).unwrap_or(i32::MAX)));
}

fn read_love(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let left = u64::try_from(*v).unwrap_or(0);
    if left == 0 {
        w.entity_mut(e).remove::<Love>();
    } else {
        let until = now_of(w, e) + left;
        w.entity_mut(e).insert(Love(until));
    }
    true
}

/// `PersistenceRequired`: a byte, written as 1 by a mob the world keeps (`Persistent`); 0 takes the marker away.
fn write_persistent(w: &World, e: Entity, c: &mut Compound) {
    if w.get::<Persistent>(e).is_some() {
        c.insert("PersistenceRequired".into(), Tag::Byte(1));
    }
}

fn read_persistent(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    if *v == 0 {
        w.entity_mut(e).remove::<Persistent>();
    } else {
        w.entity_mut(e).insert(Persistent);
    }
    true
}

/// The pitch a loaded `Rotation` carried when it is not 0 (a creature's own pitch is not modelled): kept to be written back.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Pitch(pub f32);

/// `Rotation`: [yaw, pitch] as floats; the pitch is the one loaded (`Pitch`, only when not 0), else 0.
fn write_rotation(w: &World, e: Entity, c: &mut Compound) {
    let Some(y) = w.get::<Yaw>(e) else { return };
    let pitch = w.get::<Pitch>(e).map_or(0.0, |p| p.0);
    c.insert("Rotation".into(), Tag::List(vec![Tag::Float(y.0.to_bits()), Tag::Float(pitch.to_bits())]));
}

fn read_rotation(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::List(l) = t else { return false };
    let [Tag::Float(yaw), Tag::Float(pitch)] = l.as_slice() else { return false };
    let pitch = f32::from_bits(*pitch);
    let mut em = w.entity_mut(e);
    em.insert(Yaw(f32::from_bits(*yaw)));
    if pitch == 0.0 {
        em.remove::<Pitch>();
    } else {
        em.insert(Pitch(pitch));
    }
    true
}

/// `Silent`: a byte, 1 while the marker is there.
fn write_silent(w: &World, e: Entity, c: &mut Compound) {
    if w.get::<Silent>(e).is_some() {
        c.insert("Silent".into(), Tag::Byte(1));
    }
}

fn read_silent(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    if *v == 0 {
        w.entity_mut(e).remove::<Silent>();
    } else {
        w.entity_mut(e).insert(Silent);
    }
    true
}

/// `home_pos`: the centre of `Home` as an int array. A read keeps the radius it has.
fn write_home_pos(w: &World, e: Entity, c: &mut Compound) {
    let Some(h) = w.get::<Home>(e) else { return };
    c.insert("home_pos".into(), Tag::IntArray(h.center.to_vec()));
}

fn read_home_pos(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::IntArray(v) = t else { return false };
    let [x, y, z] = v.as_slice() else { return false };
    let radius = w.get::<Home>(e).map_or(0, |h| h.radius);
    w.entity_mut(e).insert(Home { center: [*x, *y, *z], radius });
    true
}

/// `home_radius`: the radius of `Home`, an int. A read keeps the centre it has.
fn write_home_radius(w: &World, e: Entity, c: &mut Compound) {
    let Some(h) = w.get::<Home>(e) else { return };
    c.insert("home_radius".into(), Tag::Int(h.radius));
}

fn read_home_radius(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let center = w.get::<Home>(e).map_or([0; 3], |h| h.center);
    w.entity_mut(e).insert(Home { center, radius: *v });
    true
}

/// `HurtTime`: the ticks of invulnerability left (a short): `HurtAt` gives 10 from its tick. Zero leaves `HurtAt` as it is.
fn write_hurt_time(w: &World, e: Entity, c: &mut Compound) {
    let Some(h) = w.get::<HurtAt>(e).filter(|h| h.at != 0 || h.last != 0.0) else { return };
    let left = (h.at + 10).saturating_sub(now_of(w, e));
    c.insert("HurtTime".into(), Tag::Short(i16::try_from(left).unwrap_or(i16::MAX)));
}

fn read_hurt_time(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Short(v) = t else { return false };
    let left = u64::try_from(*v).unwrap_or(0);
    if left > 0 {
        let at = (now_of(w, e) + 10).saturating_sub(left);
        let last = w.get::<HurtAt>(e).map_or(0.0, |h| h.last);
        w.entity_mut(e).insert(HurtAt { at, last });
    }
    true
}

/// `HurtByTimestamp`: the tick of the last hurt by a mob (an int), the `at` of `LastHurtBy`.
fn write_hurt_by(w: &World, e: Entity, c: &mut Compound) {
    if let Some(l) = w.get::<LastHurtBy>(e) {
        c.insert("HurtByTimestamp".into(), Tag::Int(i32::try_from(l.at).unwrap_or(i32::MAX)));
    }
}

fn read_hurt_by(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    if let Some(mut l) = w.get_mut::<LastHurtBy>(e) {
        l.at = u64::try_from(*v).unwrap_or(0);
    }
    true
}

/// `anger_end_time`: the end of the anger countdown (a long), the `until` of `Anger` (own time, as the rest of the component).
fn write_anger_end(w: &World, e: Entity, c: &mut Compound) {
    if let Some(a) = w.get::<Anger>(e) {
        // A bee or a wolf holds -1 until an anger sets the end (synched data default).
        let unset = a.until == 0 && w.get::<crate::cow::SpeciesId>(e).is_some_and(|id| crate::cow::tags_of_id(*id) & tag::ANGER_UNSET != 0);
        c.insert("anger_end_time".into(), Tag::Long(if unset { -1 } else { a.until as i64 }));
    }
}

fn read_anger_end(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Long(v) = t else { return false };
    if let Some(mut a) = w.get_mut::<Anger>(e) {
        a.until = u64::try_from(*v).unwrap_or(0);
    }
    true
}

/// The effect names of the game, as the NBT `id` takes them.
fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Speed => "minecraft:speed",
        Kind::Slowness => "minecraft:slowness",
        Kind::Weakness => "minecraft:weakness",
        Kind::Poison => "minecraft:poison",
        Kind::Wither => crate::species::lists::WITHER_EFFECT,
        Kind::Regeneration => "minecraft:regeneration",
        Kind::FireResistance => "minecraft:fire_resistance",
        Kind::WaterBreathing => "minecraft:water_breathing",
        Kind::Invisibility => "minecraft:invisibility",
        Kind::Levitation => "minecraft:levitation",
        Kind::Hunger => "minecraft:hunger",
        Kind::MiningFatigue => "minecraft:mining_fatigue",
        Kind::Blindness => "minecraft:blindness",
        Kind::JumpBoost => "minecraft:jump_boost",
        Kind::Glowing => "minecraft:glowing",
        Kind::Weaving => "minecraft:weaving",
    }
}

fn kind_of(name: &str) -> Option<Kind> {
    [
        Kind::Speed,
        Kind::Slowness,
        Kind::Weakness,
        Kind::Poison,
        Kind::Wither,
        Kind::Regeneration,
        Kind::FireResistance,
        Kind::WaterBreathing,
        Kind::Invisibility,
        Kind::Levitation,
        Kind::Hunger,
        Kind::MiningFatigue,
        Kind::Blindness,
        Kind::JumpBoost,
        Kind::Glowing,
        Kind::Weaving,
    ]
    .into_iter()
    .find(|&k| kind_name(k) == name)
}

/// `active_effects`: a list of { id, amplifier (omitted at 0), duration (ticks left), ambient, show_particles (0 when hidden), show_icon }.
fn write_effects(w: &World, e: Entity, c: &mut Compound) {
    let Some(f) = w.get::<Effects>(e) else { return };
    if f.list.is_empty() {
        return;
    }
    let now = now_of(w, e);
    let list = f
        .list
        .iter()
        .map(|x| {
            let mut m: Vec<(&str, Tag)> = vec![("id", Tag::String(kind_name(x.kind).into()))];
            if x.amp != 0 {
                m.push(("amplifier", Tag::Byte(x.amp.clamp(0, 255) as u8 as i8)));
            }
            m.push(("duration", Tag::Int(i32::try_from(x.ends.saturating_sub(now)).unwrap_or(i32::MAX))));
            if x.flags & AMBIENT != 0 {
                m.push(("ambient", Tag::Byte(1)));
            }
            if x.flags & HIDDEN != 0 {
                m.push(("show_particles", Tag::Byte(0)));
            }
            m.push(("show_icon", Tag::Byte(1)));
            Tag::compound(m)
        })
        .collect();
    c.insert("active_effects".into(), Tag::List(list));
}

/// Sets the effects to the list: the ones the entity has are removed, then each is added with its ticks left (`add_flagged`). A refused
/// list changes nothing.
fn read_effects(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::List(items) = t else { return false };
    let mut list = Vec::with_capacity(items.len());
    for item in items {
        let Tag::Compound(c) = item else { return false };
        let get = |k: &str| c.get(&JavaString::from(k));
        let Some(Tag::String(id)) = get("id") else { return false };
        let Some(kind) = kind_of(&id.to_string_lossy()) else { return false };
        let amp = match get("amplifier") {
            None => 0,
            Some(Tag::Byte(a)) => i32::from(*a as u8),
            Some(_) => return false,
        };
        let Some(Tag::Int(left)) = get("duration") else { return false };
        let mut flags = 0;
        if matches!(get("ambient"), Some(Tag::Byte(1))) {
            flags |= AMBIENT;
        }
        if matches!(get("show_particles"), Some(Tag::Byte(0))) {
            flags |= HIDDEN;
        }
        list.push((kind, amp, *left, flags));
    }
    let old: Vec<Kind> = w.get::<Effects>(e).map_or_else(Vec::new, |f| f.list.iter().map(|x| x.kind).collect());
    for k in old {
        effects::remove(w, e, k);
    }
    for (kind, amp, left, flags) in list {
        if left > 0 {
            effects::add_flagged(w, e, kind, amp, left, flags);
        }
    }
    true
}
