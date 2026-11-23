//! Vanilla 26.3 entity NBT (`Entity.saveWithoutId` and the subclasses'
//! `addAdditionalSaveData`) for the entities Horizon simulates: mobs, item
//! entities, experience orbs, arrows and falling blocks. Encoding starts from
//! the compound the entity was loaded with, when there is one, so fields
//! Horizon does not model (variants, attributes, equipment, custom names)
//! survive a Horizon save.
use crate::{
    arrows::Pickup,
    entities::{ArmorStand, Cushion, ExperienceOrb, ItemEntity, ItemFrame, Painting},
    mobs::{MobType, Who},
    nbt_path::int_value,
    projectiles::{Projectile, ProjectileKind},
    save::{
        doubles, float, floats, item_from_nbt, item_to_nbt, number, put, read_doubles, text,
        uuid_from_tag, uuid_tag,
    },
};
use horizon_core::{geometry::Vec3, random::LegacyRandom};
use std::{collections::HashMap, sync::Arc};
mod boat_load;
pub use boat_load::boat_from_nbt;
mod projectile_fields;
use projectile_fields::{load_projectile_entity_fields, put_projectile_entity_fields};
mod ai_pause;
pub use ai_pause::{paused_in as ai_paused_saved, write_ai_paused};
mod refs;
mod saved_attributes;

use {
    horizon_core::nbt::{Compound, Tag},
    horizon_world::{
        chunk::BlockState,
        registry::{BlockRegistry, BlockStateId},
    },
};

/// Saved data carried by `ConversionType.convertCommon` outside the modelled
/// mob fields. The converted entity has its own identity and type-specific data;
/// only its command tags are inherited from the old entity's raw compound.
pub fn conversion_tags(raw: Option<&Tag>) -> Option<Tag> {
    let Tag::List(tags) = raw?.get("Tags")? else {
        return None;
    };
    (!tags.is_empty()).then(|| Tag::compound([("Tags", Tag::List(tags.clone()))]))
}

fn string(s: &str) -> Tag {
    Tag::String(s.into())
}

fn hidden_effect_nbt(effect: &crate::effects::EffectInstance) -> Tag {
    let mut fields = vec![("duration", Tag::Int(effect.duration))];
    if effect.amplifier != 0 {
        fields.push((
            "amplifier",
            Tag::Byte(effect.amplifier.clamp(0, 255) as u8 as i8),
        ));
    }
    if effect.ambient {
        fields.push(("ambient", Tag::Byte(1)));
    }
    if !effect.visible {
        fields.push(("show_particles", Tag::Byte(0)));
    }
    fields.push(("show_icon", Tag::Byte(i8::from(effect.show_icon))));
    if let Some(hidden) = &effect.hidden {
        fields.push(("hidden_effect", hidden_effect_nbt(hidden)));
    }
    Tag::compound(fields)
}

fn hidden_effect_from_nbt(effect: &'static str, tag: &Tag) -> crate::effects::EffectInstance {
    let mut instance = crate::effects::EffectInstance::new(
        effect,
        number(tag.get("duration")).unwrap_or(0) as i32,
        number(tag.get("amplifier")).unwrap_or(0) as i32,
    );
    instance.ambient = number(tag.get("ambient")).is_some_and(|v| v != 0);
    instance.visible = number(tag.get("show_particles")) != Some(0);
    instance.show_icon = number(tag.get("show_icon")).map_or(instance.visible, |v| v != 0);
    instance.hidden = tag
        .get("hidden_effect")
        .map(|hidden| Box::new(hidden_effect_from_nbt(effect, hidden)));
    instance
}
/// Entity type ID (`id`).
pub fn kind(tag: &Tag) -> Option<String> {
    myriade_data::identifier::normalize_id(&text(tag.get("id"))?)
}
/// The clamp `Entity.load` applies to every saved position: ±3.0000512E7 on x
/// and z, ±2.0E7 on y.
pub fn clamp_loaded_position(p: [f64; 3]) -> [f64; 3] {
    [
        p[0].clamp(-30_000_512.0, 30_000_512.0),
        p[1].clamp(-20_000_000.0, 20_000_000.0),
        p[2].clamp(-30_000_512.0, 30_000_512.0),
    ]
}
/// `Entity.load`'s `Pos`: the saved coordinates, clamped to the world's limits.
pub fn position(tag: &Tag) -> Option<Vec3> {
    read_doubles::<3>(tag.get("Pos")).map(|p| {
        let p = clamp_loaded_position(p);
        Vec3::new(p[0], p[1], p[2])
    })
}
pub fn uuid(tag: &Tag) -> Option<[i32; 4]> {
    uuid_from_tag(tag.get("UUID"))
}
pub fn motion(tag: &Tag) -> Vec3 {
    read_doubles::<3>(tag.get("Motion")).map_or(Vec3::ZERO, |m| Vec3::new(m[0], m[1], m[2]))
}
/// A saved Motion as `Entity.load` takes it: a component faster than 10 blocks
/// a tick loads as 0.
pub fn loaded_motion(tag: &Tag) -> Vec3 {
    let m = motion(tag);
    let clamp = |v: f64| if v.abs() > 10.0 { 0.0 } else { v };
    Vec3::new(clamp(m.x), clamp(m.y), clamp(m.z))
}
pub fn rotation(tag: &Tag) -> [f32; 2] {
    read_doubles::<2>(tag.get("Rotation")).map_or([0.0; 2], |r| [r[0] as f32, r[1] as f32])
}
/// `Entity.baseTick` for a burning entity Horizon keeps as its loaded compound:
/// one tick off a positive `Fire`, which an unset or spent `Fire` leaves alone.
pub fn count_down_fire(tag: &mut Tag) {
    let Tag::Compound(c) = tag else { return };
    if let Some(ticks) = number(c.get(&"Fire".into())).map(|v| v as i16)
        && ticks > 0
    {
        c.insert("Fire".into(), Tag::Short(ticks - 1));
    }
}
/// `Entity.baseTick` of a fire-immune entity (an `AreaEffectCloud`): a positive
/// `Fire` is cleared on its tick, while a loaded 0 or negative value stays.
pub fn clear_positive_fire(tag: &mut Tag) {
    let Tag::Compound(c) = tag else { return };
    if let Some(ticks) = number(c.get(&"Fire".into())).map(|v| v as i16)
        && ticks > 0
    {
        c.insert("Fire".into(), Tag::Short(0));
    }
}
/// A saved Rotation as `Entity.load` takes it (`setRot`, read off vanilla 26.3 by the parity rows): the yaw `% 360`
/// (so 400 loads as 40 but 270 stays 270 and -190 stays -190: Java's remainder keeps the sign) and the pitch
/// `% 360` clamped to [-90, 90]; a component that is not finite is discarded (`setYRot`/`setXRot`) and loads as
/// 0. What a save writes is read with [`rotation`].
pub fn loaded_rotation(tag: &Tag) -> [f32; 2] {
    let [yaw, pitch] = rotation(tag);
    [
        if yaw.is_finite() { yaw % 360.0 } else { 0.0 },
        if pitch.is_finite() {
            (pitch % 360.0).clamp(-90.0, 90.0)
        } else {
            0.0
        },
    ]
}
/// `TagValueInput.getBooleanOr`: a numeric tag's byteValue is nonzero, with a false default.
pub fn flag(tag: &Tag, key: &str) -> bool {
    flag_or(tag, key, false)
}
/// `getBooleanOr(key, default)`: any numeric tag's nonzero test, else `default`.
pub fn flag_or(tag: &Tag, key: &str, default: bool) -> bool {
    // NBT floats floor before int narrowing; the final byte keeps the low bits.
    tag.get(key)
        .and_then(int_value)
        .map_or(default, |value| value as i8 != 0)
}

/// `Entity.saveWithoutId`: write the movement flags and the flags retained
/// from `Entity.load` as booleans. Only OnGround and Invulnerable write false.
pub fn write_saved_flags(tag: &mut Tag, on_ground: bool, no_gravity: bool) {
    let invulnerable = flag(tag, "Invulnerable");
    let optional = [
        ("NoGravity", no_gravity),
        ("CustomNameVisible", flag(tag, "CustomNameVisible")),
        ("Silent", flag(tag, "Silent")),
        ("Glowing", flag(tag, "Glowing")),
        ("HasVisualFire", flag(tag, "HasVisualFire")),
    ];
    let Tag::Compound(map) = tag else {
        return;
    };
    map.insert("OnGround".into(), Tag::Byte(i8::from(on_ground)));
    map.insert("Invulnerable".into(), Tag::Byte(i8::from(invulnerable)));
    for (key, value) in optional {
        let key = key.into();
        if value {
            map.insert(key, Tag::Byte(1));
        } else {
            map.remove(&key);
        }
    }
}

/// `Entity.saveWithoutId` fields every entity has, over the loaded compound.
struct Base<'a> {
    id: &'a str,
    uuid: [i32; 4],
    position: Vec3,
    motion: Vec3,
    rotation: [f32; 2],
    on_ground: bool,
    fall_distance: f64,
}
impl Base<'_> {
    fn tag(&self, raw: Option<&Tag>) -> Tag {
        let mut tag = match raw {
            Some(t @ Tag::Compound(_)) => t.clone(),
            _ => Tag::Compound(Compound::new()),
        };
        for (key, value) in [
            ("id", string(self.id)),
            ("UUID", Tag::IntArray(self.uuid.to_vec())),
            (
                "Pos",
                doubles(&[self.position.x, self.position.y, self.position.z]),
            ),
            (
                "Motion",
                doubles(&[self.motion.x, self.motion.y, self.motion.z]),
            ),
            ("Rotation", floats(&self.rotation)),
            ("OnGround", Tag::Byte(i8::from(self.on_ground))),
            ("fall_distance", Tag::Double(self.fall_distance.to_bits())),
        ] {
            put(&mut tag, key, value);
        }
        if let Tag::Compound(map) = &mut tag {
            // Entity.saveWithoutId writes Tags only for a nonempty live set.
            if matches!(map.get(&"Tags".into()), Some(Tag::List(tags)) if tags.is_empty()) {
                map.remove(&"Tags".into());
            }
            for (key, value) in [
                // Entity.remainingFireTicks starts at 0 (-1 was the old default).
                ("Fire", Tag::Short(0)),
                ("Air", Tag::Short(300)),
                ("Invulnerable", Tag::Byte(0)),
                ("PortalCooldown", Tag::Int(0)),
            ] {
                map.entry(key.into()).or_insert(value);
            }
        }
        let no_gravity = flag(&tag, "NoGravity");
        write_saved_flags(&mut tag, self.on_ground, no_gravity);
        tag
    }
    /// Preserve Entity data without flags owned by Mob's save writer.
    /// Only the dedicated boat, item and armor-stand writers use this path.
    fn non_mob_tag(&self, raw: Option<&Tag>) -> Tag {
        let mut tag = self.tag(raw);
        if let Tag::Compound(map) = &mut tag {
            map.remove(&"NoAI".into());
            map.remove(&"PersistenceRequired".into());
        }
        tag
    }
}

/// `Entity.saveWithoutId`'s common fields for entities Horizon keeps as raw
/// compounds: each one is inserted only when the loaded compound has no value
/// for it, so explicit values survive.
pub fn fill_common_defaults(tag: &mut Tag) {
    let Tag::Compound(map) = tag else {
        return;
    };
    for (key, value) in [
        ("Motion", doubles(&[0.0, 0.0, 0.0])),
        ("OnGround", Tag::Byte(0)),
        ("fall_distance", Tag::Double(0.0f64.to_bits())),
        ("Air", Tag::Short(300)),
        ("Invulnerable", Tag::Byte(0)),
        ("PortalCooldown", Tag::Int(0)),
    ] {
        map.entry(key.into()).or_insert(value);
    }
}

/// The mobs that save `Age` and `AgeLocked`.
fn saves_age(s: crate::mobs::Species) -> bool {
    use crate::mobs::{Species, finalize::ageable, species::can_be_a_baby};
    s == Species::Tadpole || (ageable(s) && can_be_a_baby(s))
}
/// `LivingEntity` + `Mob` (+ `AgeableMob`, creeper and skeleton specifics).
/// The mobs whose class extends vanilla's `Animal` (and so save `InLove`):
/// not allays, villagers, golems, dolphins or fish.
fn is_animal(id: &str) -> bool {
    matches!(
        id.strip_prefix("minecraft:").unwrap_or(id),
        "pig"
            | "cow"
            | "mooshroom"
            | "sheep"
            | "chicken"
            | "rabbit"
            | "horse"
            | "donkey"
            | "mule"
            | "zombie_horse"
            | "skeleton_horse"
            | "llama"
            | "trader_llama"
            | "goat"
            | "panda"
            | "polar_bear"
            | "fox"
            | "wolf"
            | "cat"
            | "ocelot"
            | "parrot"
            | "bee"
            | "turtle"
            | "armadillo"
            | "frog"
            | "axolotl"
            | "hoglin"
            | "strider"
            | "camel"
            | "camel_husk"
            | "sniffer"
            | "happy_ghast"
            | "nautilus"
            | "zombie_nautilus"
    )
}
/// The single player's UUID (`offline_uuid("Player")`): the entity the
/// neutral mobs are angry at.
pub(crate) fn player_uuid() -> [i32; 4] {
    let u = horizon_protocol::crypto::offline_uuid("Player");
    [
        (u >> 96) as u32 as i32,
        (u >> 64) as u32 as i32,
        (u >> 32) as u32 as i32,
        u as u32 as i32,
    ]
}
/// The UUID a save writes for a reference to `who`. A mob is only an id here:
/// its UUID lives in the game layer (`EntityMemory.uuids`), which is what
/// `uuid_of` answers from. When it answers `None` (the mob is no longer in the
/// level) nothing is written, rather than a made-up UUID that would name nothing
/// when the save is read back. The reference comes back with [`who_by_uuid`].
///
/// The save's `uuid_of` is this for a field of a mob's own compound; the
/// game layer's `Game::save_who_uuid` is the same rule for a field written
/// outside one (a bullet's `Target`), answered from `EntityMemory::uuid_of`.
pub fn who_uuid(who: Who, uuid_of: &mut impl FnMut(u32) -> Option<[i32; 4]>) -> Option<[i32; 4]> {
    match who {
        Who::Player => Some(player_uuid()),
        Who::Mob(id) => uuid_of(id),
    }
}
/// Replace every reference to the player `old` in a saved entity with `new`. Older
/// Horizon worlds named their single player by the offline UUID (`Owner` on tamed
/// animals and horses, a projectile's `Owner` and an item's `Thrower`, a fox's
/// `Trusted`, `angry_at`, a leash's `UUID`); when such a world opens under the
/// account (`lan::playerdata::migrate_owner`), a chunk's entities are rewritten as
/// they load, and their next save keeps the account's UUID. A reference is the
/// player's four ints, wherever the entity keeps it; an entity's own `UUID` never
/// names the player.
pub fn migrate_player_uuid(tag: &mut Tag, old: [i32; 4], new: [i32; 4]) {
    if old == new {
        return;
    }
    match tag {
        Tag::IntArray(uuid) if uuid.as_slice() == old.as_slice() => uuid.copy_from_slice(&new),
        Tag::List(list) => {
            for tag in list {
                migrate_player_uuid(tag, old, new);
            }
        }
        Tag::Compound(map) => {
            for tag in map.values_mut() {
                migrate_player_uuid(tag, old, new);
            }
        }
        _ => {}
    }
}
/// `ResolvableProfile.CODEC`: the name, id and properties of a profile, then the
/// skin's texture ids and model (`PlayerSkin.Patch`: "texture", "cape", "elytra", "model").
fn profile_nbt(p: &horizon_protocol::entity_data::Profile) -> Tag {
    let mut entries: Vec<(&'static str, Tag)> = Vec::new();
    if let Some(name) = &p.name {
        entries.push(("name", string(name)));
    }
    if let Some(id) = p.uuid {
        entries.push((
            "id",
            Tag::IntArray(vec![
                (id >> 96) as i32,
                (id >> 64) as i32,
                (id >> 32) as i32,
                id as i32,
            ]),
        ));
    }
    if !p.properties.is_empty() {
        let properties = p
            .properties
            .iter()
            .map(|property| {
                let mut entries: Vec<(&'static str, Tag)> = vec![
                    ("name", string(&property.name)),
                    ("value", string(&property.value)),
                ];
                if let Some(signature) = &property.signature {
                    entries.push(("signature", string(signature)));
                }
                Tag::compound(entries)
            })
            .collect();
        entries.push(("properties", Tag::List(properties)));
    }
    for (key, texture) in [
        ("texture", &p.body),
        ("cape", &p.cape),
        ("elytra", &p.elytra),
    ] {
        if let Some(texture) = texture {
            entries.push((key, string(texture)));
        }
    }
    if let Some(slim) = p.slim {
        entries.push(("model", string(if slim { "slim" } else { "wide" })));
    }
    Tag::compound(entries)
}
/// `ResolvableProfile.CODEC` read back: the compound `profile_nbt` writes, or a
/// bare player name (`Codec.withAlternative(FULL_CODEC, PLAYER_NAME, ...)`).
fn profile_from_nbt(tag: &Tag) -> Option<horizon_protocol::entity_data::Profile> {
    if let Tag::String(name) = tag {
        return Some(horizon_protocol::entity_data::Profile {
            name: Some(name.to_string_lossy()),
            uuid: None,
            properties: Vec::new(),
            body: None,
            cape: None,
            elytra: None,
            slim: None,
        });
    }
    let property = |name: String, value: &Tag, signature: Option<String>| match value {
        Tag::String(v) => Some(horizon_protocol::packets::login::Property {
            name,
            value: v.to_string_lossy(),
            signature,
        }),
        _ => None,
    };
    let properties: Vec<horizon_protocol::packets::login::Property> = match tag.get("properties") {
        // `ExtraCodecs.PROPERTY_MAP`: a list of properties ...
        Some(Tag::List(list)) => list
            .iter()
            .filter_map(|p| {
                let name = match p.get("name")? {
                    Tag::String(s) => s.to_string_lossy(),
                    _ => return None,
                };
                let signature = match p.get("signature") {
                    Some(Tag::String(s)) => Some(s.to_string_lossy()),
                    _ => None,
                };
                property(name, p.get("value")?, signature)
            })
            .collect(),
        // ... or a map from a name to its values.
        Some(Tag::Compound(map)) => map
            .iter()
            .flat_map(|(name, values)| match values {
                Tag::List(values) => values
                    .iter()
                    .filter_map(|v| property(name.to_string_lossy(), v, None))
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            })
            .collect(),
        _ => Vec::new(),
    };
    let text = |key: &str| match tag.get(key) {
        Some(Tag::String(s)) => Some(s.to_string_lossy()),
        _ => None,
    };
    Some(horizon_protocol::entity_data::Profile {
        name: text("name"),
        uuid: uuid_from_tag(tag.get("id")).map(|a| {
            a.iter()
                .fold(0u128, |u, &x| (u << 32) | u128::from(x as u32))
        }),
        properties,
        body: text("texture"),
        cape: text("cape"),
        elytra: text("elytra"),
        slim: text("model").map(|model| model == "slim"),
    })
}
/// `BlockState.CODEC`: the block's name when the state is that block's
/// default, `{id, properties}` for any other state.
pub fn state_to_nbt(registry: &BlockRegistry, state: BlockStateId) -> Tag {
    match registry.state(state) {
        Some(s) if registry.default_state(&s.name) == Some(state) => string(&s.name),
        Some(s) => s.to_nbt(),
        None => string("minecraft:air"),
    }
}
/// `BlockState.CODEC` read back: a block name or `{id, properties}` (or the
/// legacy `{Name, Properties}`). A name the registry knows only as a block
/// reads as its default state, and one it does not know at all as `None`.
pub fn state_from_nbt(registry: &BlockRegistry, tag: &Tag) -> Option<BlockStateId> {
    let state = match tag.get("Name") {
        Some(Tag::String(name)) => BlockState {
            name: name.to_string_lossy(),
            properties: match tag.get("Properties") {
                Some(Tag::Compound(p)) => p
                    .iter()
                    .filter_map(|(k, v)| match v {
                        Tag::String(v) => Some((k.to_string_lossy(), v.to_string_lossy())),
                        _ => None,
                    })
                    .collect(),
                _ => Default::default(),
            },
        },
        _ => BlockState::from_nbt(tag).ok()?,
    };
    registry
        .id(&state)
        .or_else(|| registry.default_state(&state.name))
}
/// `BlockState.isAir`: air, cave air and void air.
fn is_air(registry: &BlockRegistry, state: BlockStateId) -> bool {
    registry.state(state).is_some_and(|s| {
        matches!(
            s.name.as_str(),
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
        )
    })
}
/// `CustomName`: a text component saved as a string, a compound or a list,
/// flattened as `Component.getString` does (`translate`, `extra`).
fn custom_name(tag: &Tag) -> Option<String> {
    match tag.get("CustomName")? {
        Tag::String(s) => Some(s.to_string_lossy()),
        t @ (Tag::Compound(_) | Tag::List(_)) => Some(name_text(t).plain()),
        _ => None,
    }
}
pub use myriade_data::component::name_text;
/// `PrimedTnt.readAdditionalSaveData`: `explosion_power`, 4 by default and
/// clamped to 0..128.
pub fn tnt_power(tag: &Tag) -> f32 {
    // `getFloatOr` accepts any numeric tag, not only a float.
    let power = match tag.get("explosion_power") {
        Some(Tag::Byte(v)) => f32::from(*v),
        Some(Tag::Short(v)) => f32::from(*v),
        Some(Tag::Int(v)) => *v as f32,
        Some(Tag::Long(v)) => *v as f32,
        Some(Tag::Float(bits)) => f32::from_bits(*bits),
        Some(Tag::Double(bits)) => f64::from_bits(*bits) as f32,
        _ => 4.0,
    };
    power.clamp(0.0, 128.0)
}

/// `PrimedTnt.addAdditionalSaveData`: its `fuse` and, when it is not the
/// default, its `explosion_power`, the encoded live block state, and its
/// `owner` (`EntityReference.store`). With no live owner, a loaded `owner`
/// stays as the raw compound has it.
#[allow(clippy::too_many_arguments)]
pub fn tnt_to_nbt(
    position: Vec3,
    motion: Vec3,
    on_ground: bool,
    fall_distance: f64,
    fuse: i32,
    power: f32,
    block_state: Tag,
    rotation: [f32; 2],
    uuid: [i32; 4],
    owner: Option<[i32; 4]>,
    raw: Option<&Tag>,
) -> Tag {
    let mut tag = Base {
        id: "minecraft:tnt",
        uuid,
        position,
        motion,
        rotation,
        on_ground,
        fall_distance,
    }
    .tag(raw);
    put(&mut tag, "fuse", Tag::Short(fuse as i16));
    if power != 4.0 {
        put(&mut tag, "explosion_power", Tag::Float(power.to_bits()));
    } else if let Tag::Compound(map) = &mut tag {
        map.remove(&"explosion_power".into());
    }
    put(&mut tag, "block_state", block_state);
    if let Some(owner) = owner {
        put(&mut tag, "owner", Tag::IntArray(owner.to_vec()));
    }
    tag
}
/// `PrimedTnt.readAdditionalSaveData`: `EntityReference.read(input, "owner")`.
pub fn tnt_owner(tag: &Tag) -> Option<[i32; 4]> {
    uuid_from_tag(tag.get("owner"))
}
/// `EvokerFangs.addAdditionalSaveData`: always save the live warmup counter.
/// Known Owner UUIDs use `EntityReference.store` encoding; unresolved
/// loaded Owner data remains in the raw compound.
pub fn evoker_fangs_to_nbt(
    position: Vec3,
    yaw: f32,
    warmup: i32,
    owner: Option<[i32; 4]>,
    uuid: [i32; 4],
    raw: Option<&Tag>,
    live_motion: Option<Vec3>,
) -> Tag {
    let mut tag = Base {
        id: "minecraft:evoker_fangs",
        uuid,
        position,
        motion: live_motion.unwrap_or_else(|| raw.map(motion).unwrap_or(Vec3::ZERO)),
        rotation: [yaw, raw.map(rotation).unwrap_or([0.0; 2])[1]],
        on_ground: raw.is_some_and(|t| flag(t, "OnGround")),
        fall_distance: raw.map_or(0.0, |t| ground_from_nbt(t).1),
    }
    .tag(raw);
    put(&mut tag, "Warmup", Tag::Int(warmup));
    if let Some(owner) = owner {
        put(&mut tag, "Owner", Tag::IntArray(owner.to_vec()));
    }
    tag
}

/// The `Entity` fields a saved non-mob entity carries (`Base::tag`), for a
/// caller that writes its own kind's fields around them.
pub fn entity_base_tag(
    id: &str,
    position: Vec3,
    motion: Vec3,
    rotation: [f32; 2],
    uuid: [i32; 4],
    raw: Option<&Tag>,
) -> Tag {
    Base {
        id,
        uuid,
        position,
        motion,
        rotation,
        on_ground: raw.is_some_and(|t| flag(t, "OnGround")),
        fall_distance: raw.map_or(0.0, |t| ground_from_nbt(t).1),
    }
    .tag(raw)
}

/// `OminousItemSpawner.addAdditionalSaveData`: its item (left out when
/// empty) and `spawn_item_after_ticks`. Its tick count is the entity's
/// `tickCount`, which isn't saved: a reloaded spawner counts from 0 again.
/// It never moves or turns: it saves the Motion, Rotation, OnGround and
/// fall_distance it was loaded with (`Entity.load`).
pub fn ominous_item_spawner_to_nbt(
    item: &crate::items::ItemStack,
    position: Vec3,
    spawn_item_after: i64,
    uuid: [i32; 4],
    raw: Option<&Tag>,
) -> Tag {
    let base = Base {
        id: "minecraft:ominous_item_spawner",
        uuid,
        position,
        motion: raw.map_or(Vec3::ZERO, loaded_motion),
        rotation: raw.map_or([0.0, 0.0], rotation),
        on_ground: raw.is_some_and(|t| flag(t, "OnGround")),
        fall_distance: float(raw.and_then(|t| t.get("fall_distance"))).unwrap_or(0.0),
    };
    let mut tag = base.tag(raw);
    // `ItemStack.CODEC` can't encode `ItemStack.EMPTY`: an empty item writes no key.
    if let Tag::Compound(map) = &mut tag {
        map.remove(&"item".into());
    }
    if &*item.item != "minecraft:air" {
        put(&mut tag, "item", item_to_nbt(item));
    }
    put(
        &mut tag,
        "spawn_item_after_ticks",
        Tag::Long(spawn_item_after),
    );
    tag
}
/// `OminousItemSpawner.readAdditionalSaveData`: the item, where it floats
/// and `spawn_item_after_ticks` (0 when absent). None without an item:
/// such a spawner would spawn nothing.
pub fn ominous_item_spawner_from_nbt(tag: &Tag) -> Option<(crate::items::ItemStack, Vec3, i64)> {
    // `readAdditionalSaveData`: `ItemStack.EMPTY` when it has none.
    let item = tag
        .get("item")
        .and_then(item_from_nbt)
        .unwrap_or_else(|| crate::items::ItemStack::new("minecraft:air", 1));
    let after = number(tag.get("spawn_item_after_ticks")).unwrap_or(0);
    Some((item, position(tag)?, after))
}

/// `ExperienceOrb`: Fire, Health, Age, Value, Count.
pub fn orb_to_nbt(o: &ExperienceOrb, uuid: [i32; 4]) -> Tag {
    let base = Base {
        id: "minecraft:experience_orb",
        uuid,
        position: o.position,
        motion: o.velocity,
        rotation: o.rotation,
        on_ground: o.on_ground,
        fall_distance: o.fall_distance,
    };
    let mut tag = base.tag(None);
    for (key, value) in [
        ("Fire", Tag::Short(o.fire_ticks as i16)),
        ("Health", Tag::Short(o.health as i16)),
        ("Air", Tag::Short(o.air as i16)),
        ("Age", Tag::Short(o.age.min(i16::MAX as u32) as i16)),
        ("Value", Tag::Short(o.value as i16)),
        ("Count", Tag::Int(o.count.min(i32::MAX as u32) as i32)),
    ] {
        put(&mut tag, key, value);
    }
    // Entity.saveWithoutId writes NoGravity only when set.
    if o.no_gravity {
        put(&mut tag, "NoGravity", Tag::Byte(1));
    }
    tag
}
/// One orb with its merged `Count`.
pub fn orbs_from_nbt(tag: &Tag) -> Vec<ExperienceOrb> {
    let Some(position) = position(tag) else {
        return Vec::new();
    };
    let count = number(tag.get("Count")).unwrap_or(1).max(1) as u32;
    let orb = ExperienceOrb {
        fire_ticks: number(tag.get("Fire")).unwrap_or(0) as i16 as i32,
        fall_distance: float(tag.get("fall_distance")).unwrap_or(0.0),
        rotation: rotation(tag),
        join: Default::default(),
        no_gravity: flag(tag, "NoGravity"),
        id: 0,
        position,
        previous: position,
        velocity: motion(tag),
        // `getShortOr("Value", 0)`, unclamped.
        value: tag.get("Value").and_then(int_value).unwrap_or(0) as i16 as i32,
        age: number(tag.get("Age")).unwrap_or(0).max(0) as u32,
        on_ground: flag(tag, "OnGround"),
        count,
        ticks: 0,
        health: number(tag.get("Health")).unwrap_or(5) as i32,
        air: number(tag.get("Air")).map_or(300, |air| air as i32), // getIntOr("Air", max)
        // ExperienceOrb's followingPlayer is transient: never saved, so a
        // loaded orb starts without one.
        following: None,
    };
    vec![orb]
}

/// `Projectile` + `AbstractArrow.addAdditionalSaveData` of a live arrow;
/// `in_block` is its `inBlockState`, `raw` the compound it was loaded with.
pub fn arrow_to_nbt(
    p: &Projectile,
    uuid: [i32; 4],
    owner: Option<[i32; 4]>,
    in_block: Option<Tag>,
    raw: Option<&Tag>,
) -> Option<Tag> {
    let a = p.arrow.as_deref()?;
    let ProjectileKind::Arrow {
        base_damage,
        critical,
        ..
    } = p.kind
    else {
        return None;
    };
    let base = Base {
        // A spectral arrow is its own entity type.
        id: if a.is_spectral_type() {
            "minecraft:spectral_arrow"
        } else {
            "minecraft:arrow"
        },
        uuid,
        position: p.position,
        motion: p.velocity,
        rotation: [p.yaw, p.pitch],
        on_ground: p.on_ground,
        fall_distance: a.fall_distance,
    };
    let mut tag = base.tag(raw);
    // `SpectralArrow.addAdditionalSaveData` always writes its integer duration.
    if base.id == "minecraft:spectral_arrow" {
        put(&mut tag, "Duration", Tag::Int(a.spectral_duration));
    }
    for (key, value) in [
        ("Fire", Tag::Short(a.fire_ticks as i16)),
        ("Air", Tag::Short(p.air_supply as i16)),
        ("life", Tag::Short(a.life as i16)),
        ("shake", Tag::Byte(a.shake as i8)),
        ("inGround", Tag::Byte(i8::from(a.in_ground))),
        ("pickup", Tag::Byte(a.pickup.byte())),
        ("damage", Tag::Double(base_damage.to_bits())),
        ("crit", Tag::Byte(i8::from(critical))),
        ("PierceLevel", Tag::Byte(a.pierce as i8)),
        ("HasBeenShot", Tag::Byte(i8::from(a.has_been_shot))),
        ("item", item_to_nbt(&a.pickup_item)),
    ] {
        put(&mut tag, key, value);
    }
    if let Tag::Compound(map) = &mut tag {
        let sound = a.saved_sound_event(map.get(&"SoundEvent".into()));
        map.insert("SoundEvent".into(), sound);
        // Projectile: LeftOwner only when set; Owner as the UUID.
        map.remove(&"LeftOwner".into());
        map.remove(&"inBlockState".into());
        map.remove(&"weapon".into());
        map.remove(&"can_break".into());
    }
    if let Some(weapon) = &a.fired_from_weapon {
        put(&mut tag, "weapon", item_to_nbt(weapon));
    }
    // `Projectile.addAdditionalSaveData`: the actual decoded predicate,
    // never a stale source compound or the pickup item's component patch.
    if let Some(predicate) = &a.can_break {
        put(&mut tag, "can_break", predicate.clone());
    }
    if p.left_owner {
        put(&mut tag, "LeftOwner", Tag::Byte(1));
    }
    if let Some(state) = in_block {
        put(&mut tag, "inBlockState", state);
    }
    // Entity.saveWithoutId writes NoGravity only when set.
    if p.no_gravity {
        put(&mut tag, "NoGravity", Tag::Byte(1));
    }
    if let Some(owner) = owner {
        put(&mut tag, "Owner", Tag::IntArray(owner.to_vec()));
    }
    Some(tag)
}
/// A saved arrow as a live one (`readAdditionalSaveData`), with its
/// `inBlockState` compound for the caller to resolve. `effect` is the tipped
/// arrow's lasting effect from its item.
pub fn arrow_from_nbt(
    tag: &Tag,
    player: Option<[i32; 4]>,
    effect: impl FnOnce(&crate::items::ItemStack) -> Option<(&'static str, i32)>,
) -> Option<(Projectile, Option<Tag>)> {
    let position = position(tag)?;
    let owner = uuid_from_tag(tag.get("Owner"));
    let from_player = player.is_some() && owner == player;
    // `getDefaultPickupItem`: a `SpectralArrow` is picked up as its own item.
    let spectral = kind(tag).as_deref() == Some("minecraft:spectral_arrow");
    let default_item = if spectral {
        "minecraft:spectral_arrow"
    } else {
        "minecraft:arrow"
    };
    let item = tag
        .get("item")
        .and_then(item_from_nbt)
        .unwrap_or_else(|| crate::items::ItemStack::new(default_item, 1));
    let kind = ProjectileKind::Arrow {
        base_damage: float(tag.get("damage")).unwrap_or(2.0),
        critical: flag(tag, "crit"),
        effect: effect(&item),
    };
    let mut p = Projectile::new(kind, position, from_player.then_some(Who::Player));
    p.velocity = motion(tag);
    [p.yaw, p.pitch] = rotation(tag);
    p.left_owner = flag(tag, "LeftOwner");
    p.owner_uuid = owner;
    p.no_gravity = flag(tag, "NoGravity");
    p.on_ground = flag(tag, "OnGround");
    projectile_fields::load_arrow_entity_fields(&mut p, tag);
    let a = p.arrow.as_mut()?;
    a.life = number(tag.get("life")).unwrap_or(0) as i16 as i32;
    a.shake = (number(tag.get("shake")).unwrap_or(0) as i8 as u8).into();
    a.in_ground = flag(tag, "inGround");
    a.pickup = Pickup::from_byte(number(tag.get("pickup")).unwrap_or(0));
    a.pierce = number(tag.get("PierceLevel")).unwrap_or(0) as u8;
    a.has_been_shot = flag(tag, "HasBeenShot");
    // `Projectile.readAdditionalSaveData`: one block predicate or a nonempty
    // list. Unsupported conjuncts remain saved, but cannot grant permission.
    a.can_break = tag
        .get("can_break")
        .filter(|predicate| match predicate {
            Tag::Compound(_) => true,
            Tag::List(list) => {
                !list.is_empty() && list.iter().all(|p| matches!(p, Tag::Compound(_)))
            }
            _ => false,
        })
        .cloned();
    a.spectral = spectral;
    // `SpectralArrow.readAdditionalSaveData`: absent Duration defaults to 200.
    if spectral {
        a.read_spectral_duration(tag);
    }
    a.pickup_item = item;
    a.load_weapon_and_sound(tag, item_from_nbt);
    p.stuck = a.in_ground;
    Some((p, tag.get("inBlockState").cloned()))
}

/// `LargeFireball`, `SmallFireball` and `WitherSkull`: `AbstractHurtingProjectile`'s
/// `acceleration_power`, `Projectile`'s owner and flags, `ExplosionPower`,
/// and a skull's `dangerous`.
pub fn fireball_to_nbt(
    p: &Projectile,
    uuid: [i32; 4],
    owner: Option<[i32; 4]>,
    raw: Option<&Tag>,
) -> Option<Tag> {
    let power = match p.kind {
        ProjectileKind::LargeFireball { power } => Some(power),
        ProjectileKind::SmallFireball
        | ProjectileKind::DragonFireball
        | ProjectileKind::WindCharge
        | ProjectileKind::BreezeWindCharge
        | ProjectileKind::WitherSkull { .. } => None,
        _ => return None,
    };
    let base = Base {
        id: p.kind.id(),
        uuid,
        position: p.position,
        motion: p.velocity,
        rotation: [p.yaw, p.pitch],
        on_ground: p.on_ground,
        fall_distance: p.fall_distance,
    };
    let mut tag = base.tag(raw);
    put(
        &mut tag,
        "acceleration_power",
        Tag::Double(p.acceleration.to_bits()),
    );
    put_projectile(&mut tag, p, owner);
    if projectile_fields::keeps_entity_fields(p) {
        put_projectile_entity_fields(&mut tag, p);
    }
    if let Some(power) = power {
        put(&mut tag, "ExplosionPower", Tag::Byte(power as i8));
    }
    // WitherSkull.addAdditionalSaveData: `dangerous`, false for a black skull.
    if let ProjectileKind::WitherSkull { dangerous } = p.kind {
        put(&mut tag, "dangerous", Tag::Byte(i8::from(dangerous)));
    }
    // Fireball.addAdditionalSaveData: the item `getItem` returns, a fire
    // charge unless `setItem` was given another (kept as a single one).
    if let Some(item) = &p.item {
        put(&mut tag, "Item", item_to_nbt(item));
    }
    Some(tag)
}

/// `Projectile.addAdditionalSaveData`: Owner, LeftOwner only when set, and
/// the explicit HasBeenShot flag.
fn put_projectile(tag: &mut Tag, p: &Projectile, owner: Option<[i32; 4]>) {
    if let Tag::Compound(map) = tag {
        map.remove(&"LeftOwner".into());
    }
    if p.left_owner {
        put(tag, "LeftOwner", Tag::Byte(1));
    }
    // Entity.saveWithoutId writes NoGravity only when set.
    if p.no_gravity {
        put(tag, "NoGravity", Tag::Byte(1));
    }
    put(tag, "HasBeenShot", Tag::Byte(i8::from(p.has_been_shot)));
    if let Some(owner) = owner {
        put(tag, "Owner", Tag::IntArray(owner.to_vec()));
    }
}

/// The thrown item projectiles Horizon saves: each entity type's id is also
/// the item it saves when none was loaded (`getDefaultItem`).
const THROWABLE_ITEMS: [&str; 6] = [
    "minecraft:snowball",
    "minecraft:egg",
    "minecraft:ender_pearl",
    "minecraft:splash_potion",
    "minecraft:lingering_potion",
    "minecraft:experience_bottle",
];
/// The item a throwable flies as: its stack (a potion's), the mod type's
/// item, or the item its entity type is named after.
fn throwable_item(kind: &ProjectileKind) -> Option<&str> {
    match kind {
        ProjectileKind::Thrown(t) => Some(t.item),
        ProjectileKind::Potion(stack, _) => Some(&stack.item),
        _ => THROWABLE_ITEMS.iter().find(|id| **id == kind.id()).copied(),
    }
}

/// The thrown kind an entity id names: vanilla's, or a content mod's type.
/// A potion flies as the item its entity type names, which is the item a
/// summoned one has when it names none (`getDefaultItem`).
pub fn throwable_kind(id: &str) -> Option<ProjectileKind> {
    if !THROWABLE_ITEMS.contains(&id) {
        return crate::projectiles::thrown_type(id).map(ProjectileKind::Thrown);
    }
    match id {
        "minecraft:snowball" => Some(ProjectileKind::Snowball),
        "minecraft:egg" => Some(ProjectileKind::Egg),
        "minecraft:ender_pearl" => Some(ProjectileKind::EnderPearl),
        // The entity type is the id; the `Item` read later may be another potion.
        "minecraft:experience_bottle" => Some(ProjectileKind::ExperienceBottle),
        _ => Some(ProjectileKind::Potion(
            Box::new(crate::items::ItemStack::new(id, 1)),
            id == "minecraft:lingering_potion",
        )),
    }
}

/// Whether its chunk saves it: the kinds `arrow_to_nbt`, `fireball_to_nbt`
/// and [`throwable_to_nbt`] write.
pub fn saved_with_chunk(p: &Projectile) -> bool {
    // No wildcard: a kind added to `ProjectileKind` must say here whether its
    // chunk saves it (`Entity.shouldBeSaved` is true for every projectile in
    // vanilla), and a `true` needs a writer in `entity_chunks`.
    match &p.kind {
        ProjectileKind::LargeFireball { .. }
        | ProjectileKind::SmallFireball
        | ProjectileKind::DragonFireball
        | ProjectileKind::WindCharge
        | ProjectileKind::BreezeWindCharge
        | ProjectileKind::WitherSkull { .. }
        | ProjectileKind::Snowball
        | ProjectileKind::Egg
        | ProjectileKind::EnderPearl
        | ProjectileKind::Potion(..)
        | ProjectileKind::ExperienceBottle
        | ProjectileKind::LlamaSpit
        | ProjectileKind::ShulkerBullet { .. }
        | ProjectileKind::Thrown(_) => true,
        ProjectileKind::Arrow { .. } => p.arrow.is_some(),
        // Not written yet: tridents save from their own list. Wither skulls and
        // bullets are written now.
        ProjectileKind::Trident => false,
    }
}

/// `ThrowableItemProjectile.addAdditionalSaveData`: `Projectile`'s keys and
/// the `Item` it flies as.
pub fn throwable_to_nbt(
    p: &Projectile,
    uuid: [i32; 4],
    owner: Option<[i32; 4]>,
    raw: Option<&Tag>,
) -> Option<Tag> {
    let item = throwable_item(&p.kind)?;
    let base = Base {
        id: p.kind.id(),
        uuid,
        position: p.position,
        motion: p.velocity,
        rotation: [p.yaw, p.pitch],
        on_ground: p.on_ground,
        fall_distance: p.fall_distance,
    };
    let mut tag = base.tag(raw);
    put_projectile(&mut tag, p, owner);
    put_projectile_entity_fields(&mut tag, p);
    // Entity.saveWithoutId writes NoGravity only when set.
    if p.no_gravity {
        put(&mut tag, "NoGravity", Tag::Byte(1));
    }
    // the loaded item stays (blue egg, potion effects) at `setItem`'s count 1.
    if let Tag::Compound(m) = &mut tag {
        let st = match (&p.kind, &p.item) {
            (ProjectileKind::Potion(s, _), _) => (**s).clone(),
            (_, Some(thrown)) => thrown.clone(),
            _ => crate::items::ItemStack::new(item, 1),
        };
        if let Tag::Compound(i) = m.entry("Item".into()).or_insert_with(|| item_to_nbt(&st)) {
            i.insert("count".into(), Tag::Int(1));
        }
    }
    Some(tag)
}

/// `ThrowableItemProjectile.readAdditionalSaveData` for the kinds
/// [`throwable_to_nbt`] writes; the owner is the player when it is theirs.
pub fn throwable_from_nbt(tag: &Tag, player: Option<[i32; 4]>) -> Option<Projectile> {
    let mut kind = throwable_kind(&kind(tag)?)?;
    if let ProjectileKind::Potion(stack, _) = &mut kind
        && let Some(item) = tag.get("Item").and_then(item_from_nbt)
    {
        **stack = item;
        stack.count = 1;
    }
    let owner = owner_uuid(tag);
    let from_player = player.is_some() && owner == player;
    let mut p = Projectile::new(kind, position(tag)?, from_player.then_some(Who::Player));
    p.velocity = motion(tag);
    [p.yaw, p.pitch] = rotation(tag);
    p.left_owner = flag(tag, "LeftOwner");
    p.has_been_shot = flag(tag, "HasBeenShot");
    p.owner_uuid = owner;
    p.no_gravity = flag(tag, "NoGravity");
    load_projectile_entity_fields(&mut p, tag);
    p.on_ground = flag(tag, "OnGround");
    if !matches!(p.kind, ProjectileKind::Potion(..))
        && let Some(item) = tag.get("Item").and_then(item_from_nbt)
    {
        // Kept on the projectile: the tag it was loaded from is found by its
        // position, so it is gone once the projectile has moved.
        p.set_thrown_item(&item);
    }
    Some(p)
}

/// `LlamaSpit` and `ShulkerBullet` as `/summon` or a chunk loads them, with
/// `ShulkerBullet.readAdditionalSaveData`'s `Dir`, `Steps` and per-axis target
/// deltas: `Projectile`'s `Owner`, `LeftOwner`, `NoGravity` and the motion.
/// `target` is the entity the caller resolved from `Target`; a bullet loaded
/// without one falls (`ShulkerBullet.tick`).
pub fn spit_or_bullet_from_nbt(
    tag: &Tag,
    player: Option<[i32; 4]>,
    target: Option<Who>,
) -> Option<Projectile> {
    let at = position(tag)?;
    let kind = match kind(tag)?.as_str() {
        "minecraft:llama_spit" => ProjectileKind::LlamaSpit,
        "minecraft:shulker_bullet" => ProjectileKind::ShulkerBullet {
            target,
            // `Dir`: `Direction.LEGACY_ID_CODEC`, the step the bullet is on,
            // absent while it has not chosen one. A byte read through
            // `Direction.from3DDataValue`, which wraps it: `abs(d % 6)`.
            dir: number(tag.get("Dir")).map(|d| ((d as i8) % 6).unsigned_abs()),
            // `getIntOr("Steps", 0)`: any int, a negative one included.
            steps: number(tag.get("Steps")).unwrap_or(0) as i32,
            delta: [
                float(tag.get("TXD")).unwrap_or(0.0),
                float(tag.get("TYD")).unwrap_or(0.0),
                float(tag.get("TZD")).unwrap_or(0.0),
            ],
            // `Entity.random` is not saved: a loaded bullet gets a stream of
            // its own from where it is, as `Projectile::new` does for an
            // arrow.
            seed: (at.x.to_bits() ^ at.z.to_bits().rotate_left(17)) | 1,
        },
        _ => return None,
    };
    let owner = owner_uuid(tag);
    let from_player = player.is_some() && owner == player;
    let mut p = Projectile::new(kind, at, from_player.then_some(Who::Player));
    p.velocity = motion(tag);
    [p.yaw, p.pitch] = rotation(tag);
    p.owner_uuid = owner;
    projectile_fields::load_projectile_flags_and_entity_fields(&mut p, tag);
    p.on_ground = flag(tag, "OnGround");
    Some(p)
}

/// The save of [`spit_or_bullet_from_nbt`]'s kinds: `Projectile`'s keys, and
/// for a bullet the `Target`, `Dir`, `Steps` and target deltas vanilla writes.
pub fn spit_or_bullet_to_nbt(
    p: &Projectile,
    uuid: [i32; 4],
    owner: Option<[i32; 4]>,
    target: Option<[i32; 4]>,
    raw: Option<&Tag>,
) -> Option<Tag> {
    if !matches!(
        p.kind,
        ProjectileKind::LlamaSpit | ProjectileKind::ShulkerBullet { .. }
    ) {
        return None;
    }
    let base = Base {
        id: p.kind.id(),
        uuid,
        position: p.position,
        motion: p.velocity,
        rotation: [p.yaw, p.pitch],
        on_ground: p.on_ground,
        fall_distance: p.fall_distance,
    };
    let mut tag = base.tag(raw);
    put_projectile(&mut tag, p, owner);
    put_projectile_entity_fields(&mut tag, p);
    // `ShulkerBullet.addAdditionalSaveData`: the target it homes on (written
    // only while it has one), the step it is on (omitted while it has not
    // chosen one), the steps left and the per-axis target deltas.
    if let ProjectileKind::ShulkerBullet {
        dir, steps, delta, ..
    } = &p.kind
    {
        if let Tag::Compound(map) = &mut tag {
            map.remove(&"Target".into());
            map.remove(&"Dir".into());
        }
        if let Some(target) = target {
            put(&mut tag, "Target", uuid_tag(target));
        }
        if let Some(dir) = dir {
            put(&mut tag, "Dir", Tag::Byte(*dir as i8));
        }
        put(&mut tag, "Steps", Tag::Int(*steps));
        for (key, v) in [("TXD", delta[0]), ("TYD", delta[1]), ("TZD", delta[2])] {
            put(&mut tag, key, Tag::Double(v.to_bits()));
        }
    }
    Some(tag)
}

/// `ShulkerBullet.readAdditionalSaveData`'s `Target`: the UUID of the entity
/// the bullet homes on, which vanilla keeps even while that entity is away.
pub fn bullet_target(tag: &Tag) -> Option<[i32; 4]> {
    uuid_from_tag(tag.get("Target"))
}

/// `Projectile`'s `Owner`.
pub fn owner_uuid(tag: &Tag) -> Option<[i32; 4]> {
    uuid_from_tag(tag.get("Owner"))
}

/// `Fireball.readAdditionalSaveData` (and, around it, `Projectile`'s Owner and
/// LeftOwner and `AbstractHurtingProjectile`'s acceleration power): a fireball
/// also reads the item it is drawn as through `setItem`, whose default is the
/// fire charge; a wind charge is no `Fireball` and has none.
pub fn fireball_from_nbt(tag: &Tag, player: Option<[i32; 4]>) -> Option<Projectile> {
    let kind = match kind(tag)?.as_str() {
        "minecraft:fireball" => ProjectileKind::LargeFireball {
            power: i32::from(number(tag.get("ExplosionPower")).unwrap_or(1) as i8),
        },
        "minecraft:small_fireball" => ProjectileKind::SmallFireball,
        "minecraft:dragon_fireball" => ProjectileKind::DragonFireball,
        "minecraft:wind_charge" => ProjectileKind::WindCharge,
        "minecraft:breeze_wind_charge" => ProjectileKind::BreezeWindCharge,
        // WitherSkull.readAdditionalSaveData: an absent `dangerous` is a black skull.
        "minecraft:wither_skull" => ProjectileKind::WitherSkull {
            dangerous: flag(tag, "dangerous"),
        },
        _ => return None,
    };
    let owner = uuid_from_tag(tag.get("Owner"));
    let from_player = player.is_some() && owner == player;
    let mut p = Projectile::new(kind, position(tag)?, from_player.then_some(Who::Player));
    p.velocity = motion(tag);
    [p.yaw, p.pitch] = rotation(tag);
    p.left_owner = flag(tag, "LeftOwner");
    p.has_been_shot = flag(tag, "HasBeenShot");
    p.owner_uuid = owner;
    p.no_gravity = flag(tag, "NoGravity");
    if projectile_fields::keeps_entity_fields(&p) {
        load_projectile_entity_fields(&mut p, tag);
    }
    p.on_ground = flag(tag, "OnGround");
    p.fall_distance = float(tag.get("fall_distance")).unwrap_or(0.0);
    p.acceleration = float(tag.get("acceleration_power")).unwrap_or(0.1);
    if let Some(item) = tag.get("Item").and_then(item_from_nbt) {
        p.set_item(item);
    }
    Some(p)
}

/// `FireworkRocketEntity.readAdditionalSaveData`: Life, LifeTime (0 when
/// absent: a bare `/summon` explodes at once), FireworksItem and
/// ShotAtAngle, with `Projectile`'s Owner and LeftOwner.
pub struct RocketNbt {
    pub position: Vec3,
    pub motion: Vec3,
    pub life: i32,
    pub lifetime: i32,
    pub item: Option<crate::items::ItemStack>,
    pub angled: bool,
    pub owner: Option<Who>,
    /// The saved `Owner`, whoever it names.
    pub owner_uuid: Option<[i32; 4]>,
    pub left_owner: bool,
    pub on_ground: bool,
    /// `Entity.load`'s Rotation.
    pub rotation: [f32; 2],
    /// `Projectile.hasBeenShot`: false until its first tick posts `projectile_shoot`.
    pub has_been_shot: bool,
    /// `Entity.isSilent`: no launch sound.
    pub silent: bool,
    /// `Entity.load`'s fall_distance.
    pub fall_distance: f64,
}
pub fn firework_rocket_from_nbt(tag: &Tag, player: Option<[i32; 4]>) -> Option<RocketNbt> {
    let owner = uuid_from_tag(tag.get("Owner"));
    Some(RocketNbt {
        position: position(tag)?,
        motion: motion(tag),
        life: number(tag.get("Life")).unwrap_or(0) as i32,
        lifetime: number(tag.get("LifeTime")).unwrap_or(0) as i32,
        item: tag.get("FireworksItem").and_then(item_from_nbt),
        angled: flag(tag, "ShotAtAngle"),
        owner: (player.is_some() && owner == player).then_some(Who::Player),
        owner_uuid: owner,
        left_owner: flag(tag, "LeftOwner"),
        on_ground: flag(tag, "OnGround"),
        rotation: rotation(tag),
        has_been_shot: flag(tag, "HasBeenShot"),
        silent: flag(tag, "Silent"),
        fall_distance: float(tag.get("fall_distance")).unwrap_or(0.0),
    })
}

/// What `FireworkRocketEntity.addAdditionalSaveData` writes, over `Projectile`'s
/// Owner and LeftOwner.
pub struct RocketSave<'a> {
    pub rotation: [f32; 2],
    pub position: Vec3,
    pub motion: Vec3,
    pub life: i32,
    pub lifetime: i32,
    pub item: &'a crate::items::ItemStack,
    pub angled: bool,
    pub owner: Option<[i32; 4]>,
    pub left_owner: bool,
    /// `Entity.onGround`, from the rocket's last `Entity.move`.
    pub on_ground: bool,
    pub has_been_shot: bool,
    /// `Entity.fallDistance`.
    pub fall_distance: f64,
}
pub fn firework_rocket_to_nbt(r: &RocketSave, uuid: [i32; 4], raw: Option<&Tag>) -> Tag {
    let mut tag = Base {
        id: "minecraft:firework_rocket",
        uuid,
        position: r.position,
        motion: r.motion,
        rotation: r.rotation,
        on_ground: r.on_ground,
        fall_distance: r.fall_distance,
    }
    .tag(raw);
    for (key, value) in [
        ("Life", Tag::Int(r.life)),
        ("LifeTime", Tag::Int(r.lifetime)),
        ("FireworksItem", item_to_nbt(r.item)),
        ("ShotAtAngle", Tag::Byte(i8::from(r.angled))),
        ("HasBeenShot", Tag::Byte(i8::from(r.has_been_shot))),
    ] {
        put(&mut tag, key, value);
    }
    if r.left_owner {
        put(&mut tag, "LeftOwner", Tag::Byte(1));
    }
    if let Some(owner) = r.owner {
        put(&mut tag, "Owner", Tag::IntArray(owner.to_vec()));
    }
    tag
}

mod trident;
pub use trident::{TRIDENT_HIT_GROUND, TridentNbt, TridentSave, trident_from_nbt, trident_to_nbt};
mod hanging;
pub use hanging::*;

/// `FallingBlockEntity`: BlockState, Time, DropItem, HurtEntities.
#[allow(clippy::too_many_arguments)]
pub fn falling_block_to_nbt(
    state: Tag,
    position: Vec3,
    motion: Vec3,
    time: i32,
    drop_item: bool,
    damage: crate::falling_block::Damage,
    fall_distance: f64,
    on_ground: bool,
    rotation: [f32; 2],
    uuid: [i32; 4],
) -> Tag {
    let base = Base {
        id: "minecraft:falling_block",
        uuid,
        position,
        motion,
        rotation,
        on_ground,
        fall_distance,
    };
    let mut tag = base.tag(None);
    for (key, value) in [
        ("BlockState", state),
        ("Time", Tag::Int(time)),
        ("DropItem", Tag::Byte(i8::from(drop_item))),
    ] {
        put(&mut tag, key, value);
    }
    damage.save(&mut tag);
    tag
}
/// (block state tag, position, motion, time, fall distance)
pub fn falling_block_from_nbt(tag: &Tag) -> Option<(Tag, Vec3, Vec3, i32, f64)> {
    Some((
        tag.get("BlockState")
            .cloned()
            .unwrap_or_else(|| string("minecraft:sand")),
        position(tag)?,
        motion(tag),
        number(tag.get("Time")).unwrap_or(0) as i32,
        float(tag.get("fall_distance")).unwrap_or(0.0),
    ))
}

/// FallingBlockEntity.readAdditionalSaveData uses the modern state codec.
pub fn falling_state_from_nbt(registry: &BlockRegistry, tag: &Tag) -> Option<BlockStateId> {
    BlockState::from_nbt(tag)
        .ok()
        .and_then(|mut state| {
            state.name = horizon_core::identifier::Identifier::parse(&state.name)
                .ok()?
                .to_string();
            registry
                .id(&state)
                .or_else(|| registry.default_state(&state.name))
        })
        .or_else(|| registry.default_state("minecraft:sand"))
}

/// `NoGravity` and `DropItem` (true when absent) of a saved falling block.
pub fn falling_block_flags(tag: &Tag) -> (bool, bool) {
    (flag(tag, "NoGravity"), flag_or(tag, "DropItem", true))
}

/// `Cushion.addAdditionalSaveData`: the block it sits on and its colour.
pub fn cushion_to_nbt(c: &Cushion, uuid: [i32; 4], raw: Option<&Tag>) -> Tag {
    // A block-attached cushion does not update these: they stay as loaded.
    let (on_ground, fall_distance) = raw.map_or((false, 0.0), ground_from_nbt);
    let base = Base {
        id: "minecraft:cushion",
        uuid,
        position: c.position,
        motion: Vec3::ZERO,
        // The cushion never turns: its saved pitch is the one it was loaded with.
        rotation: [c.yaw, raw.map_or(0.0, |t| loaded_rotation(t)[1])],
        on_ground,
        fall_distance,
    };
    let mut tag = base.tag(raw);
    put(&mut tag, "block_pos", Tag::IntArray(c.pos.to_vec()));
    put(&mut tag, "color", string(c.color_name()));
    if let Some(name) = &c.custom_name {
        put(&mut tag, "CustomName", name.clone());
    }
    tag
}
/// The position, block position, yaw and colour of a saved cushion.
pub fn cushion_from_nbt(tag: &Tag, id: u32) -> Option<Cushion> {
    let position = position(tag)?;
    let pos = match tag.get("block_pos") {
        Some(Tag::IntArray(v)) if v.len() == 3 => [v[0], v[1], v[2]],
        _ => [
            position.x.floor() as i32,
            position.y.floor() as i32,
            position.z.floor() as i32,
        ],
    };
    // `Entity.load`: the yaw `% 360` (a cushion keeps no pitch of its own: the saved one is read as loaded).
    let yaw = loaded_rotation(tag)[0];
    let color =
        crate::entities::dye_index(&text(tag.get("color")).unwrap_or_else(|| "white".into()));
    Some(Cushion {
        join: Default::default(),
        id,
        position,
        pos,
        yaw,
        color,
        check_interval: 0,
        custom_name: tag.get("CustomName").cloned(),
    })
}

#[cfg(test)]
mod cushion_tests {
    use super::*;

    #[test]
    fn flags_take_any_number_and_fall_back_when_absent() {
        let tag = Tag::compound([
            ("byte", Tag::Byte(2)),
            ("int", Tag::Int(1)),
            ("wrapped", Tag::Int(256)),
            ("short", Tag::Short(0)),
            ("text", string("1")),
        ]);
        assert!(flag(&tag, "byte"));
        assert!(flag(&tag, "int"));
        assert!(!flag(&tag, "wrapped"));
        assert!(!flag_or(&tag, "short", true));
        assert!(flag_or(&tag, "text", true));
        assert!(flag_or(&tag, "absent", true));
        assert!(!flag(&tag, "absent"));
    }

    #[test]
    fn cushion_save_preserves_color_anchor_rotation_and_unknown_data() {
        let cushion = Cushion {
            attachment_lifetime: Default::default(),
            join: Default::default(),
            id: 7,
            position: Vec3::new(-1.5, 64.5, 3.5),
            pos: [-2, 64, 3],
            yaw: 270.0,
            color: 14,
            check_interval: 55,
            custom_name: None,
        };
        let raw = Tag::compound([("CustomName", string("Seat"))]);
        let tag = cushion_to_nbt(&cushion, [1, 2, 3, 4], Some(&raw));
        assert_eq!(text(tag.get("CustomName")).as_deref(), Some("Seat"));
        let loaded = cushion_from_nbt(&tag, 8).unwrap();
        assert_eq!(loaded.id, 8);
        assert_eq!(loaded.position, cushion.position);
        assert_eq!(loaded.pos, cushion.pos);
        assert_eq!(loaded.yaw, cushion.yaw);
        assert_eq!(loaded.color_name(), "red");
        assert_eq!(loaded.check_interval, 0);
        assert_eq!(loaded.bounds(), cushion.bounds());
    }

    #[test]
    fn a_summoned_cushion_keeps_its_yaw_and_pitch_through_a_save() {
        // summon ... {Rotation:[75f,20f]}: the saved yaw 75 and pitch 20 come back.
        let raw = Tag::compound([
            ("Pos", Tag::List(vec![Tag::Double(0.5f64.to_bits()); 3])),
            (
                "Rotation",
                Tag::List(vec![
                    Tag::Float(75f32.to_bits()),
                    Tag::Float(20f32.to_bits()),
                ]),
            ),
        ]);
        let cushion = cushion_from_nbt(&raw, 1).unwrap();
        assert_eq!(cushion.yaw, 75.0);
        let saved = cushion_to_nbt(&cushion, [1, 2, 3, 4], Some(&raw));
        assert_eq!(rotation(&saved), [75.0, 20.0]);
    }
}
/// `ItemFrame.setDirection`: a frame on a wall turns to face away from it
/// (`yRot` = the 2D data value times 90, `xRot` 0); one on a floor or a
/// ceiling looks straight down or up (`xRot` = -90 times the axis step).
/// `facing` is the 3D data value (down, up, north, south, west, east).
fn frame_rotation(facing: u8) -> [f32; 2] {
    match facing {
        0 => [0.0, 90.0],
        1 => [0.0, -90.0],
        2 => [180.0, 0.0],
        3 => [0.0, 0.0],
        4 => [90.0, 0.0],
        _ => [270.0, 0.0],
    }
}

/// ItemFrame.addAdditionalSaveData.
pub fn item_frame_to_nbt(f: &ItemFrame, uuid: [i32; 4], raw: Option<&Tag>) -> Tag {
    let (lo, hi) = f.bounds();
    let base = Base {
        id: f.kind(),
        uuid,
        position: Vec3::new(
            (lo[0] + hi[0]) / 2.0,
            (lo[1] + hi[1]) / 2.0,
            (lo[2] + hi[2]) / 2.0,
        ),
        motion: raw.map_or(Vec3::ZERO, loaded_motion),
        rotation: frame_rotation(f.facing),
        on_ground: raw.is_some_and(|raw| flag(raw, "OnGround")),
        fall_distance: raw.map_or(0.0, |raw| ground_from_nbt(raw).1),
    };
    let mut tag = base.tag(raw);
    // `Entity.saveWithoutId`: `CustomName` before the frame's own fields.
    if let Some(name) = &f.custom_name {
        put(&mut tag, "CustomName", name.clone());
    } else if let Tag::Compound(map) = &mut tag {
        map.remove(&"CustomName".into());
    }
    put(&mut tag, "Facing", Tag::Byte(f.facing as i8));
    put(&mut tag, "block_pos", Tag::IntArray(f.pos.to_vec()));
    put(&mut tag, "ItemRotation", Tag::Byte(f.rotation));
    put(
        &mut tag,
        "ItemDropChance",
        Tag::Float(f.drop_chance.to_bits()),
    );
    put(&mut tag, "Invisible", Tag::Byte(i8::from(f.invisible)));
    put(&mut tag, "Fixed", Tag::Byte(i8::from(f.fixed)));
    if let Tag::Compound(map) = &mut tag {
        map.remove(&horizon_core::nbt::JavaString::from("Item"));
    }
    if let Some(item) = &f.item {
        put(&mut tag, "Item", item_to_nbt(item));
    }
    tag
}
/// `ItemFrame.readAdditionalSaveData`: an absent Facing defaults to Direction.DOWN.
pub fn item_frame_from_nbt(tag: &Tag, id: u32) -> Option<ItemFrame> {
    let pos = hanging::attached_block_pos(tag)?;
    Some(ItemFrame {
        join: Default::default(),
        id,
        pos,
        facing: hanging::wrap_data_value(number(tag.get("Facing")).unwrap_or(0), 6),
        glow: kind(tag).as_deref() == Some("minecraft:glow_item_frame"),
        item: tag.get("Item").and_then(item_from_nbt),
        // `TagValueInput.getByteOr` narrows before `ItemFrame.setRotation`'s remainder.
        rotation: (tag.get("ItemRotation").and_then(int_value).unwrap_or(0) as i8) % 8,
        drop_chance: float(tag.get("ItemDropChance")).map_or(1.0, |v| v as f32),
        fixed: flag(tag, "Fixed"),
        invisible: flag(tag, "Invisible"),
        check_interval: 0,
        custom_name: tag.get("CustomName").cloned(),
        random: horizon_core::random::EntityRandom::own(crate::random_source::entity()),
    })
}
/// ArmorStand.addAdditionalSaveData with LivingEntity's `equipment`.
pub fn armor_stand_to_nbt(a: &ArmorStand, uuid: [i32; 4], raw: Option<&Tag>) -> Tag {
    let base = Base {
        id: "minecraft:armor_stand",
        uuid,
        position: a.position,
        motion: a.velocity,
        rotation: [a.yaw, a.pitch],
        on_ground: a.on_ground,
        fall_distance: a.enchantment_fall_distance,
    };
    let mut tag = base.non_mob_tag(raw);
    // `equipment` is saved only when something is worn, `Marker` only when set.
    if let Tag::Compound(map) = &mut tag {
        a.equipment.store(map);
    }
    put(&mut tag, "DisabledSlots", Tag::Int(a.disabled_slots));
    for (key, value) in [
        ("Small", a.small),
        ("ShowArms", a.show_arms),
        ("NoBasePlate", a.no_base_plate),
        ("Invisible", a.invisible),
    ] {
        put(&mut tag, key, Tag::Byte(i8::from(value)));
    }
    if a.marker {
        put(&mut tag, "Marker", Tag::Byte(1));
    } else if let Tag::Compound(map) = &mut tag {
        map.remove(&"Marker".into());
    }
    // Entity.saveWithoutId writes NoGravity only when set.
    if a.no_gravity {
        put(&mut tag, "NoGravity", Tag::Byte(1));
    } else if let Tag::Compound(map) = &mut tag {
        map.remove(&"NoGravity".into());
    }
    if a.name_visible {
        put(&mut tag, "CustomNameVisible", Tag::Byte(1));
    } else if let Tag::Compound(map) = &mut tag {
        map.remove(&"CustomNameVisible".into());
    }
    // A name from the save keeps its own shape unless it changed.
    if let Some(name) = &a.custom_name
        && !tag.get("CustomName").is_some_and(|t| name_text(t) == *name)
    {
        put(
            &mut tag,
            "CustomName",
            crate::scoreboard::json_to_tag(&name.component()),
        );
    }
    put(&mut tag, "Health", Tag::Float(a.health.to_bits()));
    put(&mut tag, "Fire", Tag::Short(a.fire_ticks as i16));
    // ArmorStand.addAdditionalSaveData: the `Pose` parts that are not default.
    let pose: Vec<(&str, Tag)> = ArmorStand::POSE_KEYS
        .into_iter()
        .zip(a.pose)
        .zip(ArmorStand::DEFAULT_POSE)
        .filter(|((_, rotation), default)| rotation != default)
        .map(|((key, rotation), _)| (key, floats(&rotation)))
        .collect();
    put(&mut tag, "Pose", Tag::compound(pose));
    // LivingEntity.addAdditionalSaveData and ArmorStand's own fields.
    if let Tag::Compound(map) = &mut tag {
        for (key, value) in [
            ("HurtTime", Tag::Short(0)),
            ("DeathTime", Tag::Short(0)),
            ("AbsorptionAmount", Tag::Float(0)),
            ("FallFlying", Tag::Byte(0)),
            ("current_impulse_context_reset_grace_time", Tag::Int(0)),
            ("attributes", Tag::List(Vec::new())),
            (
                "Brain",
                Tag::compound([("memories", Tag::Compound(Compound::new()))]),
            ),
            ("DisabledSlots", Tag::Int(0)),
            ("Pose", Tag::Compound(Compound::new())),
        ] {
            map.entry(key.into()).or_insert(value);
        }
        // `removeFrost`'s first-tick movement_speed, over a saved list without it.
        if a.speed_instance
            && let Some(Tag::List(list)) = map.get_mut(&"attributes".into())
            && !list
                .iter()
                .any(|t| text(t.get("id")).as_deref() == Some(SPEED_ATTRIBUTE))
        {
            list.push(Tag::compound([
                ("id", string(SPEED_ATTRIBUTE)),
                ("base", Tag::Double(0.7f64.to_bits())),
            ]));
        }
        put_live_attributes(map, &a.live);
    }
    tag
}
/// The live map's instances and the loaded attributes nothing else models,
/// over the saved `attributes` list (`AttributeMap.pack`).
pub fn put_live_attributes(map: &mut Compound, live: &crate::living_attributes::LivingAttributes) {
    let mut list = match map.get(&"attributes".into()) {
        Some(Tag::List(l)) => l.clone(),
        _ => Vec::new(),
    };
    let before = list.clone();
    live.save_into(&mut list);
    if list != before {
        map.insert("attributes".into(), Tag::List(list));
    }
}
/// The attribute every living entity's first tick makes (`removeFrost`).
const SPEED_ATTRIBUTE: &str = "minecraft:movement_speed";
/// Vanilla 26.3 `Entity.setXRot`: finite pitch is clamped after modulo 360,
/// non-finite pitch resets to 0.0.
fn normalize_pitch(pitch: f32) -> f32 {
    if pitch.is_finite() {
        (pitch % 360.0).clamp(-90.0, 90.0)
    } else {
        0.0
    }
}

pub fn armor_stand_from_nbt(tag: &Tag, id: u32) -> Option<ArmorStand> {
    let rot = rotation(tag);
    let mut a = ArmorStand::new(id, position(tag)?, rot[0]);
    a.pitch = normalize_pitch(rot[1]);
    a.enchantment_fall_distance = float(tag.get("fall_distance")).unwrap_or(0.0);
    // AttributeMap.load makes an instance for each saved attribute.
    a.speed_instance = matches!(tag.get("attributes"), Some(Tag::List(list))
        if list.iter().any(|attr| text(attr.get("id")).as_deref() == Some(SPEED_ATTRIBUTE)));
    a.velocity = loaded_motion(tag);
    a.health = float(tag.get("Health")).unwrap_or(20.0) as f32;
    a.fire_ticks = i32::from(number(tag.get("Fire")).unwrap_or(0) as i16);
    a.custom_name = tag.get("CustomName").map(name_text);
    a.name_visible = flag(tag, "CustomNameVisible");
    if let Some(eq) = tag.get("equipment") {
        a.equipment = horizon_items::equipment::Equipment::read(eq);
    }
    a.live.load(tag.get("attributes"), |_| true);
    a.small = flag(tag, "Small");
    a.show_arms = flag(tag, "ShowArms");
    a.disabled_slots = number(tag.get("DisabledSlots")).unwrap_or(0) as i32;
    a.no_base_plate = flag(tag, "NoBasePlate");
    a.invisible = flag(tag, "Invisible");
    a.marker = flag(tag, "Marker");
    a.no_gravity = flag(tag, "NoGravity");
    a.on_ground = flag(tag, "OnGround");
    // ArmorStand.readAdditionalSaveData: each part of `Pose` that is a list of
    // three numbers, the others staying at their defaults.
    if let Some(pose) = tag.get("Pose") {
        for (rotation, key) in a.pose.iter_mut().zip(ArmorStand::POSE_KEYS) {
            if let Some(Tag::List(values)) = pose.get(key)
                && values.len() == 3
            {
                for (r, v) in rotation.iter_mut().zip(values) {
                    *r = float(Some(v)).unwrap_or(0.0) as f32;
                }
            }
        }
    }
    Some(a)
}
/// `AbstractBoat.addAdditionalSaveData` / `ContainerEntity.addChestVehicleSaveData`:
/// only chest boats own container fields; a pending table replaces `Items`.
/// Ordinary slots use `ContainerHelper.saveAllItems` through the existing item encoder.
/// `ground` is the live `OnGround` and `fall_distance` (`Entity.saveWithoutId`).
#[allow(clippy::too_many_arguments)]
pub fn boat_to_nbt(
    id: &str,
    position: Vec3,
    motion: Vec3,
    rotation: [f32; 2],
    ground: (bool, f64),
    uuid: [i32; 4],
    raw: Option<&Tag>,
    items: Option<&[Option<crate::items::ItemStack>]>,
) -> Tag {
    let mut tag = Base {
        id,
        uuid,
        position,
        motion,
        rotation,
        on_ground: ground.0,
        fall_distance: ground.1,
    }
    .non_mob_tag(raw);
    if let Tag::Compound(map) = &mut tag {
        map.remove(&"Items".into());
        if let Some(items) = items {
            let table = crate::save::text(map.get(&"LootTable".into()))
                .and_then(|id| myriade_data::identifier::normalize_id(&id));
            if let Some(table) = table {
                map.insert("LootTable".into(), Tag::String(table.as_str().into()));
            } else {
                map.remove(&"LootTable".into());
                map.remove(&"LootTableSeed".into());
                map.insert("Items".into(), crate::save::encode_items(items));
            }
        } else {
            map.remove(&"LootTable".into());
            map.remove(&"LootTableSeed".into());
        }
    }
    tag
}
/// A vehicle's base entity tag (minecarts, the dragon and end crystals), with
/// the live `OnGround` and `fall_distance` in `ground`.
pub fn vehicle_to_nbt(
    id: &str,
    position: Vec3,
    motion: Vec3,
    rotation: [f32; 2],
    ground: (bool, f64),
    uuid: [i32; 4],
    raw: Option<&Tag>,
) -> Tag {
    Base {
        id,
        uuid,
        position,
        motion,
        rotation,
        on_ground: ground.0,
        fall_distance: ground.1,
    }
    .tag(raw)
}
/// `Entity.load`'s `OnGround` (`getBooleanOr`: `byteValue() != 0`) and
/// `fall_distance` (`getDoubleOr`: any number's `doubleValue`), defaulting to
/// false and 0.
pub fn ground_from_nbt(tag: &Tag) -> (bool, f64) {
    (
        flag(tag, "OnGround"),
        float(tag.get("fall_distance")).unwrap_or(0.0),
    )
}

/// `ItemCost.CODEC`: id, count (always written) and the exact components.
fn item_cost_to_nbt(stack: &crate::items::ItemStack) -> Tag {
    let mut tag = item_to_nbt(stack);
    if let Tag::Compound(c) = &mut tag
        && !c.contains_key(&"count".into())
    {
        c.insert("count".into(), Tag::Int(stack.count as i32));
    }
    tag
}
/// `MerchantOffers.CODEC`: `{Recipes: [...]}`, each offer without the
/// fields at their defaults.
pub fn offers_to_nbt(offers: &[crate::merchant::MerchantOffer]) -> Tag {
    let recipes = offers
        .iter()
        .map(|o| {
            let mut entries = vec![
                ("buy", item_cost_to_nbt(&o.base_cost_a.stack)),
                ("sell", item_to_nbt(&o.result)),
            ];
            // `MerchantOffer.CODEC` omits fields at their defaults.
            if o.uses != 0 {
                entries.push(("uses", Tag::Int(o.uses)));
            }
            if o.max_uses != 4 {
                entries.push(("maxUses", Tag::Int(o.max_uses)));
            }
            if o.price_multiplier != 0.0 {
                entries.push(("priceMultiplier", Tag::Float(o.price_multiplier.to_bits())));
            }
            if o.xp != 1 {
                entries.push(("xp", Tag::Int(o.xp)));
            }
            if !o.reward_exp {
                entries.push(("rewardExp", Tag::Byte(0)));
            }
            if o.special_price_diff != 0 {
                entries.push(("specialPrice", Tag::Int(o.special_price_diff)));
            }
            if o.demand != 0 {
                entries.push(("demand", Tag::Int(o.demand)));
            }
            if let Some(b) = &o.cost_b {
                entries.push(("buyB", item_cost_to_nbt(&b.stack)));
            }
            Tag::compound(entries)
        })
        .collect::<Vec<_>>();
    // An empty list is the field's default: left out.
    if recipes.is_empty() {
        return Tag::Compound(Compound::new());
    }
    Tag::compound([("Recipes", Tag::List(recipes))])
}
/// The offers of a `MerchantOffers.CODEC` tag.
pub fn offers_from_nbt(tag: &Tag) -> Vec<crate::merchant::MerchantOffer> {
    use crate::merchant::{ItemCost, MerchantOffer};
    let Some(Tag::List(recipes)) = tag.get("Recipes") else {
        return Vec::new();
    };
    recipes
        .iter()
        .filter_map(|r| {
            let cost = |key| {
                r.get(key)
                    .and_then(item_from_nbt)
                    .map(|stack| ItemCost { stack })
            };
            Some(MerchantOffer {
                base_cost_a: cost("buy")?,
                cost_b: cost("buyB"),
                result: r.get("sell").and_then(item_from_nbt)?,
                uses: number(r.get("uses")).unwrap_or(0) as i32,
                max_uses: number(r.get("maxUses")).unwrap_or(4) as i32,
                reward_exp: number(r.get("rewardExp")).unwrap_or(1) != 0,
                special_price_diff: number(r.get("specialPrice")).unwrap_or(0) as i32,
                demand: number(r.get("demand")).unwrap_or(0) as i32,
                price_multiplier: float(r.get("priceMultiplier")).unwrap_or(0.0) as f32,
                xp: number(r.get("xp")).unwrap_or(1) as i32,
            })
        })
        .collect()
}

#[cfg(test)]
mod tnt_power_tests {
    use super::*;

    /// `PrimedTnt.readAdditionalSaveData`: `explosion_power` defaults to 4 and
    /// is clamped to 0..128 both ways, so a summoned `explosion_power:200` is
    /// 128 and one that is absent is ordinary TNT.
    #[test]
    fn explosion_power_is_read_clamped_and_defaulted() {
        let with = |power: Option<f32>| {
            let mut map = std::collections::BTreeMap::new();
            if let Some(p) = power {
                map.insert("explosion_power".into(), Tag::Float(p.to_bits()));
            }
            Tag::Compound(map)
        };
        assert_eq!(tnt_power(&with(None)), 4.0);
        assert_eq!(tnt_power(&with(Some(10.0))), 10.0);
        assert_eq!(tnt_power(&with(Some(200.0))), 128.0);
        assert_eq!(tnt_power(&with(Some(-3.0))), 0.0);
        // Any numeric tag type counts, as with `getFloatOr`.
        let numeric = |tag: Tag| {
            let mut map = std::collections::BTreeMap::new();
            map.insert("explosion_power".into(), tag);
            Tag::Compound(map)
        };
        assert_eq!(tnt_power(&numeric(Tag::Int(50))), 50.0);
        assert_eq!(tnt_power(&numeric(Tag::Byte(7))), 7.0);
        assert_eq!(tnt_power(&numeric(Tag::Short(9))), 9.0);
        assert_eq!(tnt_power(&numeric(Tag::Long(500))), 128.0);
        assert_eq!(tnt_power(&numeric(Tag::Double(12.5f64.to_bits()))), 12.5);
        // And it round-trips through the save, which writes it only when it is
        // not the default.
        let saved = tnt_to_nbt(
            Vec3::ZERO,
            Vec3::ZERO,
            false,
            0.0,
            20,
            10.0,
            string("minecraft:tnt"),
            [0.0, 0.0],
            [1, 2, 3, 4],
            None,
            None,
        );
        assert_eq!(tnt_power(&saved), 10.0);
        let plain = tnt_to_nbt(
            Vec3::ZERO,
            Vec3::ZERO,
            false,
            0.0,
            20,
            4.0,
            string("minecraft:tnt"),
            [0.0, 0.0],
            [1, 2, 3, 4],
            None,
            None,
        );
        assert!(plain.get("explosion_power").is_none());
    }

    #[test]
    fn saved_power_uses_the_model_and_omits_stale_defaults() {
        let block = string("minecraft:stone");
        let save = |power, raw| {
            tnt_to_nbt(
                Vec3::new(1.5, 200.0, 4.5),
                Vec3::ZERO,
                false,
                0.0,
                80,
                power,
                block.clone(),
                [0.0; 2],
                [91, 0, 0, 1],
                Some([91, 1, 0, 90]),
                raw,
            )
        };
        let raw_absent = Tag::compound([("Silent", Tag::Byte(1))]);
        let raw_default = Tag::compound([
            ("Silent", Tag::Byte(1)),
            ("explosion_power", Tag::Float(4.0_f32.to_bits())),
        ]);
        let raw_stale = Tag::compound([
            ("Silent", Tag::Byte(1)),
            ("explosion_power", Tag::Float(2.0_f32.to_bits())),
        ]);
        for raw in [
            None,
            Some(&raw_absent),
            Some(&raw_default),
            Some(&raw_stale),
        ] {
            let saved = save(4.0, raw);
            assert!(saved.get("explosion_power").is_none(), "stale raw {raw:?}");
            assert_eq!(saved.get("fuse"), Some(&Tag::Short(80)));
            assert_eq!(saved.get("block_state"), Some(&block));
            assert_eq!(saved.get("owner"), Some(&Tag::IntArray(vec![91, 1, 0, 90])));
            assert_eq!(saved.get("UUID"), Some(&Tag::IntArray(vec![91, 0, 0, 1])));
            if raw.is_some() {
                assert_eq!(saved.get("Silent"), Some(&Tag::Byte(1)));
            }
        }
        for power in [0.0, 2.0, 5.0, 128.0] {
            for raw in [None, Some(&raw_default), Some(&raw_stale)] {
                assert_eq!(
                    save(power, raw).get("explosion_power"),
                    Some(&Tag::Float(power.to_bits()))
                );
            }
        }
    }

    /// `EntityReference.store(owner, output, "owner")`: the igniter's UUID as
    /// an int array, read back by `EntityReference.read`; an owner the save
    /// could not name keeps the loaded one from the raw compound.
    /// A dragon fireball saved with no raw compound: its own id, UUID,
    /// motion, power and owner, no fireball item or explosion power, and no
    /// fire (`DragonFireball.shouldBurn` is false); it loads back as itself.
    #[test]
    fn a_dragon_fireball_saves_and_loads_as_itself_without_raw_data() {
        let mut p = Projectile::new(
            ProjectileKind::DragonFireball,
            Vec3::new(1.5, 70.0, -2.5),
            None,
        );
        p.velocity = Vec3::new(0.1, 0.0, 0.0);
        p.acceleration = 0.1;
        p.age = 3;
        let tag = fireball_to_nbt(&p, [1, 2, 3, 4], Some([5, 6, 7, 8]), None).unwrap();
        assert_eq!(kind(&tag).as_deref(), Some("minecraft:dragon_fireball"));
        assert_eq!(uuid(&tag), Some([1, 2, 3, 4]));
        assert_eq!(uuid_from_tag(tag.get("Owner")), Some([5, 6, 7, 8]));
        assert_eq!(float(tag.get("acceleration_power")), Some(0.1));
        for absent in ["ExplosionPower", "Item"] {
            assert!(tag.get(absent).is_none(), "{absent}");
        }
        // Entity.saveWithoutId always writes Fire; a fireball that does not
        // burn is never ignited (an item fireball saves at least 20).
        assert!(
            number(tag.get("Fire")).unwrap_or(0) <= 0,
            "{:?}",
            tag.get("Fire")
        );
        let back = fireball_from_nbt(&tag, None).unwrap();
        assert_eq!(back.kind, ProjectileKind::DragonFireball);
        assert_eq!(back.velocity, p.velocity);
        assert_eq!(back.owner_uuid, Some([5, 6, 7, 8]));
        assert!(saved_with_chunk(&back));
    }
    #[test]
    fn the_owner_round_trips_as_a_uuid() {
        let save = |owner, raw: Option<&Tag>| {
            let t = string("minecraft:tnt");
            tnt_to_nbt(
                Vec3::ZERO,
                Vec3::ZERO,
                false,
                0.0,
                80,
                4.0,
                t,
                [0.0; 2],
                [1; 4],
                owner,
                raw,
            )
        };
        let saved = save(Some([5, 6, 7, 8]), None);
        assert_eq!(saved.get("owner"), Some(&Tag::IntArray(vec![5, 6, 7, 8])));
        assert_eq!(tnt_owner(&saved), Some([5, 6, 7, 8]));
        assert_eq!(tnt_owner(&save(None, None)), None);
        assert_eq!(tnt_owner(&save(None, Some(&saved))), Some([5, 6, 7, 8]));
        assert_eq!(tnt_owner(&save(Some([9; 4]), Some(&saved))), Some([9; 4]));
    }
}

#[cfg(test)]
mod spell_ticks_nbt_tests;

#[cfg(test)]
mod tests {
    // Added after the actual old Game proof: live mutation and a fresh absent
    // load cannot inherit a predicate from a contradictory saved compound.
    #[test]
    fn arrow_can_break_mutation_clone_and_absent_load_save_actual_state() {
        let parse = |text: &str| horizon_core::nbt::parse_snbt(text).unwrap().0;
        let raw = parse(
            "{id:\"minecraft:arrow\",Pos:[1d,200d,1d],NoGravity:1b,marker:37,can_break:{blocks:\"minecraft:chorus_flower\"}}",
        );
        let (mut arrow, _) = super::arrow_from_nbt(&raw, None, |_| None).unwrap();
        let original = raw.get("can_break").unwrap().clone();
        assert_eq!(
            arrow.arrow.as_ref().unwrap().can_break.as_ref(),
            Some(&original)
        );
        let replacement = parse("{blocks:\"minecraft:stone\"}");
        arrow.arrow.as_mut().unwrap().can_break = Some(replacement.clone());
        let cloned = arrow.clone();
        for source in [None, Some(&raw)] {
            let saved = super::arrow_to_nbt(&cloned, [197, 0, 0, 1], None, None, source).unwrap();
            assert_eq!(saved.get("can_break"), Some(&replacement));
            assert_eq!(saved.get("NoGravity"), Some(&super::Tag::Byte(1)));
        }
        arrow.arrow.as_mut().unwrap().can_break = None;
        let saved = super::arrow_to_nbt(&arrow, [197, 0, 0, 1], None, None, Some(&raw)).unwrap();
        assert!(saved.get("can_break").is_none());
        assert_eq!(saved.get("marker"), Some(&super::Tag::Int(37)));
        let absent = parse("{id:\"minecraft:arrow\",Pos:[1d,200d,1d]}");
        let (fresh, _) = super::arrow_from_nbt(&absent, None, |_| None).unwrap();
        assert!(fresh.arrow.as_ref().unwrap().can_break.is_none());
        assert!(
            super::arrow_to_nbt(&fresh, [197, 0, 0, 2], None, None, Some(&raw))
                .unwrap()
                .get("can_break")
                .is_none()
        );
    }

    /// `EvokerFangs` extends `Entity` and never moves: `Entity.saveWithoutId`
    /// writes the `fall_distance` it loaded, and 0 for fangs a spell made.
    #[test]
    fn evoker_fangs_keep_their_loaded_fall_distance() {
        let raw = horizon_core::nbt::parse_snbt(
            "{id:\"minecraft:evoker_fangs\",Pos:[1d,200d,1d],fall_distance:5.0d,Warmup:100}",
        )
        .unwrap()
        .0;
        let at = super::Vec3::new(1.0, 200.0, 1.0);
        let fall = |raw: Option<&super::Tag>| match super::evoker_fangs_to_nbt(
            at, 0.0, 99, None, [1; 4], raw, None,
        )
        .get("fall_distance")
        {
            Some(super::Tag::Double(bits)) => f64::from_bits(*bits),
            other => panic!("no fall_distance: {other:?}"),
        };
        assert_eq!(fall(Some(&raw)), 5.0);
        assert_eq!(fall(None), 0.0);
    }

    /// `Shulker` reads `AttachFace` as a byte before
    /// `Direction.from3DDataValue` (`abs(data % 6)`): an int is narrowed
    /// first, so 258 is byte 2, not 258 % 6 = 0, and 250 is byte -6, face 0.
    #[test]
    fn a_shulkers_attach_face_is_narrowed_to_a_byte_first() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
    use super::*;
    use crate::items::ItemStack;

    /// Older worlds name their single player by the offline UUID; the migration rewrites
    /// every reference to it (owner, thrower, trusted list, anger, leash) and nothing else.
    #[test]
    fn a_players_offline_uuid_becomes_the_accounts_everywhere_in_an_entity() {
        let old = [1, 2, 3, 4];
        let new = [5, 6, 7, 8];
        let mut tag = Tag::compound([
            ("id", string("minecraft:wolf")),
            ("UUID", Tag::IntArray(vec![9, 9, 9, 9])),
            ("Owner", Tag::IntArray(old.to_vec())),
            ("Thrower", Tag::IntArray(old.to_vec())),
            ("angry_at", Tag::IntArray(old.to_vec())),
            (
                "Trusted",
                Tag::List(vec![
                    Tag::IntArray(old.to_vec()),
                    Tag::IntArray(vec![3, 3, 3, 3]),
                ]),
            ),
            (
                "leash",
                Tag::compound([("UUID", Tag::IntArray(old.to_vec()))]),
            ),
            ("Health", Tag::Float(20.0f32.to_bits())),
        ]);
        migrate_player_uuid(&mut tag, old, new);
        assert_eq!(tag.get("Owner"), Some(&Tag::IntArray(new.to_vec())));
        assert_eq!(tag.get("Thrower"), Some(&Tag::IntArray(new.to_vec())));
        assert_eq!(tag.get("angry_at"), Some(&Tag::IntArray(new.to_vec())));
        assert_eq!(
            tag.get("Trusted"),
            Some(&Tag::List(vec![
                Tag::IntArray(new.to_vec()),
                Tag::IntArray(vec![3, 3, 3, 3]),
            ]))
        );
        assert_eq!(
            tag.get("leash").and_then(|leash| leash.get("UUID")),
            Some(&Tag::IntArray(new.to_vec()))
        );
        // The entity's own UUID is not the player's.
        assert_eq!(tag.get("UUID"), Some(&Tag::IntArray(vec![9, 9, 9, 9])));
    }

    /// An entity that never moves (an `AreaEffectCloud`) saves the
    /// `fall_distance` `Entity.load` read, and 0 when it was made in play.
    #[test]
    fn a_base_tag_keeps_the_loaded_fall_distance() {
        let raw = horizon_core::nbt::parse_snbt(
            "{id:\"minecraft:area_effect_cloud\",Pos:[1d,200d,1d],fall_distance:5.0d}",
        )
        .unwrap()
        .0;
        let fall = |raw: Option<&super::Tag>| {
            let tag = super::entity_base_tag(
                "minecraft:area_effect_cloud",
                super::Vec3::new(1.0, 200.0, 1.0),
                super::Vec3::ZERO,
                [0.0, 0.0],
                [1; 4],
                raw,
            );
            match tag.get("fall_distance") {
                Some(super::Tag::Double(bits)) => f64::from_bits(*bits),
                other => panic!("no fall_distance: {other:?}"),
            }
        };
        assert_eq!(fall(Some(&raw)), 5.0);
        assert_eq!(fall(None), 0.0);
    }

    #[test]
    fn empty_saved_tags_are_omitted_without_changing_raw_or_nonempty_tags() {
        let base = Base {
            id: "minecraft:pig",
            uuid: [328, 0, 0, 1],
            position: Vec3::new(4.5, 200.0, 4.5),
            motion: Vec3::ZERO,
            rotation: [0.0; 2],
            on_ground: false,
            fall_distance: 0.0,
        };
        let raw = |tags| {
            Tag::compound([
                ("Tags", Tag::List(tags)),
                ("PortalCooldown", Tag::Int(17)),
                ("Invulnerable", Tag::Byte(1)),
            ])
        };
        let fresh = base.tag(None);
        assert!(fresh.get("Tags").is_none());
        assert_eq!(fresh.get("PortalCooldown"), Some(&Tag::Int(0)));
        for tags in [vec![string("keep")], vec![]] {
            let raw = raw(tags.clone());
            let saved = base.tag(Some(&raw));
            assert_eq!(saved.get("PortalCooldown"), Some(&Tag::Int(17)));
            assert_eq!(saved.get("Invulnerable"), Some(&Tag::Byte(1)));
            assert_eq!(raw.get("Tags"), Some(&Tag::List(tags.clone())));
            assert_eq!(
                saved.get("Tags"),
                if tags.is_empty() {
                    None
                } else {
                    Some(Tag::List(tags))
                }
                .as_ref()
            );
        }
    }

    #[test]
    fn conversion_keeps_only_the_old_mobs_command_tags() {
        let tags = Tag::List(vec![
            Tag::String("subject".into()),
            Tag::String("quest".into()),
        ]);
        let raw = Tag::compound([
            ("id", Tag::String("minecraft:husk".into())),
            ("UUID", Tag::IntArray(vec![1, 2, 3, 4])),
            ("HuskConversionTime", Tag::Int(1)),
            ("Tags", tags.clone()),
        ]);
        let original = raw.clone();
        assert_eq!(
            conversion_tags(Some(&raw)),
            Some(Tag::compound([("Tags", tags)]))
        );
        assert_eq!(raw, original);
        assert_eq!(conversion_tags(None), None);
        assert_eq!(
            conversion_tags(Some(&Tag::compound([("Tags", Tag::List(Vec::new()))]))),
            None
        );
        assert_eq!(
            conversion_tags(Some(&Tag::Compound(Default::default()))),
            None
        );
    }

    #[test]
    fn an_ageable_mob_that_cannot_be_a_baby_saves_no_age() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn zombie_nautilus_save_omits_ignored_age_fields() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `Horse.getVariant` is `Variant.byId(typeVariant & 255)` and
    /// `getMarkings` `Markings.byId((typeVariant & 0xFF00) >> 8)`; both maps
    /// are `OutOfBoundsStrategy.WRAP` (7 coats, 5 markings), so 9 is coat 2
    /// and markings 6 is 1.
    #[test]
    fn a_horses_unknown_coat_and_markings_wrap() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `LivingEntity.readAdditionalSaveData` reads `HurtTime` and `DeathTime`
    /// with `getShortOr`: an int tag is narrowed (`(short)(value & 65535)`),
    /// so 40000 is -25536 and never counts down.
    #[test]
    fn a_mob_narrows_its_hurt_and_death_time_to_a_short() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `Axolotl.Variant.BY_ID` is `OutOfBoundsStrategy.ZERO`: an id outside
    /// 0..=4 is lucy (0). `Parrot.Variant.BY_ID` is `CLAMP`: it is the nearest
    /// end.
    #[test]
    fn an_unknown_axolotl_variant_is_lucy_and_a_parrots_is_clamped() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// A mount keeps its body armour and a helmet (say
    /// `/item replace entity <horse> armor.head with minecraft:iron_helmet`)
    /// in their own slots through a save: vanilla holds both (`equipment.body`
    /// and `equipment.head`), one in each.
    #[test]
    fn a_horses_save_keeps_a_helmet_and_its_body_armour_apart() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// A body item taken off (`/item replace entity <cube> armor.body with minecraft:air`)
    /// is not saved again from the entity's earlier save: vanilla writes `equipment.body` only
    /// while the slot holds something (#12038).
    #[test]
    fn an_emptied_body_slot_is_not_saved_from_the_earlier_nbt() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// A wolf loads its armor (`equipment.body`) into the body slot with the
    /// durability it lost, and a helmet next to it into the head slot.
    #[test]
    fn a_wolfs_save_keeps_its_armour_and_a_helmet_apart() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `Wolf` and `Cat` read `CollarColor` with `DyeColor.LEGACY_ID_CODEC`: a
    /// byte through `DyeColor.byId`, whose out-of-range answer is the first
    /// colour, white (`OutOfBoundsStrategy.ZERO`). Nothing wraps.
    #[test]
    fn a_collar_colour_outside_the_sixteen_reads_white() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn creeper_fuse_nbt_controls_explosion_tick_and_saves_runtime_value() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// #11332: `Creeper.readAdditionalSaveData` reads `ExplosionRadius` as a
    /// byte (default 3), `explodeCreeper` blows with it, twice it when powered,
    /// and `addAdditionalSaveData` writes the value back as a byte.
    #[test]
    fn creeper_explosion_radius_nbt_sizes_the_blast_and_saves() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn creeper_fall_swell_uses_custom_fuse_without_a_zero_floor() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn villager_level_round_trips_above_byte_range() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn unresolved_villager_profession_round_trips_through_entity_nbt() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn loaded_villager_command_names_follow_profession_and_custom_name() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `Entity.getDisplayName` and `saveWithoutId`: a styled custom name is
    /// the command name and is saved whole, over an older saved name.
    #[test]
    fn a_styled_mob_name_is_its_command_name_and_saved_whole() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `Component.getString`: a compound or list name is flattened whole, so
    /// `jeb_` written as `extra` still counts; a bucket keeps the styled name.
    #[test]
    fn a_compound_mob_name_flattens_and_a_bucket_keeps_its_style() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn other_mob_command_names_keep_their_type_and_custom_name() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn mob_loading_accepts_default_namespace_and_preserves_custom_namespaces() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// A saved rocket and trident come back with their own Life, LifeTime,
    /// flags and position (the keys vanilla's addAdditionalSaveData writes).
    #[test]
    fn rocket_and_trident_round_trip() {
        let item = ItemStack::new("minecraft:firework_rocket", 1);
        let tag = firework_rocket_to_nbt(
            &RocketSave {
                rotation: [90.0, 10.0],
                position: Vec3::new(1.5, 70.0, -3.25),
                motion: Vec3::new(0.0, 0.5, 0.0),
                life: 7,
                lifetime: 31,
                item: &item,
                angled: true,
                owner: None,
                left_owner: false,
                on_ground: true,
                has_been_shot: false,
                fall_distance: 2.5,
            },
            [1, 2, 3, 4],
            None,
        );
        let r = firework_rocket_from_nbt(&tag, None).unwrap();
        assert_eq!(
            (r.life, r.lifetime, r.angled, r.on_ground, r.has_been_shot),
            (7, 31, true, true, false)
        );
        assert_eq!(r.position, Vec3::new(1.5, 70.0, -3.25));
        assert_eq!(r.rotation, [90.0, 10.0]);
        // Entity.saveWithoutId/load keep the fall.
        assert_eq!(r.fall_distance, 2.5);
        let trident = ItemStack::new("minecraft:trident", 1);
        let tag = trident_to_nbt(
            &TridentSave {
                position: Vec3::new(2.0, 64.0, 2.0),
                motion: Vec3::new(0.0, -0.25, 0.0),
                yaw: 10.0,
                pitch: 20.0,
                in_ground: true,
                life: 9,
                shake: 3,
                dealt_damage: true,
                pickup: Pickup::CreativeOnly,
                left_owner: true,
                has_been_shot: true,
                owner: Some([5, 6, 7, 8]),
                no_gravity: true,
                on_ground: true,
                fire_ticks: 0,
                fall_distance: 0.0,
                sound_event: TRIDENT_HIT_GROUND,
                item: &trident,
            },
            [1, 2, 3, 4],
            None,
        );
        let t = trident_from_nbt(&tag).unwrap();
        // `Entity.saveWithoutId` writes NoGravity only when set.
        assert!(t.no_gravity);
        assert!(matches!(tag.get("NoGravity"), Some(Tag::Byte(1))));
        // Entity.load/saveWithoutId: OnGround as it was.
        assert!(t.on_ground);
        assert_eq!(tag.get("OnGround"), Some(&Tag::Byte(1)));
        assert_eq!((t.in_ground, t.life, t.shake), (true, 9, 3));
        assert_eq!((t.dealt_damage, t.pickup), (true, Pickup::CreativeOnly));
        assert!(t.left_owner && t.has_been_shot);
        assert_eq!(t.owner, Some([5, 6, 7, 8]));
        assert_eq!((t.yaw, t.pitch), (10.0, 20.0));
    }

    /// The item, the spawn delay and the fields Horizon doesn't model come
    /// back; a compound without an item is no spawner.
    #[test]
    fn ominous_item_spawner_round_trip() {
        let item = ItemStack {
            item: Arc::from("minecraft:ominous_trial_key"),
            count: 1,
            damage: 0,
            extra: None,
        };
        let raw = Tag::compound([("CustomName", string("kept"))]);
        let at = Vec3::new(10.5, 64.25, -3.5);
        let tag = ominous_item_spawner_to_nbt(&item, at, 97, [1, 2, 3, 4], Some(&raw));
        assert_eq!(
            kind(&tag).as_deref(),
            Some("minecraft:ominous_item_spawner")
        );
        assert_eq!(uuid(&tag), Some([1, 2, 3, 4]));
        assert!(matches!(
            tag.get("spawn_item_after_ticks"),
            Some(Tag::Long(97))
        ));
        assert!(tag.get("CustomName").is_some());
        let (back, pos, after) = ominous_item_spawner_from_nbt(&tag).unwrap();
        assert_eq!(
            (back.item.as_ref(), back.count),
            ("minecraft:ominous_trial_key", 1)
        );
        assert_eq!((pos, after), (at, 97));
        assert!(ominous_item_spawner_from_nbt(&raw).is_none());
        // No item is `ItemStack.EMPTY`, which writes no key back.
        let empty = ominous_item_spawner_to_nbt(
            &crate::items::ItemStack::new("minecraft:air", 1),
            at,
            0,
            [1, 2, 3, 4],
            Some(&tag),
        );
        assert!(empty.get("item").is_none());
        let (back, pos, after) = ominous_item_spawner_from_nbt(&empty).unwrap();
        assert_eq!((back.item.as_ref(), pos, after), ("minecraft:air", at, 0));
    }

    #[test]
    fn armor_stand_names_keep_their_formatting_across_a_save() {
        let tag = Tag::compound([
            (
                "Pos",
                Tag::List(vec![
                    Tag::Double(0.0f64.to_bits()),
                    Tag::Double(64.0f64.to_bits()),
                    Tag::Double(0.0f64.to_bits()),
                ]),
            ),
            ("CustomName", string("§6Hello")),
            ("CustomNameVisible", Tag::Byte(1)),
        ]);
        let a = armor_stand_from_nbt(&tag, 7).unwrap();
        let name = a.custom_name.as_ref().unwrap();
        assert_eq!(name.plain(), "Hello");
        assert!(name.spans[0].style.color.is_some());
        assert!(a.name_visible);
        let saved = armor_stand_to_nbt(&a, [0; 4], Some(&tag));
        let restored = armor_stand_from_nbt(&saved, 7).unwrap();
        assert_eq!(restored.custom_name, a.custom_name);
        assert!(restored.name_visible);
    }

    #[test]
    fn armor_stand_keeps_no_gravity_through_a_save() {
        let tag = Tag::compound([
            ("id", string("minecraft:armor_stand")),
            (
                "Pos",
                Tag::List(vec![
                    Tag::Double(0.0f64.to_bits()),
                    Tag::Double(64.0f64.to_bits()),
                    Tag::Double(0.0f64.to_bits()),
                ]),
            ),
            ("NoGravity", Tag::Byte(1)),
        ]);
        let mut a = armor_stand_from_nbt(&tag, 7).unwrap();
        assert!(a.no_gravity, "summon NBT sets the flag");
        let saved = armor_stand_to_nbt(&a, [0; 4], Some(&tag));
        assert_eq!(saved.get("NoGravity"), Some(&Tag::Byte(1)));
        assert!(armor_stand_from_nbt(&saved, 7).unwrap().no_gravity);
        // `data merge entity @s {NoGravity:0b}` clears it again.
        a.no_gravity = false;
        let cleared = armor_stand_to_nbt(&a, [0; 4], Some(&saved));
        assert!(cleared.get("NoGravity").is_none());
        assert!(!armor_stand_from_nbt(&cleared, 7).unwrap().no_gravity);
    }

    /// `Entity.load`: a Motion component past 10 blocks a tick loads as 0,
    /// the others as saved.
    #[test]
    fn armor_stand_drops_a_motion_component_past_ten() {
        let doubles = |v: [f64; 3]| Tag::List(v.map(|d| Tag::Double(d.to_bits())).to_vec());
        let tag = Tag::compound([
            ("id", string("minecraft:armor_stand")),
            ("Pos", doubles([0.0, 64.0, 0.0])),
            ("Motion", doubles([1.0e10, -0.5, -10.5])),
        ]);
        let a = armor_stand_from_nbt(&tag, 7).unwrap();
        assert_eq!(a.velocity, Vec3::new(0.0, -0.5, 0.0));
        let tag = Tag::compound([
            ("id", string("minecraft:armor_stand")),
            ("Pos", doubles([0.0, 64.0, 0.0])),
            ("Motion", doubles([10.0, 0.0, -10.0])),
        ]);
        let a = armor_stand_from_nbt(&tag, 7).unwrap();
        assert_eq!(a.velocity, Vec3::new(10.0, 0.0, -10.0), "10 itself stays");
    }

    fn falling_decode_registry() -> BlockRegistry {
        BlockRegistry::from_nbt(&Tag::compound([
            ("DataVersion", Tag::Int(horizon_world::DATA_VERSION)),
            (
                "states",
                Tag::List(
                    ["minecraft:air", "minecraft:sand", "minecraft:gravel"]
                        .into_iter()
                        .map(string)
                        .collect(),
                ),
            ),
            (
                "defaults",
                Tag::compound([
                    ("minecraft:air", Tag::Int(0)),
                    ("minecraft:sand", Tag::Int(1)),
                    ("minecraft:gravel", Tag::Int(2)),
                ]),
            ),
        ]))
        .unwrap()
    }

    fn falling_decode_input() -> Tag {
        horizon_core::nbt::parse_snbt(
            "{Pos:[3.5d,200d,4.5d],Motion:[0d,0d,0d],Time:10,DropItem:0b,NoGravity:1b}",
        )
        .unwrap()
        .0
    }

    fn falling_decode_save(registry: &BlockRegistry, input: &Tag) -> Tag {
        let (state, pos, motion, time, distance) =
            falling_block_from_nbt(input).expect("absent BlockState is valid input");
        let actual = falling_state_from_nbt(registry, &state).unwrap();
        assert_eq!(falling_block_flags(input), (true, false));
        // Save only the decoded concrete model; no original source compound.
        let saved = falling_block_to_nbt(
            state_to_nbt(registry, actual),
            pos,
            motion,
            time,
            false,
            crate::falling_block::Damage::default(),
            distance,
            false,
            [0.0, 0.0],
            [96, 0, 0, 1],
        );
        assert_eq!(saved.get("Pos"), input.get("Pos"));
        assert_eq!(saved.get("Motion"), input.get("Motion"));
        assert_eq!(saved.get("Time"), Some(&Tag::Int(10)));
        assert_eq!(saved.get("DropItem"), Some(&Tag::Byte(0)));
        saved
    }

    #[test]
    fn falling_decode_absent_state_saves_actual_sand_without_raw() {
        let registry = falling_decode_registry();
        let saved = falling_decode_save(&registry, &falling_decode_input());
        assert_eq!(saved.get("BlockState"), Some(&string("minecraft:sand")));
    }

    #[test]
    fn falling_decode_modern_boundary_keeps_shared_legacy_reader() {
        let registry = falling_decode_registry();
        let legacy = Tag::compound([("Name", string("minecraft:gravel"))]);
        assert_eq!(
            state_from_nbt(&registry, &legacy),
            registry.default_state("minecraft:gravel")
        );
        for (state, expected) in [
            (string("minecraft:sand"), "minecraft:sand"),
            (
                Tag::compound([("id", string("minecraft:gravel"))]),
                "minecraft:gravel",
            ),
            (string("minecraft:air"), "minecraft:air"),
            (legacy, "minecraft:sand"),
            (Tag::Compound(Default::default()), "minecraft:sand"),
        ] {
            let mut input = falling_decode_input();
            put(&mut input, "BlockState", state);
            let saved = falling_decode_save(&registry, &input);
            assert_eq!(saved.get("BlockState"), Some(&string(expected)));
        }
    }

    #[test]
    fn falling_block_round_trip_keeps_impact_distance_and_settings() {
        let state = Tag::compound([("Name", string("minecraft:damaged_anvil"))]);
        let position = Vec3::new(3.5, 100.0, -7.5);
        let velocity = Vec3::new(0.0, -1.25, 0.0);
        let damage = crate::falling_block::Damage {
            cancel_drop: true,
            ..crate::falling_block::Damage::anvil()
        };
        let tag = falling_block_to_nbt(
            state.clone(),
            position,
            velocity,
            30,
            false,
            damage,
            19.75,
            true,
            [0.0, 0.0],
            [1, 2, 3, 4],
        );
        let (loaded_state, pos, motion, age, distance) = falling_block_from_nbt(&tag).unwrap();
        assert_eq!(loaded_state, state);
        assert_eq!(
            (pos, motion, age, distance),
            (position, velocity, 30, 19.75)
        );
        assert_eq!(falling_block_flags(&tag), (false, false));
        assert_eq!(crate::falling_block::Damage::from_nbt(&tag, true), damage);
        assert_eq!(uuid(&tag), Some([1, 2, 3, 4]));
    }

    #[test]
    fn nbt_numeric_flags_decode_and_save_modeled_values_without_raw() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `Goat` reads `HasLeftHorn` and `HasRightHorn` with `getBooleanOr(key,
    /// true)`: a number is true when its byte value is not 0, so an int 256
    /// (byte 0) is a lost horn, as `0b` is.
    #[test]
    fn a_goats_horn_flags_are_read_as_bytes() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn non_mob_saves_drop_mob_flags_without_losing_entity_fields() {
        let uuid = [104, 0, 0, 1];
        for input in [None, Some(0), Some(1)] {
            for id in [
                "minecraft:oak_boat",
                "minecraft:oak_chest_boat",
                "minecraft:armor_stand",
            ] {
                let mut raw = Tag::compound([
                    ("id", string(id)),
                    ("Pos", doubles(&[4.5, 200.0, 4.5])),
                    ("Motion", doubles(&[0.0, 0.0, 0.0])),
                    ("Silent", Tag::Byte(1)),
                    ("NoGravity", Tag::Byte(1)),
                    ("Invulnerable", Tag::Byte(1)),
                    ("PortalCooldown", Tag::Int(17)),
                    ("Tags", Tag::List(vec![string("audit")])),
                    ("Unrelated", string("keep")),
                    (
                        "Item",
                        item_to_nbt(&crate::items::ItemStack::new("minecraft:stone", 1)),
                    ),
                ]);
                if let Some(value) = input {
                    put(&mut raw, "NoAI", Tag::Byte(value));
                    put(&mut raw, "PersistenceRequired", Tag::Byte(value));
                }
                let stand = armor_stand_from_nbt(&raw, 1).unwrap();
                let boat = boat_from_nbt(&raw).unwrap();
                for source in [None, Some(&raw)] {
                    let saved = match id {
                        "minecraft:armor_stand" => armor_stand_to_nbt(&stand, uuid, source),
                        _ => boat_to_nbt(
                            id,
                            boat.0,
                            boat.1,
                            [boat.2, 0.0],
                            (false, 0.0),
                            uuid,
                            source,
                            (id == "minecraft:oak_chest_boat").then_some(&[]),
                        ),
                    };
                    assert!(saved.get("NoAI").is_none(), "{id} {input:?}");
                    assert!(saved.get("PersistenceRequired").is_none(), "{id} {input:?}");
                    assert_eq!(saved.get("UUID"), Some(&Tag::IntArray(uuid.to_vec())));
                    assert_eq!(saved.get("Pos"), raw.get("Pos"));
                    if source.is_some() {
                        for key in [
                            "Silent",
                            "NoGravity",
                            "Invulnerable",
                            "PortalCooldown",
                            "Tags",
                            "Unrelated",
                        ] {
                            assert_eq!(saved.get(key), raw.get(key), "{id} {key}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn animal_integer_loads_save_modeled_values_without_raw_source() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn animal_nbt_preserves_signed_integer_in_love_and_defaults_after_removal() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn armor_stand_loads_and_saves_pitch() {
        let tag = Tag::compound([
            ("id", string("minecraft:armor_stand")),
            (
                "Pos",
                Tag::List(vec![
                    Tag::Double(4103.5f64.to_bits()),
                    Tag::Double(300.0f64.to_bits()),
                    Tag::Double(4103.5f64.to_bits()),
                ]),
            ),
            (
                "Rotation",
                Tag::List(vec![
                    Tag::Float((-170.5f32).to_bits()),
                    Tag::Float((-90.0f32).to_bits()),
                ]),
            ),
        ]);
        let stand = armor_stand_from_nbt(&tag, 1).unwrap();
        assert_eq!(stand.yaw, -170.5);
        assert_eq!(stand.pitch, -90.0);

        let saved = armor_stand_to_nbt(&stand, [1, 2, 3, 4], None);
        let rot = rotation(&saved);
        assert_eq!(rot[0], -170.5);
        assert_eq!(rot[1], -90.0);

        // Out-of-range pitch (e.g. 450.0f) normalizes to 90.0f matching Entity.setXRot.
        let out_of_range = Tag::compound([
            ("id", string("minecraft:armor_stand")),
            (
                "Pos",
                Tag::List(vec![
                    Tag::Double(0.0f64.to_bits()),
                    Tag::Double(0.0f64.to_bits()),
                    Tag::Double(0.0f64.to_bits()),
                ]),
            ),
            (
                "Rotation",
                Tag::List(vec![
                    Tag::Float(0.0f32.to_bits()),
                    Tag::Float(450.0f32.to_bits()),
                ]),
            ),
        ]);
        let normalized = armor_stand_from_nbt(&out_of_range, 2).unwrap();
        assert_eq!(normalized.pitch, 90.0);

        // Nonfinite pitch resets to 0.0f.
        let nonfinite = Tag::compound([
            ("id", string("minecraft:armor_stand")),
            (
                "Pos",
                Tag::List(vec![
                    Tag::Double(0.0f64.to_bits()),
                    Tag::Double(0.0f64.to_bits()),
                    Tag::Double(0.0f64.to_bits()),
                ]),
            ),
            (
                "Rotation",
                Tag::List(vec![
                    Tag::Float(0.0f32.to_bits()),
                    Tag::Float(f32::NAN.to_bits()),
                ]),
            ),
        ]);
        let nan_stand = armor_stand_from_nbt(&nonfinite, 3).unwrap();
        assert_eq!(nan_stand.pitch, 0.0);
    }

    #[test]
    fn negative_facing_wraps_like_vanilla_direction_codecs() {
        // from3DDataValue: abs(-1 % 6) = 1 (UP); 7 -> 1; -6 -> 0.
        for (raw, want) in [(-1i8, 1u8), (7, 1), (-6, 0), (5, 5), (0, 0)] {
            let tag = Tag::compound([
                ("Facing", Tag::Byte(raw)),
                ("block_pos", Tag::IntArray(vec![0, 0, 0])),
            ]);
            assert_eq!(item_frame_from_nbt(&tag, 1).unwrap().facing, want);
        }
        // from2DDataValue: abs(-1 % 4) = 1.
        let tag = Tag::compound([
            ("facing", Tag::Byte(-1)),
            ("block_pos", Tag::IntArray(vec![0, 0, 0])),
        ]);
        assert_eq!(painting_from_nbt(&tag).unwrap().1, 1);
    }

    #[test]
    fn saved_rotation_is_the_entitys_own() {
        // `/tp <entity> x y z 90 10` sets the entity's yaw and pitch; the save
        // (`/data get ... Rotation`) writes them, not [0, 0].
        let tnt = tnt_to_nbt(
            Vec3::ZERO,
            Vec3::ZERO,
            false,
            0.0,
            20,
            4.0,
            string("minecraft:tnt"),
            [90.0, 10.0],
            [1; 4],
            None,
            None,
        );
        assert_eq!(rotation(&tnt), [90.0, 10.0]);
        let mut stand = ArmorStand::new(1, Vec3::ZERO, 0.0);
        stand.yaw = 90.0;
        stand.pitch = 10.0;
        assert_eq!(
            rotation(&armor_stand_to_nbt(&stand, [1; 4], None)),
            [90.0, 10.0]
        );
        let cart = vehicle_to_nbt(
            "minecraft:minecart",
            Vec3::ZERO,
            Vec3::ZERO,
            [90.0, 10.0],
            (false, 0.0),
            [1; 4],
            None,
        );
        assert_eq!(rotation(&cart), [90.0, 10.0]);
    }

    #[test]
    fn entities_without_fire_save_zero_and_a_mob_saves_its_speed_instance() {
        // Entity.remainingFireTicks starts at 0; LivingEntity.removeFrost makes
        // the movement_speed instance AttributeMap.pack then saves.
        let orb = orbs_from_nbt(&Tag::compound([("Pos", doubles(&[0.0; 3]))])).remove(0);
        let tag = orb_to_nbt(&orb, [0; 4]);
        assert_eq!(tag.get("Fire"), Some(&Tag::Short(0)));
        // A stand summoned this tick has no instance yet; its first tick makes
        // it, and a save that lists it (`/data merge`, a reload) keeps it.
        let attributes =
            |stand: &ArmorStand| match armor_stand_to_nbt(stand, [0; 4], None).get("attributes") {
                Some(Tag::List(attrs)) => attrs
                    .iter()
                    .map(|a| (text(a.get("id")).unwrap(), a.get("base").cloned()))
                    .collect::<Vec<_>>(),
                other => panic!("attributes: {other:?}"),
            };
        let mut stand = ArmorStand::new(1, Vec3::ZERO, 0.0);
        assert_eq!(attributes(&stand), []);
        stand.speed_instance = true;
        let speed = vec![(
            "minecraft:movement_speed".to_owned(),
            Some(Tag::Double(0.7f64.to_bits())),
        )];
        assert_eq!(attributes(&stand), speed);
        let saved = armor_stand_to_nbt(&stand, [0; 4], None);
        assert_eq!(attributes(&armor_stand_from_nbt(&saved, 1).unwrap()), speed);
    }
    #[test]
    fn a_loaded_stand_gains_the_speed_instance_its_first_tick_makes() {
        // Vanilla 26.3: `/summon armor_stand ~ ~ ~ {attributes:[]}` saves `[]` until the
        // first tick, whose removeFrost makes movement_speed (base 0.7d); a saved
        // instance of another attribute stays in front of it.
        let ids = |stand: &ArmorStand, raw: &Tag| match armor_stand_to_nbt(stand, [0; 4], Some(raw))
            .get("attributes")
        {
            Some(Tag::List(attrs)) => attrs
                .iter()
                .map(|a| (text(a.get("id")).unwrap(), a.get("base").cloned()))
                .collect::<Vec<_>>(),
            other => panic!("attributes: {other:?}"),
        };
        let health = (
            "minecraft:max_health".to_owned(),
            Some(Tag::Double(20f64.to_bits())),
        );
        let speed = (
            "minecraft:movement_speed".to_owned(),
            Some(Tag::Double(0.7f64.to_bits())),
        );
        let empty = Tag::compound([("attributes", Tag::List(Vec::new()))]);
        let saved = Tag::compound([(
            "attributes",
            Tag::List(vec![Tag::compound([
                ("id", string("minecraft:max_health")),
                ("base", Tag::Double(20f64.to_bits())),
            ])]),
        )]);
        let mut stand = ArmorStand::new(1, Vec3::ZERO, 0.0);
        assert_eq!(ids(&stand, &empty), []);
        assert_eq!(ids(&stand, &saved), std::slice::from_ref(&health));
        stand.speed_instance = true;
        assert_eq!(ids(&stand, &empty), std::slice::from_ref(&speed));
        assert_eq!(ids(&stand, &saved), [health, speed]);
    }
    #[test]
    fn summoned_trident_defaults_are_not_pickable_or_already_shot() {
        let raw = Tag::compound([
            ("Pos", doubles(&[0.0, 0.0, 0.0])),
            ("LeftOwner", Tag::Byte(0)),
        ]);
        let loaded = trident_from_nbt(&raw).unwrap();
        assert_eq!(loaded.pickup, Pickup::Disallowed);
        assert!(!loaded.left_owner && !loaded.has_been_shot);
        let trident = ItemStack::new("minecraft:trident", 1);
        let tag = trident_to_nbt(
            &TridentSave {
                position: Vec3::ZERO,
                motion: Vec3::ZERO,
                yaw: 0.0,
                pitch: 0.0,
                in_ground: false,
                life: 0,
                shake: 0,
                dealt_damage: false,
                pickup: loaded.pickup,
                left_owner: loaded.left_owner,
                has_been_shot: loaded.has_been_shot,
                owner: loaded.owner,
                no_gravity: false,
                on_ground: false,
                fire_ticks: 0,
                fall_distance: 0.0,
                sound_event: TRIDENT_HIT_GROUND,
                item: &trident,
            },
            [0; 4],
            Some(&raw),
        );
        assert!(tag.get("LeftOwner").is_none());
        assert_eq!(tag.get("HasBeenShot"), Some(&Tag::Byte(0)));
        assert_eq!(tag.get("pickup"), Some(&Tag::Byte(0)));
    }
    #[test]
    fn orbs_keep_their_count() {
        let at = ("Pos", doubles(&[1.0, 2.0, 3.0]));
        let loaded = [at, ("Value", Tag::Short(7)), ("Age", Tag::Short(3))];
        let o = orbs_from_nbt(&Tag::compound(
            loaded
                .into_iter()
                .chain([("Count", Tag::Int(3)), ("Air", Tag::Short(32767))]),
        ))
        .remove(0);
        let tag = orb_to_nbt(&o, [0; 4]);
        let back = orbs_from_nbt(&tag);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].value, 7);
        assert_eq!(back[0].count, 3);
        // Entity.load keeps the saved Air; the orb writes it back unchanged.
        assert_eq!(back[0].air, 32767);
        assert_eq!(tag.get("Air"), Some(&Tag::Short(32767)));
        assert_eq!(
            orb_to_nbt(&back[0], [0; 4]).get("Air"),
            Some(&Tag::Short(32767))
        );
    }
    #[test]
    fn every_orb_set_experience_orb_accepts_reloads_unchanged() {
        let at = Tag::compound([("Pos", doubles(&[1.0, 2.0, 3.0]))]);
        let mut o = orbs_from_nbt(&at).remove(0);
        for (value, count) in [(11, 1), (32767, 2147483647), (-32768, 1)] {
            (o.value, o.count) = (value, count);
            let back = orbs_from_nbt(&orb_to_nbt(&o, [0; 4])).remove(0);
            assert_eq!((back.value, back.count), (value, count));
        }
    }
    /// `Entity.saveWithoutId` writes `Fire` for every entity and `Entity.load`
    /// reads it back with `getShortOr("Fire", 0)`: a summon's `Fire:40s` and the
    /// `-1s` an entity carries after leaving water both survive, an absent key
    /// is 0, and the short wraps like `(short) remainingFireTicks`.
    #[test]
    fn orbs_save_and_load_their_fire() {
        let mut o = ExperienceOrb {
            attachment_lifetime: Default::default(),
            rotation: [0.0, 0.0],
            join: Default::default(),
            no_gravity: false,
            id: 0,
            position: Vec3::new(1.0, 2.0, 3.0),
            previous: Vec3::ZERO,
            velocity: Vec3::ZERO,
            value: 7,
            age: 3,
            on_ground: false,
            count: 1,
            ticks: 0,
            health: 5,
            air: 300,
            following: None,
            fall_distance: 0.0,
            fire_ticks: 0,
        };
        for (ticks, saved) in [(0, 0), (40, 40), (-1, -1), (300, 300), (40_000, -25_536)] {
            o.fire_ticks = ticks;
            let tag = orb_to_nbt(&o, [0; 4]);
            assert_eq!(tag.get("Fire"), Some(&Tag::Short(saved)), "saved {ticks}");
            assert_eq!(orbs_from_nbt(&tag)[0].fire_ticks, i32::from(saved));
        }
        let mut tag = orb_to_nbt(&o, [0; 4]);
        let Tag::Compound(map) = &mut tag else {
            unreachable!()
        };
        map.remove(&"Fire".into());
        assert_eq!(orbs_from_nbt(&tag)[0].fire_ticks, 0);
    }
    /// `Entity.baseTick` takes one tick off a burning `Fire` (`Fire:300s` reads
    /// 299 after one tick); 0 and -1 stay. A fire-immune cloud clears only a
    /// positive `Fire` on its tick.
    #[test]
    fn a_loaded_fire_counts_down_and_a_cloud_clears_it() {
        let fire = |snbt: &str| {
            let mut tag = horizon_core::nbt::parse_snbt(snbt).unwrap().0;
            super::count_down_fire(&mut tag);
            number(tag.get("Fire"))
        };
        assert_eq!(fire("{Fire:300s}"), Some(299));
        assert_eq!(fire("{Fire:1s}"), Some(0));
        assert_eq!(fire("{Fire:0s}"), Some(0));
        assert_eq!(fire("{Fire:-1s}"), Some(-1));
        let cleared = |snbt: &str| {
            let mut tag = horizon_core::nbt::parse_snbt(snbt).unwrap().0;
            super::clear_positive_fire(&mut tag);
            number(tag.get("Fire"))
        };
        assert_eq!(cleared("{Fire:300s}"), Some(0));
        assert_eq!(cleared("{Fire:-5s}"), Some(-5));
        assert_eq!(cleared("{Fire:0s}"), Some(0));
        assert_eq!(cleared("{Age:5}"), None);
    }
}

/// `Axolotl.Variant` names, by id.
pub const AXOLOTL_VARIANTS: [&str; 5] = ["lucy", "wild", "gold", "cyan", "blue"];
/// `TropicalFish.Pattern` names in enum order: the six small patterns, then
/// the six large ones (the order of `extra.variant`).
pub const TROPICAL_PATTERNS: [&str; 12] = [
    "kob",
    "sunstreak",
    "snooper",
    "dasher",
    "brinely",
    "spotty",
    "flopper",
    "stripey",
    "glitter",
    "blockfish",
    "betty",
    "clayfish",
];
#[cfg(test)]
mod axolotl_nbt_tests {
    use super::*;
    #[test]
    fn axolotl_boolean_memory_preserves_values_and_saved_lifetimes() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
    #[test]
    fn aquatic_from_bucket_saves_defaults_and_tadpole_constant() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `HappyGhast.addAdditionalSaveData` / `readAdditionalSaveData`: the still timeout
    /// (`serverStillTimeout`) goes with the world. A ghast saved while it was held still
    /// comes back still, which is what keeps the box a dropped item rests on
    /// (`EntityGetter.getEntityCollisions`) and the control a rider has
    /// (`getControllingPassenger` reads `isOnStillTimeout`).
    #[test]
    fn a_happy_ghast_keeps_its_still_timeout() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn bucket_cooldown_and_brain_memories_survive_and_expired_cooldown_is_removed() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
}

#[cfg(test)]
mod sheep_nbt_tests {
    use super::*;
    #[test]
    fn a_sheep_keeps_its_dyed_and_sheared_wool() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn a_snow_golem_keeps_its_pumpkin_state() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `Hoglin.readAdditionalSaveData` and `addAdditionalSaveData`:
    /// `CannotBeHunted` (a bastion's hoglins) stays with the hoglin and is always
    /// written, and such a hoglin is no piglin's prey (`Hoglin.canBeHunted`).
    #[test]
    fn a_hoglin_keeps_cannot_be_hunted_and_is_no_prey() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `Mob.addAdditionalSaveData` and `readAdditionalSaveData`:
    /// `DeathLootTable` and a set `DeathLootTableSeed` stay with the mob.
    #[test]
    fn a_death_loot_table_round_trips() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `Rabbit.addAdditionalSaveData` and `readAdditionalSaveData`: `MoreCarrotTicks`.
    #[test]
    fn a_rabbit_keeps_how_full_of_carrots_it_is() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `Ocelot.addAdditionalSaveData` and `readAdditionalSaveData`: `Trusting`.
    #[test]
    fn an_ocelot_keeps_its_trust() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `Mannequin.addAdditionalSaveData` and `readAdditionalSaveData`: `immovable`, and the
    /// description (the default is not written, `hide_description` hides it, a component stays).
    #[test]
    fn a_mannequin_keeps_its_immovable_flag_and_description() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// The only lib test that registers mod effects (once per process).
    #[test]
    fn a_mob_keeps_a_mod_effect() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn an_amplifier_of_zero_is_left_out_of_active_effects() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn farm_animals_keep_their_variant_and_sound_variant() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn sound_variants_follow_the_spawn_registries() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/myriade-engine")
            .join(horizon_core::VANILLA_VERSION);
        if !root.join("items.json").exists() {
            return;
        }
        // The sound variants are indices into the registries finalizeSpawn draws from.
        use crate::mobs::{Species, finalize};
        let registries = finalize::SpawnRegistries::load(&root);
        for (species, registry) in [
            (Species::Pig, &registries.pig_sound),
            (Species::Cow, &registries.cow_sound),
            (Species::Chicken, &registries.chicken_sound),
            (Species::Cat, &registries.cat_sound),
            (Species::Wolf, &registries.wolf_sound),
        ] {
            assert_eq!(
                finalize::sound_variants(species),
                registry.names,
                "{species:?}"
            );
        }
    }

    #[test]
    fn parrots_omit_raw_age_and_pigs_keep_modeled_age() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn a_baby_villager_keeps_its_age() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `Dolphin.addAdditionalSaveData`: a fed dolphin stays fed and a wet one
    /// stays wet across a reload; a save with neither field reads as an unfed
    /// dolphin at full moistness.
    #[test]
    fn a_dolphins_moistness_and_fish_survive_a_save() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `WanderingTrader.addAdditionalSaveData`: the visit's countdown and the
    /// place it is walking to (a `BlockPos` codec, so an int array), which the
    /// despawn and stroll goals read back after a reload.
    #[test]
    fn a_wandering_traders_countdown_and_target_survive_a_save() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn mobs_keep_their_variant_puff_state_carried_block_and_conversion_times() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
}

#[cfg(test)]
mod item_component_tests {
    use super::*;
    #[test]
    fn a_spawn_egg_beats_the_biome_variant() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `Fox.applyImplicitComponent` and `Frog.applyImplicitComponent`: the
    /// variant component alone decides the mob, `setVariant` from the egg.
    #[test]
    fn a_fox_or_frog_spawn_egg_picks_its_variant_component() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `Horse.applyImplicitComponent` and `Llama.applyImplicitComponent`: the
    /// `.../variant` component sets the coat colour. A horse's markings are
    /// the other half of the packed variant its spawn picked and stay.
    #[test]
    fn a_horse_or_llama_spawn_egg_picks_its_variant_component() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `EntityType.updateCustomEntityTag`: the egg's `entity_data` on the mob it
    /// spawned, when the type is its own — the mob's compound, the data merged
    /// over it and loaded back, as the spawn egg site does it.
    #[test]
    fn a_spawn_eggs_entity_data_reaches_the_mob_it_spawned() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
}

#[cfg(test)]
mod fireball_nbt_tests {
    use super::*;

    /// `WitherSkull`: a skull in flight is saved with its chunk, `dangerous`
    /// is a byte (absent means a black skull), and it never saves a fire
    /// because `WitherSkull.shouldBurn` is false.
    #[test]
    fn a_wither_skull_keeps_whether_it_is_dangerous() {
        let blue = Tag::compound([
            ("id", Tag::String("minecraft:wither_skull".into())),
            ("Pos", doubles(&[3.0, 64.0, 3.0])),
            ("Motion", doubles(&[0.0, 0.0, 0.0])),
            ("dangerous", Tag::Byte(1)),
        ]);
        let mut skull = fireball_from_nbt(&blue, None).unwrap();
        assert_eq!(skull.kind, ProjectileKind::WitherSkull { dangerous: true });
        assert!(saved_with_chunk(&skull));
        skull.age = 5;
        let back = fireball_to_nbt(&skull, [1, 2, 3, 4], None, None).unwrap();
        assert_eq!(kind(&back).as_deref(), Some("minecraft:wither_skull"));
        assert_eq!(number(back.get("dangerous")), Some(1));
        // Entity saves Fire every time; a skull's stays 0 (a fireball's is 20 once ticked).
        assert_eq!(number(back.get("Fire")), Some(0));

        let black = Tag::compound([
            ("id", Tag::String("minecraft:wither_skull".into())),
            ("Pos", doubles(&[0.0, 0.0, 0.0])),
        ]);
        let skull = fireball_from_nbt(&black, None).unwrap();
        assert_eq!(skull.kind, ProjectileKind::WitherSkull { dangerous: false });
        let back = fireball_to_nbt(&skull, [1, 2, 3, 4], None, None).unwrap();
        assert_eq!(number(back.get("dangerous")), Some(0));
    }

    #[test]
    fn a_fireball_round_trips_its_flight() {
        let raw = Tag::compound([
            ("id", Tag::String("minecraft:fireball".into())),
            ("Pos", doubles(&[1.0, 70.0, -2.5])),
            ("Motion", doubles(&[0.0, -0.5, 1.0])),
            ("ExplosionPower", Tag::Byte(4)),
            ("acceleration_power", Tag::Double(0.0f64.to_bits())),
        ]);
        let p = fireball_from_nbt(&raw, None).unwrap();
        assert_eq!(p.kind, ProjectileKind::LargeFireball { power: 4 });
        assert_eq!((p.velocity.z, p.acceleration), (1.0, 0.0));
        let back = fireball_to_nbt(&p, [1, 2, 3, 4], None, None).unwrap();
        assert_eq!(number(back.get("ExplosionPower")), Some(4));
        assert_eq!(float(back.get("acceleration_power")), Some(0.0));
        // Projectile.addAdditionalSaveData: no LeftOwner unless set, HasBeenShot always.
        assert!(back.get("LeftOwner").is_none());
        assert_eq!(number(back.get("HasBeenShot")), Some(0));
        assert_eq!(
            read_doubles::<3>(back.get("Motion")),
            Some([0.0, -0.5, 1.0])
        );
        // A default fireball keeps vanilla's power and acceleration.
        let plain = Tag::compound([
            ("id", Tag::String("minecraft:fireball".into())),
            ("Pos", doubles(&[0.0, 0.0, 0.0])),
        ]);
        let p = fireball_from_nbt(&plain, None).unwrap();
        assert_eq!(
            (p.kind, p.acceleration),
            (ProjectileKind::LargeFireball { power: 1 }, 0.1)
        );
        // `/summon` and a saved world give back both wind charges.
        for (id, kind) in [
            ("minecraft:wind_charge", ProjectileKind::WindCharge),
            (
                "minecraft:breeze_wind_charge",
                ProjectileKind::BreezeWindCharge,
            ),
        ] {
            let tag = Tag::compound([
                ("id", Tag::String(id.into())),
                ("Pos", doubles(&[0.0, 0.0, 0.0])),
            ]);
            assert_eq!(fireball_from_nbt(&tag, None).unwrap().kind, kind);
        }
    }

    /// `Fireball.addAdditionalSaveData` / `readAdditionalSaveData`: both fireballs save
    /// `Item`, a fire charge by default, and a loaded one stays, as one item.
    #[test]
    fn a_fireball_saves_its_item() {
        let item_of = |tag: &Tag| {
            let item = tag.get("Item").and_then(item_from_nbt).unwrap();
            (item.item.to_string(), item.count)
        };
        for id in ["minecraft:fireball", "minecraft:small_fireball"] {
            let plain = Tag::compound([
                ("id", Tag::String(id.into())),
                ("Pos", doubles(&[0.0, 0.0, 0.0])),
            ]);
            let p = fireball_from_nbt(&plain, None).unwrap();
            let back = fireball_to_nbt(&p, [1, 2, 3, 4], None, Some(&plain)).unwrap();
            assert_eq!(
                item_of(&back),
                ("minecraft:fire_charge".to_owned(), 1),
                "{id}"
            );
            // A loaded item stays, as a single one, on the entity itself.
            let mut loaded = plain.clone();
            put(
                &mut loaded,
                "Item",
                item_to_nbt(&crate::items::ItemStack::new("minecraft:diamond", 5)),
            );
            let p = fireball_from_nbt(&loaded, None).unwrap();
            let held = p.item.as_ref().unwrap();
            assert_eq!((&*held.item, held.count), ("minecraft:diamond", 1), "{id}");
            // `addAdditionalSaveData` writes `getItem()`: the item in the
            // compound it was loaded from does not replace the one it holds,
            // which is what `/data get` and the chunk save then report.
            let mut stale = plain.clone();
            put(
                &mut stale,
                "Item",
                item_to_nbt(&crate::items::ItemStack::new("minecraft:stone", 8)),
            );
            let back = fireball_to_nbt(&p, [1, 2, 3, 4], None, Some(&stale)).unwrap();
            assert_eq!(item_of(&back), ("minecraft:diamond".to_owned(), 1), "{id}");
            // An empty one falls back to the fire charge.
            let mut empty = plain.clone();
            put(
                &mut empty,
                "Item",
                item_to_nbt(&crate::items::ItemStack::new("minecraft:air", 1)),
            );
            let p = fireball_from_nbt(&empty, None).unwrap();
            let back = fireball_to_nbt(&p, [1, 2, 3, 4], None, Some(&empty)).unwrap();
            assert_eq!(
                item_of(&back),
                ("minecraft:fire_charge".to_owned(), 1),
                "{id}"
            );
        }
        // The wind charges are not `Fireball`s: they save no item.
        let wind = Tag::compound([
            ("id", Tag::String("minecraft:wind_charge".into())),
            ("Pos", doubles(&[0.0, 0.0, 0.0])),
        ]);
        let p = fireball_from_nbt(&wind, None).unwrap();
        let back = fireball_to_nbt(&p, [1, 2, 3, 4], None, Some(&wind)).unwrap();
        assert!(back.get("Item").is_none());
    }

    #[test]
    fn a_summoned_potion_flies_as_the_item_its_compound_names() {
        // `ThrowableItemProjectile.getDefaultItem`: the item its entity type is
        // named after, so a bare summon is a splash potion with no effects and
        // the chunk saves it (ThrowableProjectile is not transient).
        for id in ["minecraft:splash_potion", "minecraft:lingering_potion"] {
            let bare = Tag::compound([
                ("id", Tag::String(id.into())),
                ("Pos", doubles(&[24.5, 216.0, 24.5])),
            ]);
            let p = throwable_from_nbt(&bare, None).unwrap();
            assert_eq!(p.kind.id(), id);
            assert_eq!(
                p.kind,
                ProjectileKind::Potion(
                    Box::new(crate::items::ItemStack::new(id, 1)),
                    id == "minecraft:lingering_potion"
                )
            );
            assert!(saved_with_chunk(&p));
            let tag = throwable_to_nbt(&p, [1; 4], None, None).unwrap();
            let item = tag.get("Item").and_then(item_from_nbt).unwrap();
            assert_eq!((&*item.item, item.count), (id, 1));
            assert_eq!(item.potion(), None);
        }
        // `readAdditionalSaveData`: the `Item` it flies as, `potion_contents`
        // and all, so what it splashes is the potion's.
        let harming = Tag::compound([
            ("id", Tag::String("minecraft:splash_potion".into())),
            ("Pos", doubles(&[24.5, 216.0, 24.5])),
            (
                "Item",
                item_to_nbt(
                    &crate::items::ItemStack::new("minecraft:splash_potion", 1)
                        .with_potion("minecraft:harming"),
                ),
            ),
        ]);
        let p = throwable_from_nbt(&harming, None).unwrap();
        let ProjectileKind::Potion(stack, _) = &p.kind else {
            panic!("a potion");
        };
        assert_eq!(stack.potion(), Some("minecraft:harming"));
        // The stack stays in the tag it was loaded from.
        let tag = throwable_to_nbt(&p, [1; 4], None, Some(&harming)).unwrap();
        let item = tag.get("Item").and_then(item_from_nbt).unwrap();
        assert_eq!(item.potion(), Some("minecraft:harming"));
        // A lingering potion is its own entity type.
        let lingering = Tag::compound([
            ("id", Tag::String("minecraft:lingering_potion".into())),
            ("Pos", doubles(&[0.0, 64.0, 0.0])),
            (
                "Item",
                item_to_nbt(
                    &crate::items::ItemStack::new("minecraft:lingering_potion", 1)
                        .with_potion("minecraft:poison"),
                ),
            ),
        ]);
        let p = throwable_from_nbt(&lingering, None).unwrap();
        assert_eq!(p.kind.id(), "minecraft:lingering_potion");
    }

    #[test]
    fn wind_charges_in_flight_are_saved_and_read_back() {
        // vanilla 26.3 saves them (`Entity.shouldBeSaved` is true; the round
        // trip of a vanilla world lost all four before this).
        for id in ["minecraft:wind_charge", "minecraft:breeze_wind_charge"] {
            let saved = Tag::compound([
                ("id", Tag::String(id.into())),
                ("Pos", doubles(&[4.5, 80.0, 10.5])),
                ("NoGravity", Tag::Byte(1)),
                ("acceleration_power", Tag::Double(0.1_f64.to_bits())),
            ]);
            let p = fireball_from_nbt(&saved, None).unwrap();
            assert_eq!(p.kind.id(), id);
            assert!(saved_with_chunk(&p), "{id}");
            let tag = fireball_to_nbt(&p, [1; 4], None, Some(&saved)).unwrap();
            assert_eq!(kind(&tag).as_deref(), Some(id));
            assert_eq!(tag.get("NoGravity"), Some(&Tag::Byte(1)));
        }
    }

    #[test]
    fn a_lingering_potion_keeps_its_type_whatever_item_it_flies_as() {
        // `/summon minecraft:lingering_potion ~ ~ ~ {Item:{id:"minecraft:splash_potion"}}`
        // is a `ThrownLingeringPotion` in vanilla and stays one when saved.
        let tag = Tag::compound([
            ("id", Tag::String("minecraft:lingering_potion".into())),
            ("Pos", doubles(&[24.5, 80.0, 8.5])),
            (
                "Item",
                item_to_nbt(&crate::items::ItemStack::new("minecraft:splash_potion", 1)),
            ),
        ]);
        let p = throwable_from_nbt(&tag, None).unwrap();
        assert_eq!(p.kind.id(), "minecraft:lingering_potion");
        let back = throwable_to_nbt(&p, [1; 4], None, Some(&tag)).unwrap();
        assert_eq!(kind(&back).as_deref(), Some("minecraft:lingering_potion"));
        let item = back.get("Item").and_then(item_from_nbt).unwrap();
        assert_eq!(&*item.item, "minecraft:splash_potion");
    }

    #[test]
    fn a_painting_keeps_its_saved_pitch_and_takes_its_yaw_from_the_facing() {
        // Entity.load sets both angles; Painting.readAdditionalSaveData's
        // setDirection only rewrites the yaw. `/summon painting ...
        // {Rotation:[45.0f,10.0f]}` reads back `Rotation: [0.0f, 10.0f]`.
        let painting_at = |rotation: Option<Tag>| {
            let mut tag = Tag::compound([
                ("id", Tag::String("minecraft:painting".into())),
                ("Pos", doubles(&[28.5, 80.0, 6.5])),
                ("variant", Tag::String("minecraft:kebab".into())),
                ("facing", Tag::Byte(3)),
            ]);
            if let (Tag::Compound(map), Some(rotation)) = (&mut tag, rotation) {
                map.insert("Rotation".into(), rotation);
            }
            tag
        };
        let saved = |raw: &Tag| {
            let (pos, facing, variant) = painting_from_nbt(raw).unwrap();
            let painting = crate::entities::Painting {
                attachment_lifetime: Default::default(),
                join: Default::default(),
                id: 1,
                pos,
                facing,
                variant: variant.unwrap().into(),
                width: 1,
                height: 1,
                check_interval: 0,
            };
            rotation(&painting_to_nbt(&painting, [1; 4], Some(raw)))
        };
        assert_eq!(
            saved(&painting_at(Some(floats(&[45.0, 10.0])))),
            [270.0, 10.0]
        );
        // setXRot: `xRot % 360` clamped to +-90.
        assert_eq!(
            saved(&painting_at(Some(floats(&[0.0, -400.0])))),
            [270.0, -40.0]
        );
        assert_eq!(
            saved(&painting_at(Some(floats(&[0.0, 135.0])))),
            [270.0, 90.0]
        );
        // A non-finite pitch is ignored, as is no Rotation at all.
        assert_eq!(
            saved(&painting_at(Some(floats(&[0.0, f32::NAN])))),
            [270.0, 0.0]
        );
        assert_eq!(saved(&painting_at(None)), [270.0, 0.0]);
    }

    #[test]
    fn a_summoned_painting_or_frame_has_no_block_pos_and_sits_at_its_position() {
        // BlockAttachedEntity.setPos: the block holding the summon position.
        let painting = Tag::compound([
            ("id", Tag::String("minecraft:painting".into())),
            ("Pos", doubles(&[28.5, 80.0, 6.5])),
            ("variant", Tag::String("minecraft:kebab".into())),
            ("facing", Tag::Byte(3)),
        ]);
        assert_eq!(
            painting_from_nbt(&painting),
            Some(([28, 80, 6], 3, Some("minecraft:kebab".to_string())))
        );
        // No variant saved (a bare summon): the position and facing still load.
        let mut bare = painting.clone();
        if let Tag::Compound(m) = &mut bare {
            m.remove(&"variant".into());
        }
        assert_eq!(painting_from_nbt(&bare), Some(([28, 80, 6], 3, None)));
        let frame = Tag::compound([
            ("id", Tag::String("minecraft:item_frame".into())),
            ("Pos", doubles(&[-0.5, 80.0, 6.5])),
            ("Facing", Tag::Byte(1)),
        ]);
        assert_eq!(item_frame_from_nbt(&frame, 1).unwrap().pos, [-1, 80, 6]);
        // BlockAttachedEntity.readAdditionalSaveData: a block_pos 16 blocks or
        // more from the Pos block is refused for that block.
        let mut far = painting.clone();
        put(&mut far, "block_pos", Tag::IntArray(vec![28, 80, 22]));
        assert_eq!(painting_from_nbt(&far).map(|p| p.0), Some([28, 80, 6]));
        let mut near = painting.clone();
        put(&mut near, "block_pos", Tag::IntArray(vec![28, 80, 21]));
        assert_eq!(painting_from_nbt(&near).map(|p| p.0), Some([28, 80, 21]));
    }

    #[test]
    fn frame_missing_facing_loads_down_and_valid_directions_round_trip() {
        // ItemFrame.readAdditionalSaveData: LEGACY_ID_CODEC, orElse(DOWN).
        for kind in ["minecraft:item_frame", "minecraft:glow_item_frame"] {
            // Explicit controls run before the missing-field assertion.
            for facing in [Some(0), Some(1), Some(2), Some(3), Some(4), Some(5), None] {
                let mut tag = Tag::compound([
                    ("id", string(kind)),
                    ("block_pos", Tag::IntArray(vec![4, 102, 6])),
                    ("Fixed", Tag::Byte(1)),
                    ("ItemRotation", Tag::Byte(5)),
                    ("ItemDropChance", Tag::Float(0.25_f32.to_bits())),
                    (
                        "Item",
                        item_to_nbt(&crate::items::ItemStack::new("minecraft:diamond", 1)),
                    ),
                ]);
                if let Some(direction) = facing {
                    put(&mut tag, "Facing", Tag::Byte(direction));
                }
                let frame = item_frame_from_nbt(&tag, 17).unwrap();
                assert_eq!(frame.kind(), kind);
                assert_eq!(frame.id, 17);
                assert!(frame.fixed);
                assert_eq!(
                    frame.item.as_ref().unwrap().item.as_ref(),
                    "minecraft:diamond"
                );
                for raw in [None, Some(&tag)] {
                    let saved = item_frame_to_nbt(&frame, [1, 2, 3, 4], raw);
                    assert_eq!(
                        saved.get("Facing"),
                        Some(&Tag::Byte(facing.unwrap_or(0))),
                        "{kind}"
                    );
                    assert_eq!(saved.get("Item"), tag.get("Item"));
                    assert_eq!(saved.get("ItemRotation"), Some(&Tag::Byte(5)));
                    assert_eq!(saved.get("ItemDropChance"), tag.get("ItemDropChance"));
                    assert_eq!(saved.get("Fixed"), Some(&Tag::Byte(1)));
                    assert_eq!(saved.get("block_pos"), tag.get("block_pos"));
                    if facing.is_none() {
                        assert_eq!(saved.get("Pos"), Some(&doubles(&[4.5, 102.96875, 6.5])));
                    }
                }
            }
        }
    }

    #[test]
    fn frame_rotation_uses_signed_byte_value_and_round_trips() {
        // `ItemFrame.readAdditionalSaveData`: getByteOr, then setRotation's `% 8`.
        for kind in ["minecraft:item_frame", "minecraft:glow_item_frame"] {
            for (rotation, expected) in [
                (Some(Tag::Byte(-1)), -1),
                (Some(Tag::Byte(-9)), -1),
                (Some(Tag::Byte(-128)), 0),
                (Some(Tag::Byte(0)), 0),
                (Some(Tag::Byte(7)), 7),
                (Some(Tag::Byte(8)), 0),
                (Some(Tag::Byte(127)), 7),
                (Some(Tag::Short(255)), -1),
                (Some(Tag::Int(129)), -7),
                (Some(Tag::Int(-129)), 7),
                (Some(Tag::Long(255)), -1),
                (Some(Tag::Float((-0.5_f32).to_bits())), -1),
                (Some(Tag::Double((-9.5_f64).to_bits())), -2),
                (Some(string("-1")), 0),
                (None, 0),
            ] {
                let mut tag = Tag::compound([
                    ("id", string(kind)),
                    ("block_pos", Tag::IntArray(vec![4, 102, 6])),
                ]);
                if let Some(rotation) = rotation {
                    put(&mut tag, "ItemRotation", rotation);
                }
                let frame = item_frame_from_nbt(&tag, 17).unwrap();
                assert_eq!(frame.rotation, expected, "{kind}: {tag:?}");
                for raw in [None, Some(&tag)] {
                    let saved = item_frame_to_nbt(&frame, [1, 2, 3, 4], raw);
                    assert_eq!(saved.get("ItemRotation"), Some(&Tag::Byte(expected)));
                    assert_eq!(item_frame_from_nbt(&saved, 17).unwrap().rotation, expected);
                }
            }
        }
    }

    #[test]
    fn a_named_frame_drops_with_its_custom_name() {
        // `ItemFrame.getFrameItemStackWithData`: the frame item, then
        // `DataComponents.CUSTOM_NAME` from `getCustomName`. Glow frames
        // share it through `GlowItemFrame.getFrameItemStack`.
        for id in ["minecraft:item_frame", "minecraft:glow_item_frame"] {
            let tag = Tag::compound([
                ("id", string(id)),
                ("Pos", doubles(&[0.5, 64.0, 0.5])),
                ("Facing", Tag::Byte(2)),
                ("CustomName", string("Gallery")),
            ]);
            let frame = item_frame_from_nbt(&tag, 1).unwrap();
            let stack = frame.item_stack_with_data();
            assert_eq!(&*stack.item, id);
            assert_eq!(stack.count, 1);
            assert_eq!(stack.custom_name().as_deref(), Some("Gallery"));
            let saved = item_frame_to_nbt(&frame, [1, 2, 3, 4], None);
            assert_eq!(text(saved.get("CustomName")).as_deref(), Some("Gallery"));
        }
        let bare = Tag::compound([
            ("id", string("minecraft:item_frame")),
            ("Pos", doubles(&[0.5, 64.0, 0.5])),
            ("Facing", Tag::Byte(3)),
        ]);
        let frame = item_frame_from_nbt(&bare, 2).unwrap();
        assert!(frame.custom_name.is_none());
        assert!(frame.item_stack_with_data().custom_name().is_none());
        assert!(
            item_frame_to_nbt(&frame, [1, 2, 3, 4], Some(&bare))
                .get("CustomName")
                .is_none()
        );
    }

    #[test]
    fn projectile_shot_flag_round_trips_independently_of_age() {
        for id in [
            "minecraft:snowball",
            "minecraft:fireball",
            "minecraft:llama_spit",
            "minecraft:shulker_bullet",
        ] {
            for shot in [0, 1] {
                let raw = Tag::compound([
                    ("id", Tag::String(id.into())),
                    ("Pos", doubles(&[1.0, 70.0, 2.0])),
                    ("HasBeenShot", Tag::Byte(shot)),
                ]);
                let mut p = throwable_from_nbt(&raw, None)
                    .or_else(|| fireball_from_nbt(&raw, None))
                    .or_else(|| spit_or_bullet_from_nbt(&raw, None, None))
                    .unwrap();
                assert_eq!(p.has_been_shot, shot != 0);
                for age in [0, 12] {
                    p.age = age;
                    let tag = throwable_to_nbt(&p, [1, 2, 3, 4], None, Some(&raw))
                        .or_else(|| fireball_to_nbt(&p, [1, 2, 3, 4], None, Some(&raw)))
                        .or_else(|| spit_or_bullet_to_nbt(&p, [1, 2, 3, 4], None, None, Some(&raw)))
                        .unwrap();
                    assert_eq!(number(tag.get("HasBeenShot")), Some(i64::from(shot)));
                }
            }
        }
    }

    #[test]
    fn thrown_item_projectiles_round_trip_their_flight_item_and_owner() {
        let player = Some([5, 6, 7, 8]);
        for (kind, id) in [
            (ProjectileKind::Snowball, "minecraft:snowball"),
            (ProjectileKind::Egg, "minecraft:egg"),
            (ProjectileKind::EnderPearl, "minecraft:ender_pearl"),
        ] {
            let mut p =
                Projectile::new(kind.clone(), Vec3::new(1.5, 70.0, -2.5), Some(Who::Player));
            (p.velocity, p.left_owner, p.age) = (Vec3::new(0.3, -0.1, 1.2), true, 4);
            p.has_been_shot = true;
            [p.yaw, p.pitch] = [30.0, -10.0];
            let tag = throwable_to_nbt(&p, [1, 2, 3, 4], player, None).unwrap();
            assert_eq!(super::kind(&tag).as_deref(), Some(id));
            // ThrowableItemProjectile: the item it flies as, by its default.
            let item = tag.get("Item").and_then(item_from_nbt).unwrap();
            assert_eq!((&*item.item, item.count), (id, 1));
            assert_eq!(number(tag.get("HasBeenShot")), Some(1));
            assert_eq!(uuid_from_tag(tag.get("Owner")), player);
            let back = throwable_from_nbt(&tag, player).unwrap();
            assert_eq!((back.kind, back.owner), (kind, Some(Who::Player)));
            assert_eq!((back.position, back.velocity), (p.position, p.velocity));
            assert_eq!(
                ([back.yaw, back.pitch], back.left_owner),
                ([30.0, -10.0], true)
            );
            // an owner who is not here keeps their UUID through a moved resave.
            let mut away = throwable_from_nbt(&tag, Some([9; 4])).unwrap();
            assert_eq!((away.owner, away.owner_uuid), (None, player));
            away.position.y += 3.0;
            let resaved = throwable_to_nbt(&away, [1; 4], away.owner_uuid, None).unwrap();
            assert_eq!(uuid_from_tag(resaved.get("Owner")), player);
        }
        // the chunk saves arrows, fireballs, wind charges and these, and so takes them with it.
        let kinds = [
            (ProjectileKind::SmallFireball, true),
            (ProjectileKind::Snowball, true),
            (ProjectileKind::WindCharge, true),
            (ProjectileKind::BreezeWindCharge, true),
            (ProjectileKind::Trident, false),
        ];
        for (kind, saved) in kinds {
            let p = Projectile::new(kind, Vec3::ZERO, None);
            assert_eq!(saved_with_chunk(&p), saved, "{:?}", p.kind);
        }
        // a loaded item stays, and a new projectile leaves LeftOwner out.
        let raw = Tag::compound([
            ("id", Tag::String("minecraft:egg".into())),
            ("Pos", doubles(&[0.0, 64.0, 0.0])),
            (
                "Item",
                item_to_nbt(&crate::items::ItemStack::new("minecraft:blue_egg", 1)),
            ),
        ]);
        let egg = throwable_from_nbt(&raw, None).unwrap();
        // With or without the tag it was loaded from, which is lost once it moves.
        for raw in [Some(&raw), None] {
            let tag = throwable_to_nbt(&egg, [1; 4], None, raw).unwrap();
            let item = tag.get("Item").and_then(item_from_nbt).unwrap();
            assert_eq!(&*item.item, "minecraft:blue_egg");
        }
        let tag = throwable_to_nbt(&egg, [1; 4], None, Some(&raw)).unwrap();
        assert!(tag.get("LeftOwner").is_none() && tag.get("Owner").is_none());
        assert_eq!(number(tag.get("HasBeenShot")), Some(0));
        let wind = Projectile::new(ProjectileKind::WindCharge, Vec3::ZERO, None);
        assert!(throwable_to_nbt(&wind, [1; 4], None, None).is_none());
        // NoGravity is loaded and saved again (a summoned pearl that floats).
        let floating = Tag::compound([
            ("id", Tag::String("minecraft:ender_pearl".into())),
            ("Pos", doubles(&[0.0, 64.0, 0.0])),
            ("NoGravity", Tag::Byte(1)),
        ]);
        let pearl = throwable_from_nbt(&floating, None).unwrap();
        assert!(pearl.no_gravity);
        let tag = throwable_to_nbt(&pearl, [1; 4], None, None).unwrap();
        assert_eq!(number(tag.get("NoGravity")), Some(1));
        assert!(
            throwable_to_nbt(&egg, [1; 4], None, None)
                .unwrap()
                .get("NoGravity")
                .is_none()
        );
    }
}

#[cfg(test)]
mod neutral_anger_tests {
    use super::*;

    /// #2784: a neutral mob's anger survives the save: the anger ends at an
    /// absolute game time and the target comes back with it
    /// (`NeutralMob.addPersistentAngerSaveData` / `readPersistentAngerSaveData`).
    #[test]
    fn a_neutral_mobs_anger_survives_the_save() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn anger_left_counts_down_to_zero() {
        // The anger that is left when the mob comes back.
        assert_eq!(crate::mobs::anger_left(3000, 1000), 2000);
        assert_eq!(crate::mobs::anger_left(3000, 4000), 0);
    }
}

#[cfg(test)]
mod uuid_bridge_tests {
    use super::*;
    use crate::mobs::{Species, sensor_scan_tests::kind};

    /// A wolf, whose anger is `NeutralMob`'s, so its save carries `angry_at`.
    /// No resources are needed: only the species decides that.
    #[test]
    fn a_mob_anger_target_is_saved_as_its_uuid() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn an_unknown_anger_target_writes_no_angry_at() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn a_player_anger_target_needs_no_map() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
}

#[cfg(test)]
mod warden_anger_tests {
    use super::*;
    use crate::mobs::warden::AngerManagement;
    use std::cell::Cell;

    /// `Warden.addAdditionalSaveData` stores the anger as `AngerManagement`'s
    /// codec does - one `{uuid, anger}` pair per suspect, the ones in the level
    /// by each entity's UUID and then the ones that are away - and the per
    /// suspect anger comes back with the mob.
    #[test]
    fn a_wardens_anger_round_trips_as_suspect_uuid_pairs() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// Anger at a suspect that is still away outlives the session: it decays
    /// while nothing answers for its UUID, and the next save writes the pair
    /// again, so a warden's anger at a mob in an unloaded chunk is not lost by
    /// saving and loading twice.
    #[test]
    fn an_away_suspect_survives_another_save() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// A warden with no suspect writes the codec's default: an `anger` compound
    /// with no `suspects` key, which is what vanilla's `lenientOptionalFieldOf`
    /// leaves out.
    #[test]
    fn a_warden_with_no_anger_writes_no_suspects() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// A suspect whose anger is below zero, or whose UUID is not four ints,
    /// does not decode: the whole list is dropped, as
    /// `lenientOptionalFieldOf` does with a value that fails.
    #[test]
    fn a_suspects_list_that_does_not_decode_is_ignored() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// An away suspect calms down by one an anger tick, like the ones in the
    /// level, and is forgotten at one point - and its UUID keeps the anger until
    /// it does.
    #[test]
    fn an_away_suspect_calms_down_one_a_tick() {
        let mut a = AngerManagement::from_saved([([1, 0, 0, 1], 30), ([2, 0, 0, 2], 1)]);
        for _ in 0..5 {
            a.tick(|_| true, |_| None);
        }
        // Five ticks, five points: the one at one point was dropped on the first.
        assert_eq!(a.unloaded().collect::<Vec<_>>(), vec![([1, 0, 0, 1], 25)]);
        for _ in 0..24 {
            a.tick(|_| true, |_| None);
        }
        assert_eq!(a.unloaded().collect::<Vec<_>>(), vec![([1, 0, 0, 1], 1)]);
        a.tick(|_| true, |_| None);
        assert!(a.unloaded().next().is_none());
        // Nothing in the level sees that anger while it is away.
        assert!(a.suspects().is_empty());
        assert_eq!(a.active_anger(None), 0);
    }

    /// The reconversion: an away suspect's UUID becomes a live `Who` once the
    /// mob is loaded, on the first anger tick that is due a lookup
    /// (`conversionDelay` counts two of them).
    #[test]
    fn an_away_suspect_rejoins_once_its_mob_is_loaded() {
        let uuid = [7, 0, 0, 3];
        let mut a = AngerManagement::from_saved([(uuid, 60)]);
        // The mob is not in the level for the first lookups.
        let loaded = Cell::new(false);
        let by_uuid = |u: [i32; 4]| (loaded.get() && u == uuid).then_some(Who::Mob(3));
        // A fresh anger converts on its first tick, then every second one.
        a.tick(|_| true, by_uuid);
        assert_eq!(a.unloaded().collect::<Vec<_>>(), vec![(uuid, 59)]);
        assert!(a.suspects().is_empty());
        // Now the mob loads. This tick is no lookup one: nothing happens yet.
        loaded.set(true);
        a.tick(|_| true, by_uuid);
        assert_eq!(a.unloaded().collect::<Vec<_>>(), vec![(uuid, 58)]);
        assert!(a.suspects().is_empty());
        // The lookup is due: it rejoins with the anger it kept.
        a.tick(|_| true, by_uuid);
        assert!(a.unloaded().next().is_none());
        assert_eq!(a.suspects(), &[Who::Mob(3)]);
        // 60, less one for each of the three ticks: the two it was away for and
        // the one it rejoined on, which calms it like any suspect in the level.
        assert_eq!(a.anger_of(Who::Mob(3)), 57);
        assert_eq!(a.active_anger(None), 57);
    }

    /// Anger gained at a suspect that has not rejoined yet: vanilla's
    /// `increaseAnger` takes the anger kept under its UUID into the new total
    /// (min(150, 35) + 20 = 55), so the lookup adds it rather than dropping it or
    /// letting it overwrite the new points; one anger tick then calms it to 54.
    #[test]
    fn a_suspect_that_has_not_rejoined_keeps_both_angers() {
        let mut a = AngerManagement::from_saved([([3, 0, 0, 3], 20)]);
        assert_eq!(a.increase(Who::Mob(3), 35), 35);
        a.tick(|_| true, |_| Some(Who::Mob(3)));
        assert_eq!(a.anger_of(Who::Mob(3)), 54);
        assert!(a.unloaded().next().is_none());
    }
}

#[cfg(test)]
mod held_item_name_tests {
    use super::*;

    #[test]
    fn horse_head_and_body_survive_save_load_and_drop_independently() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn a_worn_piece_keeps_its_damage_and_name_through_the_save() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn replacing_a_named_item_does_not_restore_its_old_name_on_save() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
}

#[cfg(test)]
mod shulker_bullet_nbt_tests {
    use super::*;

    /// The level's side of a bullet's `Target`: the id -> UUID map
    /// (`EntityMemory.uuids`), the mobs it currently holds, and the player.
    /// `save_who_uuid` and `bullet_target` are the game layer's two helpers,
    /// kept here as the same two calls so both halves of the bridge are what the
    /// round trips below go through.
    struct Level {
        uuids: HashMap<u32, [i32; 4]>,        player: Option<[i32; 4]>,
    }
    impl Level {
        /// `Game::save_who_uuid`.
        fn save_who_uuid(&self, who: Who) -> Option<[i32; 4]> {
            who_uuid(who, &mut |id| self.uuids.get(&id).copied())
        }
        /// `Game::bullet_target`: the `Who` a saved `Target` names, while that
        /// entity is in the level.
        fn bullet_target(&self, tag: &Tag) -> Option<Who> {
            who_by_uuid(&self.uuids, &self.mobs, self.player, bullet_target(tag)?)
        }
    }
    /// The bullet the tests start from: fired by the player at mob 31 along +X.
    fn homing(target: Option<Who>) -> Projectile {
        Projectile::new(
            ProjectileKind::ShulkerBullet {
                target,
                dir: Some(5),
                steps: 23,
                delta: [0.15, -0.15, 0.075],
                seed: 7,
            },
            Vec3::new(4.5, 80.0, 10.5),
            Some(Who::Player),
        )
    }
    fn bullet_kind(p: &Projectile) -> (Option<Who>, Option<u8>, i32, [f64; 3]) {
        let ProjectileKind::ShulkerBullet {
            target,
            dir,
            steps,
            delta,
            ..
        } = p.kind
        else {
            panic!("not a bullet: {:?}", p.kind);
        };
        (target, dir, steps, delta)
    }

    /// `ShulkerBullet.addAdditionalSaveData` then `readAdditionalSaveData`, both
    /// over the UUID bridge: a bullet in flight keeps the target it homes on, the
    /// step it is on, the steps left and its per-axis target deltas, and the
    /// target comes back as the `Who::Mob` of the shulker it was fired at.
    ///
    /// Horizon wrote none of it and `saved_with_chunk` turned the kind away while
    /// it had a target, so every bullet a shulker fired was dropped by the chunk
    /// save.
    #[test]
    fn a_homing_bullet_round_trips_its_target_step_and_deltas() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `ShulkerBullet.readAdditionalSaveData`: `Dir` goes through
    /// `Direction.from3DDataValue`, which wraps any byte, and `Steps` is read
    /// as saved, negative or not.
    #[test]
    fn a_loaded_dir_wraps_and_steps_keep_their_sign() {
        // `Direction.from3DDataValue(b)` is `BY_3D_DATA[abs(b % 6)]`, and
        // `getIntOr` keeps a negative step count.
        for (dir, steps, expected) in [(7, -3, (1, -3)), (-2, 4, (2, 4)), (5, 0, (5, 0))] {
            let tag = Tag::compound([
                ("id", Tag::String("minecraft:shulker_bullet".into())),
                ("Pos", doubles(&[4.5, 80.0, 10.5])),
                ("Dir", Tag::Byte(dir)),
                ("Steps", Tag::Int(steps)),
            ]);
            let p = spit_or_bullet_from_nbt(&tag, None, None).unwrap();
            let (_, d, s, _) = bullet_kind(&p);
            assert_eq!(
                (d, s),
                (Some(expected.0), expected.1),
                "Dir {dir}, Steps {steps}"
            );
        }
    }

    /// A bullet with no target — a `/summon`ed one, or one loaded without its
    /// target — writes no `Target` key at all, as vanilla's
    /// `if (this.finalTarget != null)` does, and no `Dir` until it has chosen a
    /// step. `Steps` and the deltas are always written, zero or not.
    #[test]
    fn a_bullet_without_a_target_writes_no_target_key() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// The stored `Target` names a mob that is no longer loaded: vanilla keeps
    /// the UUID and re-resolves it every tick, and Horizon's `Who` can only name
    /// an entity in the level, so the bullet comes back with no target and falls
    /// rather than homing on whatever the id now means. Here the id the mob had
    /// is reused by a projectile the same save holds, which is the case a bare
    /// id would get wrong.
    #[test]
    fn a_target_that_is_not_loaded_comes_back_with_none() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// The player is the other `Who` a `Target` can name: the same bridge, the
    /// player's own UUID.
    #[test]
    fn a_target_naming_the_player_comes_back_as_the_player() {
        let player = player_uuid();
        let level = Level {
            uuids: HashMap::new(),
            mobs: Vec::new(),
            player: Some(player),
        };
        let p = homing(Some(Who::Player));
        let target = level.save_who_uuid(Who::Player);
        assert_eq!(target, Some(player));
        let tag = spit_or_bullet_to_nbt(&p, [1; 4], None, target, None).unwrap();
        assert_eq!(level.bullet_target(&tag), Some(Who::Player));
        // With no player in the level the UUID names nobody.
        let empty = Level {
            uuids: HashMap::new(),
            mobs: Vec::new(),
            player: None,
        };
        assert_eq!(empty.bullet_target(&tag), None);
    }
}

#[cfg(test)]
#[path = "spectral_duration_nbt_tests.rs"]
mod spectral_duration_nbt_tests;

#[cfg(test)]
mod mod_thrown_nbt_tests {
    use super::*;

    #[test]
    fn a_mod_thrown_type_saves_its_id_and_item_and_loads_back() {
        crate::projectiles::register_test_types();
        let bolt = crate::projectiles::thrown_type("ruby:bolt").unwrap();
        let mut p = Projectile::new(
            ProjectileKind::Thrown(bolt),
            Vec3::new(1.0, 80.0, 2.0),
            None,
        );
        p.velocity = Vec3::new(0.0, 0.5, 1.0);
        assert!(saved_with_chunk(&p));
        let tag = throwable_to_nbt(&p, [1; 4], None, None).unwrap();
        assert_eq!(super::kind(&tag).as_deref(), Some("ruby:bolt"));
        let item = tag.get("Item").and_then(item_from_nbt).unwrap();
        assert_eq!(&*item.item, "ruby:gem");
        let back = throwable_from_nbt(&tag, None).unwrap();
        assert_eq!((&back.kind, back.velocity), (&p.kind, p.velocity));
        // NoGravity comes back too, as for vanilla's (a summoned bolt that floats).
        p.no_gravity = true;
        let tag = throwable_to_nbt(&p, [1; 4], None, None).unwrap();
        assert!(throwable_from_nbt(&tag, None).unwrap().no_gravity);
        // a type no mod declares now is not a thrown kind.
        assert!(throwable_kind("gone:bolt").is_none());
    }
    fn summoned(id: &str, extra: &[(&str, Tag)]) -> Tag {
        let mut c = vec![
            ("id", Tag::String(id.into())),
            ("Pos", doubles(&[0.5, 110.0, 0.5])),
            ("Motion", doubles(&[0.0, 0.0, 0.0])),
        ];
        c.extend(extra.iter().cloned());
        Tag::compound(c)
    }

    /// Four summoned kinds had no loader and stayed inert (a spectral arrow,
    /// a shulker bullet, a llama's spit and an experience bottle never fell).
    #[test]
    fn summoned_projectiles_of_every_kind_load_and_honour_no_gravity() {
        use crate::entity_rules::EntityRules;
        let flag = ("NoGravity", Tag::Byte(1));
        let arrow = arrow_from_nbt(&summoned("minecraft:spectral_arrow", &[]), None, |_| None)
            .unwrap()
            .0;
        assert!(arrow.is_spectral_arrow() && !arrow.no_gravity);
        let held = arrow_from_nbt(
            &summoned("minecraft:spectral_arrow", std::slice::from_ref(&flag)),
            None,
            |_| None,
        )
        .unwrap()
        .0;
        assert!(held.no_gravity);
        // ...and it is saved as the entity type it is.
        let saved = arrow_to_nbt(&arrow, [1; 4], None, None, None).unwrap();
        assert_eq!(kind(&saved).as_deref(), Some("minecraft:spectral_arrow"));
        let plain = arrow_from_nbt(&summoned("minecraft:arrow", &[]), None, |_| None)
            .unwrap()
            .0;
        let saved = arrow_to_nbt(&plain, [1; 4], None, None, None).unwrap();
        assert_eq!(kind(&saved).as_deref(), Some("minecraft:arrow"));

        for id in ["minecraft:llama_spit", "minecraft:shulker_bullet"] {
            let p = spit_or_bullet_from_nbt(&summoned(id, &[]), None, None).unwrap();
            assert_eq!(p.kind.id(), id);
            assert!(!p.no_gravity && p.position == Vec3::new(0.5, 110.0, 0.5));
            let held =
                spit_or_bullet_from_nbt(&summoned(id, std::slice::from_ref(&flag)), None, None)
                    .unwrap();
            assert!(held.no_gravity);
        }
        assert!(spit_or_bullet_from_nbt(&summoned("minecraft:arrow", &[]), None, None).is_none());
        // a spit and a target-less bullet are saved and selectable (`/data`, `@e`).
        let spit =
            spit_or_bullet_from_nbt(&summoned("minecraft:llama_spit", &[]), None, None).unwrap();
        assert!(saved_with_chunk(&spit));
        let saved = spit_or_bullet_to_nbt(&spit, [1; 4], None, None, None).unwrap();
        assert_eq!(kind(&saved).as_deref(), Some("minecraft:llama_spit"));

        // A summoned experience bottle flies as the bottle item, with a bottle's gravity.
        let bottle =
            throwable_from_nbt(&summoned("minecraft:experience_bottle", &[]), None).unwrap();
        assert_eq!(bottle.kind.id(), "minecraft:experience_bottle");
        assert!(!bottle.no_gravity && (bottle.get_gravity() - 0.07).abs() < 1e-12);
        let held =
            throwable_from_nbt(&summoned("minecraft:experience_bottle", &[flag]), None).unwrap();
        assert!(held.no_gravity && held.get_gravity() == 0.0);
        let saved = throwable_to_nbt(&bottle, [1; 4], None, None).unwrap();
        assert_eq!(kind(&saved).as_deref(), Some("minecraft:experience_bottle"));
        let item = saved.get("Item").and_then(item_from_nbt).unwrap();
        assert_eq!(&*item.item, "minecraft:experience_bottle");
    }

    #[test]
    fn no_gravity_survives_save_and_load_for_orbs() {
        // Entity.saveWithoutId / load: NoGravity round-trips on a kind that
        // fall by their own tick (orb, item, arrow, fireball).
        let mut orb = orbs_from_nbt(&Tag::compound([
            ("Pos", doubles(&[0.0, 10.0, 0.0])),
            ("NoGravity", Tag::Byte(1)),
        ]))
        .remove(0);
        assert!(orb.no_gravity);
        let back = orbs_from_nbt(&orb_to_nbt(&orb, [1; 4])).remove(0);
        assert!(back.no_gravity);
        orb.no_gravity = false;
        assert!(orb_to_nbt(&orb, [1; 4]).get("NoGravity").is_none());
    }
}

#[cfg(test)]
mod equipment_tests;

#[cfg(test)]
mod axolotl_memory_ttl_tests {
    use super::*;

    #[test]
    fn integer_memory_saves_keep_finite_ttl_and_drop_it_after_a_permanent_write() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
}

#[cfg(test)]
mod generic_home_nbt_tests {
    use super::*;
    #[test]
    fn spawned_trader_home_restricts_and_survives_save() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn generic_home_output_uses_model_and_removes_stale_raw_fields() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn negative_home_updates_retain_position_without_replacing_runtime_ai() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn fresh_home_loads_apply_radius_and_position_codec_defaults() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn turtle_and_creaking_keep_their_own_home_position_writer() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
}

#[cfg(test)]
mod piglin_live_anger_memory_tests {
    use super::*;

    #[test]
    fn guarded_event_saves_actual_uuid_and_remaining_long_ttl() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `Piglin.readAdditionalSaveData` reads the brain's `ANGRY_AT` memory
    /// back: the actor's UUID and the ticks left (`ExpirableValue`'s TTL).
    /// Horizon wrote the memory but never read it, so a piglin that a player
    /// had angered came back calm after its chunk reloaded.
    #[test]
    fn a_saved_guarded_anger_loads_its_uuid_and_ticks() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn expired_owned_memory_cannot_resurrect_stale_raw_anger() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn calm_loaded_and_missing_identity_states_are_distinct() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
}

#[cfg(test)]
mod pickup_loot_save_tests {
    use super::*;
    #[test]
    fn pickup_save_uses_live_state_instead_of_raw_and_preserves_overrides() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
}

#[cfg(test)]
mod stand_fall_save_tests {
    use super::*;

    /// `Entity.load` reads `Fire` with `getShortOr`: an int tag is narrowed
    /// (`(short)(value & 65535)`), so 40000 is -25536 and the stand or mob
    /// does not burn.
    #[test]
    fn a_stand_and_a_mob_narrow_a_loaded_fire_to_a_short() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `LargeFireball.readAdditionalSaveData` reads `ExplosionPower` with
    /// `getByteOr`: an int tag is narrowed (`(byte)(value & 255)`), so 300 is
    /// 44 and 200 is -56.
    #[test]
    fn a_fireball_narrows_its_explosion_power_to_a_byte() {
        for (saved, expected) in [(Tag::Int(300), 44), (Tag::Int(200), -56), (Tag::Byte(4), 4)] {
            let input = Tag::compound([
                ("id", string("minecraft:fireball")),
                ("Pos", doubles(&[4.5, 205.0, 4.5])),
                ("ExplosionPower", saved),
            ]);
            let fireball = fireball_from_nbt(&input, None).unwrap();
            assert!(
                matches!(fireball.kind, ProjectileKind::LargeFireball { power } if power == expected),
                "{expected}"
            );
        }
    }

    #[test]
    fn decoded_stand_fall_distance_survives_save_without_raw_or_with_stale_raw() {
        let stale = Tag::compound([("fall_distance", Tag::Double(99.0_f64.to_bits()))]);
        for no_gravity in [false, true] {
            for distance in [0.0_f64, 1.25, 17.5] {
                let input = Tag::compound([
                    ("Pos", doubles(&[4.5, 205.0, 4.5])),
                    ("NoGravity", Tag::Byte(i8::from(no_gravity))),
                    ("fall_distance", Tag::Double(distance.to_bits())),
                ]);
                let stand = armor_stand_from_nbt(&input, 1).unwrap();
                for raw in [None, Some(&stale)] {
                    let saved = armor_stand_to_nbt(&stand, [28, 0, 0, 1], raw);
                    assert_eq!(
                        saved.get("fall_distance"),
                        Some(&Tag::Double(distance.to_bits())),
                        "distance={distance} no_gravity={no_gravity} raw={}",
                        raw.is_some()
                    );
                    assert_eq!(saved.get("Pos"), input.get("Pos"));
                    assert_eq!(stand.no_gravity, no_gravity);
                }
            }
        }
    }

    /// `Entity.saveWithoutId`'s common fields are filled in when a raw compound
    /// (a marker's) lacks them, and explicit values win.
    #[test]
    fn marker_common_fields_default_and_keep_explicit_values() {
        let mut tag = Tag::compound([("Air", Tag::Short(10)), ("OnGround", Tag::Byte(1))]);
        fill_common_defaults(&mut tag);
        assert_eq!(tag.get("Air"), Some(&Tag::Short(10)));
        assert_eq!(tag.get("OnGround"), Some(&Tag::Byte(1)));
        assert_eq!(tag.get("Invulnerable"), Some(&Tag::Byte(0)));
        assert_eq!(tag.get("PortalCooldown"), Some(&Tag::Int(0)));
        assert_eq!(
            tag.get("fall_distance"),
            Some(&Tag::Double(0.0f64.to_bits()))
        );
        assert_eq!(tag.get("Motion"), Some(&doubles(&[0.0, 0.0, 0.0])));
        // Anything but a compound is left untouched.
        let mut plain = Tag::String("x".into());
        fill_common_defaults(&mut plain);
        assert_eq!(plain, Tag::String("x".into()));
    }
}

#[cfg(test)]
mod animal_love_cause_tests {
    use super::*;

    #[test]
    fn animal_save_uses_the_parsed_love_cause_instead_of_stale_raw() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
}

#[cfg(test)]
mod carrier_inventory_tests {
    use super::*;

    #[test]
    fn carrier_inventory_is_adopted_and_saved_from_the_model() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
}

/// Added controls (H192): the sulfur cube's saved archetype attributes are
/// its map's base and permanent modifiers, replacing a stale entry of the
/// same id once, never its transient modifiers.
#[cfg(test)]
mod sulfur_attribute_save_tests {
    use super::*;
    #[test]
    fn the_save_writes_base_and_permanent_modifiers_and_replaces_stale_entries() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn another_species_map_is_not_saved_by_the_bridge() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
}

#[cfg(test)]
mod mob_hidden_speed_tests {
    use super::*;
    #[test]
    fn loaded_speed_fallback_survives_a_no_raw_save() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
    // Added after the frozen old proof: modeled mutation and stale raw controls.
    #[test]
    fn fallback_model_wins_over_stale_raw_and_clears_without_resurrection() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
}

#[cfg(test)]
mod mob_effect_amplifier_codec {
    use super::*;

    #[test]
    fn decoded_numeric_amplifiers_roundtrip_without_raw_input() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `MobEffectInstance.Details.CODEC`: `show_icon` is its own field, and a saved effect
    /// without one takes `show_particles`.
    #[test]
    fn a_mobs_show_icon_roundtrips_apart_from_show_particles() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn modeled_amplifier_mutation_overrides_stale_raw_and_omits_zero() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
}

#[cfg(test)]
mod camel_inherited_saved_tests {
    use super::*;
    #[test]
    fn camel_inherited_fields_decode_and_save_without_raw_or_with_stale_raw() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
    #[test]
    fn camel_signed_state_and_mutations_override_contradictory_raw() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn camel_saved_carry_leaves_the_seven_equine_paths_unchanged() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
}

#[cfg(test)]
mod equine_temper_preservation_tests {
    use super::*;
    #[test]
    fn temper_preservation_decodes_and_saves_without_raw_input() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    #[test]
    fn equine_and_ravager_attributes_parity() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
}

#[cfg(test)]
mod sulfur_bucket_content_tests {
    use super::*;
    /// `MobBucketItem.spawn` → `SulfurCube.applyImplicitComponent`: a cube
    /// released from its bucket wears the block the bucket recorded
    /// (`minecraft:sulfur_cube_content`) as its body again (#17990).
    #[test]
    fn a_released_sulfur_cube_keeps_the_block_its_bucket_carried() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
}

#[cfg(test)]
mod loaded_rotation_tests {
    use super::*;

    fn tag(yaw: f32, pitch: f32) -> Tag {
        Tag::compound([(
            "Rotation",
            Tag::List(vec![Tag::Float(yaw.to_bits()), Tag::Float(pitch.to_bits())]),
        )])
    }

    /// vanilla 26.3's values (the parity rows): yaw `% 360`, pitch `% 360` clamped to +-90.
    #[test]
    fn a_saved_rotation_loads_like_vanilla() {
        assert_eq!(loaded_rotation(&tag(400.0, 100.0)), [40.0, 90.0]);
        assert_eq!(loaded_rotation(&tag(270.0, 370.0)), [270.0, 10.0]);
        assert_eq!(loaded_rotation(&tag(-190.0, -100.0)), [-190.0, -90.0]);
        assert_eq!(loaded_rotation(&tag(180.0, 45.0)), [180.0, 45.0]);
        assert_eq!(loaded_rotation(&tag(73.0, -24.0)), [73.0, -24.0]);
        // a non-finite component is discarded: it loads as 0 and a save writes 0
        assert_eq!(loaded_rotation(&tag(f32::NAN, 20.0)), [0.0, 20.0]);
        assert_eq!(loaded_rotation(&tag(30.0, f32::INFINITY)), [30.0, 0.0]);
        // a save writes the raw value (a painting's yaw 270 from its facing) and reads it back as it is
        assert_eq!(rotation(&tag(270.0, 10.0)), [270.0, 10.0]);
        assert_eq!(
            loaded_rotation(&Tag::compound(Vec::<(&str, Tag)>::new())),
            [0.0, 0.0]
        );
    }
}
