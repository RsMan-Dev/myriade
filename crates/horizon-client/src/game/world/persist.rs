//! Saving and loading the game state that lives outside chunk blocks:
//! entities (`entities/*.mca`, per chunk like vanilla's `EntityStorage`) and
//! scheduled block/fluid ticks (chunk `block_ticks`/`fluid_ticks`).
#[path = "persist/display.rs"]
mod display;
#[path = "persist/entity_uuids.rs"]
mod entity_uuids;
pub(super) use entity_uuids::EntityUuids;
#[path = "persist/loaded_pass.rs"]
mod loaded_pass;
#[path = "persist/object_memory.rs"]
mod object_memory;
#[path = "persist/unload_sweep.rs"]
mod unload_sweep;

#[cfg(test)]
#[path = "persist/completion_tests.rs"]
mod completion_tests;
#[path = "persist/passenger_graph.rs"]
mod passenger_graph;
#[path = "persist/hurt_by_refs.rs"]
mod hurt_by_refs;
#[cfg(test)]
#[path = "persist/lookup_order_tests.rs"]
mod lookup_order_tests;

use super::Game;
use horizon_client::{
    Result,
    chunk::Chunk,
    entity_nbt,
    mobs::{Mob, Who},
    random_source,
    save::{ChunkExtras, random_uuid, text, uuid_from_text, uuid_text},
    scoreboard::Scoreboard,
};
use horizon_core::{
    geometry::{Direction, Vec3},
    position::{BlockPos, ChunkPos},
    random::RandomSource,
    ticks::ChunkTicks,
};
use horizon_mod_runtime::attachments::{ATTACHMENTS_KEY, attachment_compound};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use {
    horizon_core::nbt::Tag,
    horizon_world::{level::MovingPiston, registry::BlockStateId},
};

/// One place in a vehicle's `Passengers` list: a live object ID (possibly
/// refused level residency), or a compound retained without a live object.
#[derive(Clone, Debug)]
pub(super) enum PassengerSlot {
    Live(u32),
    Saved(Tag),
}

/// An entity object retained by passenger links but refused level residency.
/// Queries and tracking never visit this store; internal topology and saving do.
/// Boats can also be retained as refused transferred riders.
pub(super) enum OffLevel {
    Mob(Mob),
    Boat(super::boats::Boat),
}

/// Formerly resident vehicles kept solely by surviving passenger edges.
pub(super) enum RemovedVehicle {
    Boat(super::boats::Boat),
    Cart(super::minecarts::Minecart),
}

impl RemovedVehicle {
    pub(super) fn position(&self) -> Vec3 {
        match self {
            Self::Boat(boat) => boat.position,
            Self::Cart(cart) => cart.position,
        }
    }

    pub(super) fn retire_attachment_registration(&self) {
        match self {
            Self::Boat(boat) => {
                boat.attachment_lifetime.retire();
            }
            Self::Cart(cart) => {
                cart.attachment_lifetime.retire();
            }
        }
    }

    pub(super) fn kind(&self) -> String {
        match self {
            Self::Boat(boat) => boat.entity_type(),
            Self::Cart(cart) => cart.kind.id().into(),
        }
    }
}

impl OffLevel {
    pub(super) fn as_mob(&self) -> Option<&Mob> {
        match self {
            Self::Mob(mob) => Some(mob),
            Self::Boat(_) => None,
        }
    }

    fn as_mob_mut(&mut self) -> Option<&mut Mob> {
        match self {
            Self::Mob(mob) => Some(mob),
            Self::Boat(_) => None,
        }
    }

    fn as_boat(&self) -> Option<&super::boats::Boat> {
        match self {
            Self::Boat(boat) => Some(boat),
            Self::Mob(_) => None,
        }
    }

    /// Only a retained mob can be removed while dependents keep it: a boat or
    /// cart is never selectable, so it leaves only with its passenger graph.
    pub(super) fn is_removed(&self) -> bool {
        match self {
            Self::Mob(mob) => mob.removed,
            Self::Boat(_) => false,
        }
    }
}

/// The UUID an entity loads with: the saved one, else the one its constructor made.
/// `Entity.<init>` always draws it from the entity's own random, so the draw is taken
/// whether or not the tag has a UUID (as `load_mob` does).
fn with_constructor_uuid(saved: Option<[i32; 4]>) -> Option<[i32; 4]> {
    let made = horizon_client::save::insecure_uuid(&mut random_source::entity());
    Some(saved.unwrap_or(made))
}
/// `Entity.saveWithoutId`: materialize the mandatory fields of an entity
/// whose simulation state is still represented by its loaded compound.
pub(super) fn write_static_entity_fields(tag: &mut Tag) {
    use horizon_client::{nbt_path::int_value, save::float};

    let int = |key, default| tag.get(key).and_then(int_value).unwrap_or(default);
    let motion = entity_nbt::loaded_motion(tag);
    let double = |value: f64| Tag::Double(if value == 0.0 { 0 } else { value.to_bits() });
    let fall_distance = float(tag.get("fall_distance")).unwrap_or(0.0);
    let values = [
        (
            "Motion",
            Tag::List([motion.x, motion.y, motion.z].map(double).to_vec()),
        ),
        ("fall_distance", double(fall_distance)),
        ("Fire", Tag::Short(int("Fire", 0) as i16)),
        ("Air", Tag::Short(int("Air", 300) as i16)),
        (
            "OnGround",
            Tag::Byte(i8::from(entity_nbt::flag(tag, "OnGround"))),
        ),
        (
            "Invulnerable",
            Tag::Byte(i8::from(entity_nbt::flag(tag, "Invulnerable"))),
        ),
        ("PortalCooldown", Tag::Int(int("PortalCooldown", 0))),
    ];
    if let Tag::Compound(fields) = tag {
        for (key, value) in values {
            fields.insert(key.into(), value);
        }
    }
}

/// What Horizon remembers about loaded entities to save them faithfully.
#[derive(Default)]
pub struct EntityMemory {
    /// UUIDs of entities by Horizon entity ID: the one each was loaded with,
    /// or made up once something needed it (a save, a native mod's attachment).
    pub(super) uuids: EntityUuids,
    /// The compound each mob or item entity was loaded with.
    pub(super) raw: HashMap<u32, Tag>,
    /// Full rejected passenger objects, not resident entities or frozen NBT copies.
    /// Kept with the level while any live object graph still refers to them.
    pub(super) off_level: HashMap<u32, OffLevel>,
    /// Passengers that ride a vehicle, in load order: live entity IDs or unspawned tags.
    /// (`passenger_graph.rs`; mob riders also keep `extra.mount`, which stays authoritative
    /// for the mob objects above.)
    pub(super) passenger_slots: HashMap<u32, Vec<PassengerSlot>>,
    /// Removed boat/cart objects are never part of level residency.
    pub(super) removed_vehicles: HashMap<u32, RemovedVehicle>,
    /// Destruction callbacks already consumed for geometry retained after removal.
    score_destroyed: HashSet<u32>,
    /// Mount notifications currently on the callback stack, keyed by object IDs.
    pub(super) mount_notifications: HashSet<(u32, u32, bool)>,
    /// Deferred native removals already notified their passenger edges.
    pub(super) notified_vehicle_removals: HashSet<u32>,
    /// Vehicle destruction is logically visible before its passenger callbacks.
    pub(super) destroying_vehicles: HashSet<super::commands::Target>,
    /// Synched entity-data entries dirtied by forced writes since the last sync.
    pub(super) forced_meta: HashMap<u32, [u64; 4]>,
    /// `Entity.noPhysics` a native mod wrote for a kind whose own struct keeps
    /// no field for it (the dragon, an area-effect cloud, an ominous item
    /// spawner, a shulker bullet): the value replaces the kind's constructor
    /// default and is never saved.
    pub(super) no_physics: HashMap<u32, bool>,
    /// Entities of types Horizon does not simulate, kept per chunk and
    /// written back unchanged.
    unknown: HashMap<(i32, i32), Vec<Tag>>,
    /// The compound of orbs, arrows, falling blocks, tridents, crystals and the
    /// dragon as they were loaded, found again by type and position (one that
    /// has moved since loading is saved afresh); their UUIDs are in `uuids`.
    anon: HashMap<(String, [u64; 3]), (Vec3, Tag)>,
    /// UUIDs of the vehicles `restore_saved_vehicle` brought in from the
    /// player's file: their chunk copies must not spawn a second one. Cleared
    /// with the rest when a world closes.
    restored: HashSet<[i32; 4]>,
    /// Entities a native mod kept out of the level (`native_entity_join_level`),
    /// counted so `/summon` can fail as vanilla's refused add does.
    pub(super) joins_canceled: u32,
    /// The latest insertion considered for `native_entity_join_level`.
    pub(super) joined_seq: horizon_client::join_seq::JoinSeq,
    /// The entities mods were told of and that have not left since: id (in
    /// increasing order), type, and whether the last leave sweep saw it.
    /// Only filled while a mod is loaded.
    pub(super) live: Vec<(u32, String, bool)>,
    /// Entities a native mod removed (`entity_discard`): gone for every
    /// lookup from the call on, taken out of their lists at the next sweep of
    /// departed entities. Mobs go at once and never wait here.
    pub(super) removals: Vec<super::commands::Target>,
}

/// Runtime ownership paired with serialized nodes by one object walk. UUIDs
/// are admission identities, never keys for matching distinct object states.
#[derive(Debug)]
pub(super) struct TransferredVehicleTree {
    source_id: u32,
    kind: VehicleTreeKind,
    name: String,
    state: super::entity_initialization::TransferredEntityState,
    /// `None` is an unspawned saved slot, at exactly the same NBT child index.
    passengers: Vec<Option<TransferredVehicleTree>>,
}

pub(super) struct RestoredVehicleTree {
    pub(super) root: u32,
    pub(super) by_source: HashMap<u32, u32>,
}

#[derive(Clone, Copy, Debug)]
enum VehicleTreeKind {
    Mob,
    Boat,
    /// Any entity the ECS level holds (mob, item, cart, arrow): it comes back through `ecs_load`.
    Ecs,
    Other,
}

/// The boundary has paired each modeled NBT node with exactly one captured
/// object state before any destination object or attachment map is installed.
struct PreparedVehicleTransfer {
    source_id: u32,
    tag: Tag,
    kind: VehicleTreeKind,
    name: String,
    state: super::entity_initialization::TransferredEntityState,
    passengers: Vec<PreparedVehiclePassenger>,
}

enum PreparedVehiclePassenger {
    Object(PreparedVehicleTransfer),
    Saved(Tag),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MobObject {
    Resident(usize),
    OffLevel(u32),
}

impl Game<'_> {}

impl EntityMemory {
    pub(super) fn force_meta(&mut self, id: u32, index: u8) {
        self.forced_meta.entry(id).or_default()[usize::from(index) / 64] |= 1 << (index % 64);
    }
}
impl EntityMemory {
    /// The UUID entity `id` was loaded or saved with: what a save writes for a
    /// field naming that entity (`entity_nbt::mob_to_nbt`'s `uuid_of`). `None`
    /// for one that has none yet, so no reference is written rather than a
    /// made-up UUID nothing would answer for.
    pub(super) fn uuid_of(&self, id: u32) -> Option<[i32; 4]> {
        self.uuids.get(&id).copied()
    }
}

/// The key that marks a compound queued with the chunk loads as a fresh add
/// (`PersistentEntitySectionManager.addNewEntity`: `/summon`, a portal
/// arrival), not an entity loaded from disk. Taken off before the compound
/// is kept or saved.
const FRESH_ADD: &str = "horizon:fresh_add";

/// The kinds `load_unlisted` loads, before `load_listed` sees them.
const UNLISTED_KINDS: &[&str] = &[
    "minecraft:area_effect_cloud",
    "minecraft:evoker_fangs",
    "minecraft:ominous_item_spawner",
    "minecraft:ender_dragon",
    "minecraft:end_crystal",
    "minecraft:item_frame",
    "minecraft:glow_item_frame",
    "minecraft:cushion",
    "minecraft:painting",
];

/// Queue `tag` with the chunk loads as a fresh add.
pub(super) fn mark_fresh(tag: &mut Tag) {
    if let Tag::Compound(c) = tag {
        c.insert(FRESH_ADD.into(), Tag::Byte(1));
    }
}

/// The key that marks a compound queued with the chunk loads as an entity
/// moving within its level (an End gateway: `Entity.teleport` takes
/// `teleportSameDimension`, which moves the entity without removing it), so
/// it doesn't join the level again. Taken off like [`FRESH_ADD`].
const SAME_LEVEL_MOVE: &str = "horizon:same_level_move";

/// Whether `tag` was queued as a move within its level; the mark comes off.
fn take_moved(tag: &mut Tag) -> bool {
    match tag {
        Tag::Compound(c) => c.remove(&SAME_LEVEL_MOVE.into()).is_some(),
        _ => false,
    }
}

/// Whether `tag` was queued as a fresh add; the mark comes off.
fn take_fresh(tag: &mut Tag) -> bool {
    match tag {
        Tag::Compound(c) => c.remove(&FRESH_ADD.into()).is_some(),
        _ => false,
    }
}

fn anon_key(kind: &str, p: Vec3) -> (String, [u64; 3]) {
    (
        kind.to_owned(),
        [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()],
    )
}
/// The loaded compound underneath the fields Horizon models: what the entity was
/// loaded with, with every key of `modelled` replacing its loaded value.
pub(super) fn with_raw(mut modelled: Tag, raw: Option<&Tag>) -> Tag {
    if let (Some(Tag::Compound(raw)), Tag::Compound(fields)) = (raw, &modelled) {
        let mut out = raw.clone();
        for (key, value) in fields {
            // Defaults Horizon invents for fields it does not simulate give way to the save.
            let invented = matches!(
                key.to_string_lossy().as_str(),
                "Fire" | "Air" | "Invulnerable" | "PortalCooldown" | "HasBeenShot"
            );
            if !(invented && out.contains_key(key)) {
                out.insert(key.clone(), value.clone());
            }
        }
        modelled = Tag::Compound(out);
    }
    // The overlay may restore raw bytes for fields whose false value the
    // live writer omitted. Entity.saveWithoutId always writes booleans.
    let on_ground = entity_nbt::flag(&modelled, "OnGround");
    let no_gravity = entity_nbt::flag(&modelled, "NoGravity");
    entity_nbt::write_saved_flags(&mut modelled, on_ground, no_gravity);
    modelled
}

fn read_attachments(
    tag: &Tag,
    id: u32,
    maps: &mut HashMap<
        u32,
        horizon_mod_runtime::attachments::Attachments,
        impl std::hash::BuildHasher,
    >,
    live: bool,
) {
    if let Ok(Some(data)) = attachment_compound(tag.get(ATTACHMENTS_KEY)) {
        match maps.get_mut(&id) {
            // Made by `entity_constructing` listeners just now: saved values win.
            Some(map) if !live => map.read_saved_over(data.clone()),
            Some(_) => {}
            None => {
                maps.insert(
                    id,
                    horizon_mod_runtime::attachments::Attachments::from_nbt(data.clone()),
                );
            }
        }
    }
}

/// Overlay one modeled object's contents. Raw passengers retain their own payload.
pub(super) fn write_attachments(
    tag: &mut Tag,
    id: u32,
    maps: &HashMap<u32, horizon_mod_runtime::attachments::Attachments, impl std::hash::BuildHasher>,
) {
    if let Tag::Compound(root) = tag
        && let Some(map) = maps.get(&id)
        && attachment_compound(root.get(&ATTACHMENTS_KEY.into())).is_ok()
    {
        let data = map.to_nbt();
        if data.is_empty() {
            root.remove(&ATTACHMENTS_KEY.into());
        } else {
            root.insert(ATTACHMENTS_KEY.into(), Tag::Compound(data));
        }
    }
}

pub(in crate::game) fn block_entity_pos(tag: &Tag) -> [i32; 3] {
    [tag.get("x"), tag.get("y"), tag.get("z")]
        .map(|v| horizon_client::save::number(v).unwrap_or(0) as i32)
}
/// `EntitySectionStorage.getExistingSectionsInChunk`: a chunk's sections in
/// packed `SectionPos` order, whose low 20 bits are the section Y: section 0
/// upward, then the negative sections. A record without a position goes last.
fn section_order(tag: &Tag) -> u32 {
    entity_nbt::position(tag).map_or(u32::MAX, |at| {
        ((at.y.floor() as i32 >> 4) as u32) & 0xF_FFFF
    })
}

fn chunk_of(p: Vec3) -> (i32, i32) {
    ((p.x.floor() as i32) >> 4, (p.z.floor() as i32) >> 4)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ItemDisplayTransformation {
    translation: [f32; 3],
    left_rotation: [f32; 4],
    scale: [f32; 3],
    right_rotation: [f32; 4],
}

impl From<mod_api::Transformation> for ItemDisplayTransformation {
    fn from(value: mod_api::Transformation) -> Self {
        Self {
            translation: value.translation,
            left_rotation: value.left_rotation,
            scale: value.scale,
            right_rotation: value.right_rotation,
        }
    }
}

impl ItemDisplayTransformation {
    pub(super) fn from_tag(tag: Option<&Tag>) -> Option<Self> {
        let (translation, left_rotation, scale, right_rotation) = tag
            .and_then(horizon_client::text_display::transformation)
            .unwrap_or((
                [0.0; 3],
                [0.0, 0.0, 0.0, 1.0],
                [1.0; 3],
                [0.0, 0.0, 0.0, 1.0],
            ));
        Some(Self {
            translation,
            left_rotation,
            scale,
            right_rotation,
        })
    }

    pub(super) fn to_tag(self) -> Tag {
        transformation_tag(
            self.translation,
            self.left_rotation,
            self.scale,
            self.right_rotation,
        )
    }
}

/// The transformation-only portion of Display.addAdditionalSaveData and the native writer.
fn transformation_tag(
    translation: [f32; 3],
    left_rotation: [f32; 4],
    scale: [f32; 3],
    right_rotation: [f32; 4],
) -> Tag {
    let floats = |v: &[f32]| {
        Tag::List(
            v.iter()
                .map(|x| Tag::Float(if *x == 0.0 { 0 } else { x.to_bits() }))
                .collect(),
        )
    };
    let mut t = horizon_core::nbt::Compound::new();
    t.insert("translation".into(), floats(&translation));
    t.insert("left_rotation".into(), floats(&left_rotation));
    t.insert("scale".into(), floats(&scale));
    t.insert("right_rotation".into(), floats(&right_rotation));
    Tag::Compound(t)
}

impl Game<'_> {
    /// Saved ticks and the save time for a chunk about to be written.
    pub(super) fn attach_save_data(&self, chunk: &mut Chunk) {
        let pos = ChunkPos::new(chunk.x, chunk.z);
        let time = self.world.game_time;
        let mut extras: ChunkExtras = chunk.saved.as_deref().cloned().unwrap_or_default();
        extras.block_entity_attachments = self
            .world
            .block_entities
            .chunk_attachments(chunk.x, chunk.z);
        // ComparatorBlockEntity.saveAdditional: the output belongs to the
        // circuit, not the inventory map. Overlay only that field on the raw
        // compound so components and other saved data survive.
        for (p, output) in &self.block_ticks.comparator_outputs {
            if (p.x >> 4, p.z >> 4) != (chunk.x, chunk.z)
                || self
                    .res
                    .facts()
                    .get(chunk.block(p.x & 15, p.y, p.z & 15))
                    .name
                    .as_ref()
                    != "minecraft:comparator"
            {
                continue;
            }
            let at = [p.x, p.y, p.z];
            let index = extras
                .block_entities
                .iter()
                .position(|t| block_entity_pos(t) == at);
            let tag = match index {
                Some(i) => &mut extras.block_entities[i],
                None => {
                    extras.block_entities.push(Tag::compound([
                        ("id", Tag::String("minecraft:comparator".into())),
                        ("x", Tag::Int(p.x)),
                        ("y", Tag::Int(p.y)),
                        ("z", Tag::Int(p.z)),
                    ]));
                    extras.block_entities.last_mut().unwrap()
                }
            };
            if text(tag.get("id")).as_deref() != Some("minecraft:comparator") {
                *tag = Tag::compound([
                    ("id", Tag::String("minecraft:comparator".into())),
                    ("x", Tag::Int(p.x)),
                    ("y", Tag::Int(p.y)),
                    ("z", Tag::Int(p.z)),
                ]);
            }
            if let Tag::Compound(c) = tag {
                c.insert("OutputSignal".into(), Tag::Int(*output));
            }
        }
        // HopperBlockEntity.saveAdditional: its inventory is modelled, but its
        // transfer phase lives in Game. The snapshot's source overlay preserves
        // the remaining NBT without changing the live source compound.
        for (p, state) in &self.hoppers {
            if (p.0 >> 4, p.2 >> 4) != (chunk.x, chunk.z)
                || self
                    .res
                    .facts()
                    .get(chunk.block(p.0 & 15, p.1, p.2 & 15))
                    .name
                    .as_ref()
                    != "minecraft:hopper"
            {
                continue;
            }
            let index = extras.block_entity_sources.iter().position(|(q, _)| q == p);
            let tag = match index {
                Some(i) => &mut extras.block_entity_sources[i].1,
                None => {
                    extras.block_entity_sources.push((
                        *p,
                        Tag::compound([
                            ("id", Tag::String("minecraft:hopper".into())),
                            ("x", Tag::Int(p.0)),
                            ("y", Tag::Int(p.1)),
                            ("z", Tag::Int(p.2)),
                        ]),
                    ));
                    &mut extras.block_entity_sources.last_mut().unwrap().1
                }
            };
            if text(tag.get("id")).as_deref() != Some("minecraft:hopper") {
                *tag = Tag::compound([
                    ("id", Tag::String("minecraft:hopper".into())),
                    ("x", Tag::Int(p.0)),
                    ("y", Tag::Int(p.1)),
                    ("z", Tag::Int(p.2)),
                ]);
            }
            if let Tag::Compound(c) = tag {
                c.insert("TransferCooldown".into(), Tag::Int(state.cooldown));
            }
        }
        // A chunk the scheduler hasn't registered yet (one loaded and
        // unloaded again before the next tick) still holds its saved ticks:
        // `register_chunk_ticks` empties that list once it takes them.
        if let Some(t) = self.block_ticks.blocks.chunk(pos) {
            extras.block_ticks = t.pack(time);
        }
        if let Some(t) = self.block_ticks.fluids.chunk(pos) {
            extras.fluid_ticks = t.pack(time);
        }
        extras.last_update = time;
        // PistonMovingBlockEntity.saveAdditional: a block caught mid-move is
        // saved with its chunk (arismoko/horizon#3310).
        for (p, piston) in &self.block_ticks.moving_pistons {
            if (p.x >> 4, p.z >> 4) != (chunk.x, chunk.z) {
                continue;
            }
            extras
                .block_entities
                .retain(|tag| block_entity_pos(tag) != [p.x, p.y, p.z]);
            extras
                .block_entities
                .push(self.saved_moving_piston(*p, piston));
        }
        // TheEndGatewayBlockEntity saves its Age (the server copy's).
        for tag in &mut extras.block_entities {
            let pos = [tag.get("x"), tag.get("y"), tag.get("z")]
                .map(|v| horizon_client::save::number(v).unwrap_or(0) as i32);
            if let Some(g) = self.be_anims.gateway_servers.get(&pos)
                && let Some(age) = tag.get_mut("Age")
            {
                *age = horizon_core::nbt::Tag::Long(g.age);
            }
        }
        if let Some(t) = self.world.inhabited.get(&(chunk.x, chunk.z)) {
            extras
                .root
                .insert("InhabitedTime".into(), horizon_core::nbt::Tag::Long(*t));
        }
        chunk.saved = Some(Arc::new(extras));
    }
    /// Register a newly resident chunk with the tick scheduler, with the
    /// block and fluid ticks it was saved with (`LevelChunk.registerTickContainerInLevel`
    /// at FULL). They are unpacked when the chunk starts ticking
    /// (`start_ticking_chunks`).
    pub(super) fn register_chunk_ticks(&mut self, key: (i32, i32)) {
        let pos = ChunkPos::new(key.0, key.1);
        let saved = self.world.chunks.get(&key).and_then(|c| c.saved.clone());
        // Load before the first block-entity tick, including while ticks are
        // frozen. The scheduler's maps are authoritative from this point on.
        if let Some(saved) = saved.as_deref() {
            for tag in &saved.block_entities {
                let p = block_entity_pos(tag);
                if text(tag.get("id")).as_deref() == Some("minecraft:comparator")
                    && self.data_block_name(p) == "minecraft:comparator"
                    && let Some(tag) = horizon_client::block_entity_nbt::canonical(tag)
                    && let Some(Tag::Int(output)) = tag.get("OutputSignal")
                {
                    self.block_ticks
                        .comparator_outputs
                        .insert(BlockPos::new(p[0], p[1], p[2]), *output);
                }
            }
            for (p, tag) in &saved.block_entity_sources {
                if text(tag.get("id")).as_deref() == Some("minecraft:hopper")
                    && self.data_block_name([p.0, p.1, p.2]) == "minecraft:hopper"
                {
                    self.hoppers
                        .insert(*p, super::machines::HopperState::load(tag));
                }
            }
        }
        // ChunkAccess.inhabitedTime as saved.
        if let Some(Tag::Long(t)) = saved
            .as_deref()
            .and_then(|s| s.root.get(&"InhabitedTime".into()))
        {
            self.world.inhabited.insert(key, *t);
        }
        let (blocks, fluids) = saved.as_deref().map_or((Vec::new(), Vec::new()), |s| {
            (s.block_ticks.clone(), s.fluid_ticks.clone())
        });
        let loaded = !blocks.is_empty() || !fluids.is_empty();
        if self
            .block_ticks
            .add_chunk(
                pos,
                ChunkTicks::from_saved(blocks),
                ChunkTicks::from_saved(fluids),
            )
            .is_ok()
            && loaded
        {
            self.tick_chunks.wait(key);
        }
        // The saved list is consumed: later saves take the scheduler's ticks.
        if loaded && let Some(c) = self.world.chunks.get_mut(&key) {
            let c = Arc::make_mut(c);
            if let Some(s) = c.saved.as_mut() {
                let s = Arc::make_mut(s);
                s.block_ticks.clear();
                s.fluid_ticks.clear();
            }
        }
        // PistonMovingBlockEntity.loadAdditional: blocks saved mid-move carry
        // on moving; later saves take them from the level (arismoko/horizon#3310).
        let pistons: Vec<_> = saved.as_deref().map_or_else(Vec::new, |s| {
            s.block_entities
                .iter()
                .filter_map(|tag| self.moving_piston_from_nbt(tag))
                .collect()
        });
        if pistons.is_empty() {
            return;
        }
        for &(pos, piston) in &pistons {
            self.restore_moving_piston(pos, piston, saved.as_deref());
        }
        if let Some(c) = self.world.chunks.get_mut(&key) {
            let c = Arc::make_mut(c);
            if let Some(s) = c.saved.as_mut() {
                Arc::make_mut(s).block_entities.retain(|tag| {
                    let at = block_entity_pos(tag);
                    !pistons.iter().any(|(p, _)| [p.x, p.y, p.z] == at)
                });
            }
        }
    }
    /// `PistonMovingBlockEntity.loadAdditional`, for a `minecraft:piston`
    /// entity whose block is still a moving piston.
    fn moving_piston_from_nbt(&self, tag: &Tag) -> Option<(BlockPos, MovingPiston)> {
        use horizon_client::save::{float, number, text};
        if text(tag.get("id")).as_deref() != Some("minecraft:piston") {
            return None;
        }
        let [x, y, z] = block_entity_pos(tag);
        let here = self.world.block(x, y, z);
        if &*self.res.facts().get(here).name != "minecraft:moving_piston" {
            return None;
        }
        let moved_state = tag
            .get("blockState")
            .and_then(|t| self.state_from_nbt(t))
            .or_else(|| self.pack.assets.registry.default_state("minecraft:air"))?;
        let direction = usize::try_from(number(tag.get("facing")).unwrap_or(0))
            .ok()
            .and_then(|i| Direction::ALL.get(i).copied())
            .unwrap_or(Direction::Down);
        let progress = float(tag.get("progress")).unwrap_or(0.0) as f32;
        let flag = |k: &str| number(tag.get(k)).unwrap_or(0) != 0;
        Some((
            BlockPos::new(x, y, z),
            MovingPiston {
                moved_state,
                direction,
                extending: flag("extending"),
                source: flag("source"),
                progress,
                progress_o: progress,
                last_ticked: 0,
                death_ticks: 0,
                id: 0,
            },
        ))
    }
    /// The kinds `entity_lists!` does not hold: `Err` hands the compound back
    /// when `kind` is not one of them.
    pub(super) fn load_unlisted(
        &mut self,
        key: ChunkKey,
        kind: &str,
        uuid: Option<[i32; 4]>,
        tag: Tag,
    ) -> std::result::Result<(), Tag> {
        self.load_unlisted_transferred(key, kind, uuid, tag, &mut None)
    }
    pub(super) fn load_unlisted_transferred(
        &mut self,
        key: ChunkKey,
        kind: &str,
        uuid: Option<[i32; 4]>,
        tag: Tag,
        transferred: &mut Option<super::entity_initialization::TransferredEntityState>,
    ) -> std::result::Result<(), Tag> {
        if !UNLISTED_KINDS.contains(&kind) {
            return Err(tag);
        }
        match kind {
            "minecraft:area_effect_cloud" => {
                let id = if let Some(state) = transferred.take() {
                    self.load_potion_cloud_transferred(&tag, Some(state))
                } else {
                    self.load_potion_cloud(&tag)
                };
                if let Some(id) = id {
                    let uuid = with_constructor_uuid(uuid);
                    self.remember(id, uuid, tag);
                }
            }
            "minecraft:evoker_fangs" => {
                if let Some(at) = entity_nbt::position(&tag) {
                    let delay = horizon_client::save::number(tag.get("Warmup")).unwrap_or(0) as i32;
                    let owner = self.loaded_fangs_owner(&tag);
                    let id = self.add_fangs([at.x, at.y, at.z], entity_nbt::rotation(&tag)[0], delay, owner);
                    if let Some(state) = transferred.take() {
                        self.adopt_transferred_entity_state(id, state);
                    }
                    let uuid = with_constructor_uuid(uuid);
                    self.remember(id, uuid, tag);
                }
            }
            "minecraft:ominous_item_spawner" => {
                if let Some(state) = transferred.take() {
                    self.load_ominous_item_spawner_transferred(tag, Some(state));
                } else {
                    self.load_ominous_item_spawner(tag);
                }
            }
            "minecraft:ender_dragon" => {
                if let Some(state) = transferred.take() {
                    self.load_dragon_transferred(tag, uuid, Some(state));
                } else {
                    self.load_dragon(tag, uuid);
                }
            }
            "minecraft:end_crystal" => {
                if let Some(state) = transferred.take() {
                    self.load_crystal_transferred(tag, uuid, Some(state));
                } else {
                    self.load_crystal(tag, uuid);
                }
            }
            "minecraft:cushion" => {
                if let Some(state) = transferred.take() {
                    self.load_cushion_transferred(tag, uuid, Some(state));
                } else {
                    self.load_cushion(tag, uuid);
                }
            }
            _ => return Err(tag),
        }
        Ok(())
    }
    /// A saved arrow or spectral arrow.
    pub(super) fn load_arrow(
        &mut self,
        tag: Tag,
        uuid: Option<[i32; 4]>,
        player_uuid: Option<[i32; 4]>,
    ) {
        self.load_arrow_transferred(tag, uuid, player_uuid, None);
    }

    /// A saved llama spit or shulker bullet.
    pub(super) fn load_spit_or_bullet(
        &mut self,
        tag: Tag,
        uuid: Option<[i32; 4]>,
        player_uuid: Option<[i32; 4]>,
    ) {
        self.load_spit_or_bullet_transferred(tag, uuid, player_uuid, None);
    }

    /// A saved fireball or wind charge.
    pub(super) fn load_fireball(
        &mut self,
        tag: Tag,
        uuid: Option<[i32; 4]>,
        player_uuid: Option<[i32; 4]>,
    ) {
        self.load_fireball_transferred(tag, uuid, player_uuid, None);
    }

    /// A saved thrown item.
    pub(super) fn load_throwable(
        &mut self,
        tag: Tag,
        uuid: Option<[i32; 4]>,
        player_uuid: Option<[i32; 4]>,
    ) {
        self.load_throwable_transferred(tag, uuid, player_uuid, None);
    }

    /// A saved falling block.
    pub(super) fn load_falling(&mut self, tag: Tag, uuid: Option<[i32; 4]>) {
        self.load_falling_transferred(tag, uuid, None);
    }

    /// A saved thrown trident.
    pub(super) fn load_trident(&mut self, tag: Tag, uuid: Option<[i32; 4]>) {
        self.load_trident_transferred(tag, uuid, None);
    }

    /// A saved firework rocket.
    pub(super) fn load_rocket(&mut self, tag: Tag, uuid: Option<[i32; 4]>) {
        self.load_rocket_transferred(tag, uuid, None);
    }

    /// `PrimedTnt.readAdditionalSaveData`: its numeric short fuse (80 when absent) and motion.
    // Its Passengers ride it (EntityType.loadEntityRecursive forces them on).
    pub(super) fn load_tnt(&mut self, tag: Tag, uuid: Option<[i32; 4]>) {
        self.load_tnt_transferred(tag, uuid, None);
    }

    /// A saved boat.
    pub(super) fn load_boat(&mut self, tag: Tag, uuid: Option<[i32; 4]>, k: &str) {
        self.load_boat_transferred(tag, uuid, k, None);
    }

    /// A saved armor stand.
    pub(super) fn load_armor_stand(&mut self, tag: Tag, uuid: Option<[i32; 4]>) {
        self.load_armor_stand_transferred(tag, uuid, None)
    }
    pub(super) fn load_armor_stand_transferred(
        &mut self,
        tag: Tag,
        uuid: Option<[i32; 4]>,
        transferred: Option<super::entity_initialization::TransferredEntityState>,
    ) {
        self.next_entity += 1;
        if let Some(mut a) = entity_nbt::armor_stand_from_nbt(&tag, self.next_entity) {
            a.live
                .retain_supplier(&self.motion.attributes, "minecraft:armor_stand");
            if let Some(state) = transferred {
                self.adopt_transferred_entity_state(a.id, state);
            }
            self.remember(a.id, uuid, tag);
            self.bind_saved_entity_attachments(a.id, &mut a.attachment_lifetime);
            self.armor_stands.push(a);
        }
    }

    /// The saved ender dragon.
    pub(super) fn load_dragon(&mut self, tag: Tag, uuid: Option<[i32; 4]>) {
        self.load_dragon_transferred(tag, uuid, None)
    }
    pub(super) fn load_dragon_transferred(
        &mut self,
        tag: Tag,
        uuid: Option<[i32; 4]>,
        transferred: Option<super::entity_initialization::TransferredEntityState>,
    ) {
        // The loaded dragon's own random (`Entity.random`): loading draws
        // nothing from the level's.
        let seed = random_source::unique_seed();
        if self.dragon.is_none()
            && let Some(mut d) = super::dragon::Dragon::from_nbt(&tag, seed)
        {
            // EnderDragon's constructor takes its id and its
            // parts' from the entity counter.
            d.live
                .retain_supplier(&self.motion.attributes, "minecraft:ender_dragon");
            d.id = self.next_entity + 1;
            self.next_entity += 9;
            if let Some(state) = transferred {
                self.adopt_transferred_entity_state(d.id, state);
            }
            self.remember_tags(d.id, &tag);
            self.remember_uuid(d.id, uuid, &tag);
            d.join = horizon_client::join_seq::JoinSeq::next();
            assert!(self.dragon.install(d).is_ok());
        }
    }

    /// A saved end crystal.
    pub(super) fn load_crystal(&mut self, tag: Tag, uuid: Option<[i32; 4]>) {
        self.load_crystal_transferred(tag, uuid, None)
    }
    pub(super) fn load_crystal_transferred(
        &mut self,
        tag: Tag,
        uuid: Option<[i32; 4]>,
        transferred: Option<super::entity_initialization::TransferredEntityState>,
    ) {
        let drawn = self.world.random.next_int_bound(100_000).unwrap_or(0);
        if let Some(mut c) = super::dragon::Crystal::from_nbt(&tag, drawn) {
            c.time = super::dragon::Crystal::start_time(c.position, drawn, self.clock.still());
            // EndCrystal's constructor takes an id from the
            // entity counter.
            self.next_entity += 1;
            c.id = self.next_entity;
            if let Some(state) = transferred {
                self.adopt_transferred_entity_state(c.id, state);
            }
            self.remember_tags(c.id, &tag);
            self.remember_uuid(c.id, uuid, &tag);
            self.crystals.push(c);
        }
    }

    /// A saved cushion.
    pub(super) fn load_cushion(&mut self, tag: Tag, uuid: Option<[i32; 4]>) {
        self.load_cushion_transferred(tag, uuid, None)
    }
    pub(super) fn load_cushion_transferred(
        &mut self,
        tag: Tag,
        uuid: Option<[i32; 4]>,
        transferred: Option<super::entity_initialization::TransferredEntityState>,
    ) {
        self.next_entity += 1;
        if let Some(c) = entity_nbt::cushion_from_nbt(&tag, self.next_entity) {
            if let Some(state) = transferred {
                self.adopt_transferred_entity_state(c.id, state);
            }
            self.remember(c.id, uuid, tag);
            self.cushions.push(c);
        }
    }

    /// `Entity.load`: adopt the bounded TAG_LIST_CODEC input as a set.
    /// Keep that adopted Mob state in the retained compound for commands and saves.
    pub(in crate::game) fn normalize_loaded_mob_tags(tag: &mut Tag) {
        let Tag::Compound(raw) = tag else {
            return;
        };
        let Some(Tag::List(mut tags)) = raw.remove(&"Tags".into()) else {
            return;
        };
        tags.retain(|tag| matches!(tag, Tag::String(_)));
        tags.truncate(1024);
        let mut seen = HashSet::new();
        tags.retain(|tag| seen.insert(tag.clone()));
        if !tags.is_empty() {
            raw.insert("Tags".into(), Tag::List(tags));
        }
    }

    /// A saved mob and, under `Passengers`, the mobs riding it.
    pub(super) fn load_mob(&mut self, tag: Tag, mount: Option<u32>) -> Option<u32> {
        self.load_mob_transferred(tag, mount, None)
    }

    /// The UUIDs `spawn_entity_from_nbt` would add for `tag`, in
    /// `Entity.getSelfAndPassengers` order: the root's, then those of the
    /// riders its loader takes, read as the loaders read them (a malformed
    /// UUID is none). Passengers the loaders keep unloaded have none here.
    pub(in crate::game) fn summon_tree_uuids(&mut self, tag: &Tag) -> Vec<[i32; 4]> {
        self.load_entity_types();
        let mut uuids: Vec<[i32; 4]> = entity_nbt::uuid(tag).into_iter().collect();
        self.rider_uuids(tag, &mut uuids);
        uuids
    }
    fn rider_uuids(&self, tag: &Tag, uuids: &mut Vec<[i32; 4]>) {
        if !self.saved_vehicle_accepts(tag) {
            return;
        }
        if let Some(Tag::List(list)) = tag.get("Passengers") {
            for p in list.iter().filter(|p| self.passenger_creatable(p)) {
                uuids.extend(entity_nbt::uuid(p));
                self.rider_uuids(p, uuids);
            }
        }
    }

    /// `Entity.shouldBeSaved`: serialize the ordered object graph once per rider.
    /// Slots retain mixed-kind load order; mob mounts also retain off-level objects.
    pub(in crate::game) fn put_mob_passengers(&mut self, tag: &mut Tag, id: u32) {
        let mut riders = Vec::new();
        for slot in self.ordered_passenger_slots(id) {
            match slot {
                PassengerSlot::Live(rider_id) => {
                    if let Some(object) = self.mob_object_slot(rider_id) {
                        riders.push(self.mob_object_tree_nbt(object));
                    } else if let Some(body) = self.saved_passenger_body(rider_id) {
                        riders.push(body);
                    }
                }
                PassengerSlot::Saved(saved_tag) => {
                    if entity_nbt::kind(&saved_tag)
                        .is_none_or(|kind| super::spawner::entity_type_saves(&kind))
                    {
                        riders.push(saved_tag);
                    }
                }
            }
        }
        add_passengers(tag, riders);
    }

    /// Read an unsimulated entity without assigning it a new identity. An eye of ender's
    /// item is set in `tag`, the compound the entity is kept with.
    pub(super) fn other_from_nbt(
        tag: &mut Tag,
        id: u32,
        kind: &str,
    ) -> Option<super::commands::OtherEntity> {
        let mut position = entity_nbt::position(tag)?;
        let item_transformation = (kind == "minecraft:item_display")
            .then(|| ItemDisplayTransformation::from_tag(tag.get("transformation")))
            .flatten();
        let knot_check = (kind == "minecraft:leash_knot").then_some(0);
        if knot_check.is_some() {
            // BlockAttachedEntity.setPos -> LeashFenceKnotEntity.recalculateBoundingBox.
            let block = BlockPos::containing(position);
            position =
                horizon_client::mobs::leash::knot_state([block.x, block.y, block.z]).position;
        }
        let interaction = super::interactions::is_interaction(kind)
            .then(|| super::interactions::InteractionState::from_tag(tag));
        let rotation = entity_nbt::loaded_rotation(tag);
        Some(super::commands::OtherEntity {
            attachment_lifetime: Default::default(),
            join: Default::default(),
            id,
            kind: kind.to_owned(),
            position,
            yaw: rotation[0],
            pitch: rotation[1],
            item_transformation,
            interaction,
            knot_check,
            eye: (kind == "minecraft:eye_of_ender").then(|| super::end::SummonedEye::load(tag)),
        })
    }

    /// An entity of a type Horizon does not simulate, kept as its compound.
    fn load_other(&mut self, tag: Tag, kind: &str) -> Option<u32> {
        self.load_other_transferred(tag, kind, None)
    }
    pub(super) fn load_other_transferred(
        &mut self,
        mut tag: Tag,
        kind: &str,
        transferred: Option<super::entity_initialization::TransferredEntityState>,
    ) -> Option<u32> {
        // A missing mod's record is opaque, not a vanilla fallback entity to normalize.
        let quarantined = super::general::quarantined_kind(kind).then(|| tag.clone());
        strip_finalize(&mut tag);
        let Tag::Compound(c) = &tag else { return None };
        let e = self.ecs_with(|ecs, mw| ecs.load_entity(mw, c))?;
        let id = self.ecs_id_of(e);
        let uuid = with_constructor_uuid(entity_nbt::uuid(&tag));
        if let Some(state) = transferred {
            self.adopt_transferred_entity_state(id, state);
        }
        self.remember(id, uuid, tag);
        if let Some(original) = quarantined {
            self.persist.raw.insert(id, original);
        }
        Some(id)
    }
    /// The compound of an entity Horizon does not simulate, with its current
    /// position, rotation and UUID.
    pub(super) fn other_to_nbt(&self, o: &super::commands::OtherEntity, uuid: [i32; 4]) -> Tag {
        let mut tag = self
            .persist
            .raw
            .get(&o.id)
            .cloned()
            .unwrap_or_else(|| Tag::Compound(Default::default()));
        if let Tag::Compound(c) = &mut tag {
            // Display.addAdditionalSaveData omits the transient start delay.
            if matches!(
                o.kind.as_str(),
                "minecraft:block_display" | "minecraft:item_display" | "minecraft:text_display"
            ) {
                c.remove(&"start_interpolation".into());
            }
            c.insert("id".into(), Tag::String(o.kind.as_str().into()));
            c.insert(
                "Pos".into(),
                Tag::List(
                    [o.position.x, o.position.y, o.position.z]
                        .map(|v| Tag::Double(v.to_bits()))
                        .to_vec(),
                ),
            );
            c.insert(
                "Rotation".into(),
                Tag::List(vec![
                    Tag::Float(o.yaw.to_bits()),
                    Tag::Float(o.pitch.to_bits()),
                ]),
            );
            c.insert("UUID".into(), Tag::IntArray(uuid.to_vec()));
        }
        let display = matches!(
            o.kind.as_str(),
            "minecraft:text_display" | "minecraft:block_display" | "minecraft:item_display"
        );
        if display || super::interactions::is_interaction(&o.kind) {
            write_static_entity_fields(&mut tag);
        }
        if display {
            self.write_display_nbt(o, &mut tag);
        }
        if let Some(state) = &o.interaction
            && let Tag::Compound(fields) = &mut tag
        {
            state.write(fields);
        }
        if let Some(eye) = o.eye
            && let Tag::Compound(c) = &mut tag
        {
            // Entity.saveWithoutId: the live motion and the fire ticks.
            let m = eye.motion;
            c.insert(
                "Motion".into(),
                Tag::List([m.x, m.y, m.z].map(|v| Tag::Double(v.to_bits())).to_vec()),
            );
            c.entry("Fire".into()).or_insert(Tag::Short(0));
        }
        // Marker, LeashFenceKnotEntity and EyeOfEnder still use
        // Entity.saveWithoutId's common fields, which their own saves leave out.
        if o.eye.is_some()
            || matches!(
                o.kind.as_str(),
                "minecraft:marker" | "marker" | "minecraft:leash_knot"
            )
        {
            entity_nbt::fill_common_defaults(&mut tag);
        }
        with_raw(tag, None) // Entity.saveWithoutId's flags, as booleans.
    }
    /// The root of a stack of riders (`Entity.getRootVehicle`).
    fn mob_root(&self, mut object: MobObject) -> MobObject {
        for _ in 0..self.remote_entities.len() + self.persist.off_level.len() {
            let Some(vehicle) = self.mob_at(object).extra.mount else {
                break;
            };
            let Some(parent) = self.mob_object_slot(vehicle) else {
                break;
            };
            object = parent;
        }
        object
    }

    /// `Entity.getVehicle` of entity `id`: the mob, boat or minecart a mob
    /// rides (only a mob rides another entity here).
    fn vehicle_of(&self, id: u32) -> Option<u32> {
        // A mob rider: its mount, when the vehicle is still an object of the level (the
        // object memory also holds riders refused residency).
        if let Some(mob) = self.mob_object(id) {
            let v = mob.extra.mount?;
            let there = self.mob_objects().any(|m| m.id == v)
                || self.is_boat(v)
                || self.ecs.cart_kind(v).is_some()
                || self.passenger_present(v);
            return there.then_some(v);
        }
        // A rider of any other kind: the vehicle whose slots name it.
        self.passenger_vehicle(id)
    }
    /// The actual root, using the existing per-kind data writers.
    pub(super) fn vehicle_tree_nbt(&mut self, root: u32) -> Option<Tag> {
        if let Some(object) = self.mob_object_slot(root) {
            return Some(self.mob_object_tree_nbt(object));
        }
        if self.is_boat(root) {
            if !self.persist.off_level.contains_key(&root)
                && !self.resident_source_target_visible(
                    self.dim.key(),
                    super::commands::Target::Boat(root),
                )
            {
                return None;
            }
            let uuid = self.boat_object(root).is_some().then(|| self.uuid_for(root));
            let mut tag = match uuid.zip(self.boat_object(root)) {
                Some((uuid, b)) => b.to_nbt(uuid, self.persist.raw.get(&root)),
                None => Tag::Compound(self.ecs_mob_tree(root)?),
            };
            self.put_mob_passengers(&mut tag, root);
            write_attachments(&mut tag, root, &self.entity_attachments);
            return Some(tag);
        }
        if self.ecs.cart_kind(root).is_some() {
            return self.ecs_mob_tree(root).map(Tag::Compound);
        }
        // Any other kind of root vehicle, with its passengers.
        self.saved_passenger_body(root)
    }
    /// `ServerPlayer.saveParentVehicle`: a player riding `vehicle` saves
    /// RootVehicle {Attach: that mount's UUID, Entity: the root vehicle with
    /// its passengers}, only while it is the root's one player passenger
    /// (`Entity.hasExactlyOnePlayerPassenger`); `others` are the vehicles the
    /// other players in the level ride. With the root and the mount's UUID.
    pub(super) fn parent_vehicle_tag(
        &mut self,
        vehicle: Option<u32>,
        others: &[u32],
    ) -> Option<(Tag, u32, [i32; 4])> {
        let vehicle = vehicle?;
        let root = parent_vehicle_root(vehicle, others, |id| self.vehicle_of(id))?;
        let entity = self.vehicle_tree_nbt(root)?;
        let attach = self.uuid_for(vehicle);
        Some((root_vehicle_tag(attach, entity), root, attach))
    }
    /// `PlayerList.remove`: the root vehicle and its passengers leave the
    /// world with the player (`setRemoved(UNLOADED_WITH_PLAYER)`).
    pub(super) fn remove_vehicle_tree(&mut self, root: u32) {
        // The mobs `put_mob_passengers` saves under each vehicle of the tree.
        let mounts: Vec<(u32, Option<u32>)> = self
            .mob_objects()
            .filter(|m| !m.removed)
            .map(|m| (m.id, m.extra.mount))
            .collect();
        let mut gone = vehicle_tree(root, &mounts);
        // The ECS's creatures riding the root, all the way down (the host's lists are empty on a server).
        if self.ecs.is_active() {
            for id in self.ecs_tree(root) {
                if !gone.contains(&id) {
                    gone.push(id);
                }
            }
        }
        // And the riders of other kinds that the slots name.
        for id in self.passenger_tree(root) {
            if !gone.contains(&id) {
                gone.push(id);
            }
        }
        if self.ecs.is_active() {
            for &g in &gone {
                if let Some(e) = self.ecs.entity_of(g) {
                    self.ecs.despawn(e);
                }
            }
        } else {
            self.remote_entities.retain(|m| !gone.contains(&m.id));
        }
        self.persist.off_level.retain(|id, _| !gone.contains(id));
        for &id in &gone {
            self.release_vehicle_container(id);
        }
        // Entities of the other kinds, the slots and the links of the whole tree.
        self.discard_passenger_entities(&gone);
        for g in gone {
            self.forget_entity(g);
        }
    }
    /// The vehicle with this UUID, among the ones in the world.
    pub(super) fn vehicle_by_uuid(&self, uuid: [i32; 4]) -> Option<u32> {
        self.persist.uuids.iter().find_map(|(&id, &found)| {
            (found == uuid
                && (self.mob_present(id)
                    || (self.ecs.boat_status(id).is_some()
                        && self.resident_source_target_visible(
                            self.dim.key(),
                            super::commands::Target::Boat(id),
                        ))
                    || (self.ecs.cart_kind(id).is_some()
                        && self.resident_source_target_visible(
                            self.dim.key(),
                            super::commands::Target::Cart(id),
                        ))
                    // A vehicle of another kind (an armor stand, a display...).
                    || (!(self.mob_known(id)
                        || self.ecs.boat_status(id).is_some()
                        || self.ecs.cart_kind(id).is_some())
                        && self.passenger_present(id))))
            .then_some(id)
        })
    }
    /// A vehicle already in the world that was restored from the player's file
    /// during this load (not any entity whose UUID is remembered).
    fn restored_vehicle(&self, uuid: [i32; 4]) -> Option<u32> {
        if !self.persist.restored.contains(&uuid) {
            return None;
        }
        self.vehicle_by_uuid(uuid)
    }
    /// Vanilla stores the root entity in the player's file, outside chunk entities.
    /// Keep the raw wrapper until attachment succeeds, including unsupported trees.
    pub(super) fn restore_saved_vehicle(
        &mut self,
        uuid: [i32; 4],
        raw: Option<&Tag>,
    ) -> Option<u32> {
        if let Some(id) = self.vehicle_by_uuid(uuid) {
            return Some(id);
        }
        let tag = raw?.get("RootVehicle")?.get("Entity")?.clone();
        let root_uuid = entity_nbt::uuid(&tag)?;
        self.load_entity_types();
        if !self.passenger_creatable(&tag) {
            return None;
        }
        if self.vehicle_by_uuid(root_uuid).is_none() {
            let at = entity_nbt::position(&tag)?;
            let key = (
                (at.x.floor() as i32).div_euclid(16),
                (at.z.floor() as i32).div_euclid(16),
            );
            self.persist.restored.insert(root_uuid);
            self.world.queue_entity_nbt(key, vec![tag]);
            self.spawn_loaded_entities();
        }
        self.persist.restored.insert(uuid);
        self.vehicle_by_uuid(uuid)
    }
    /// `ServerPlayer.loadAndSpawnParentVehicle`: the ride the player's file
    /// names, once the vehicle is in the world.
    pub(super) fn restore_root_vehicle(&mut self) {
        if self.serving.is_some()
            || self.options.dedicated.is_some()
            || self.riding.is_some()
            || self.player.is_none()
        {
            return;
        }
        let Some(uuid) = self.pending_vehicle else {
            return;
        };
        let raw = self.level.player_extra.clone();
        let Some(id) = self.restore_saved_vehicle(uuid, raw.as_ref()) else {
            return;
        };
        if let Some(Tag::Compound(p)) = &mut self.level.player_extra {
            p.remove(&"RootVehicle".into());
        }
        self.pending_vehicle = None;
        self.riding = Some(id);
    }
    /// A mob's entity tag with the mobs riding it under `Passengers`.
    pub(super) fn mob_tree_nbt(&mut self, i: usize) -> Tag {
        self.mob_object_tree_nbt(MobObject::Resident(i))
    }
    /// Forget an entity that left the world (its UUID and loaded compound).
    pub(super) fn forget_entity(&mut self, id: u32) {
        self.persist.uuids.remove(&id);
        self.persist.score_destroyed.remove(&id);
        self.forget_entity_initialization(mod_api::Holder::Entity(id));
        self.entity_attachments.remove(&id);
        self.persist.raw.remove(&id);
        self.persist.no_physics.remove(&id);
    }
    /// Detach a travelling entity without discarding its in-memory attachments.
    pub(super) fn detach_entity(&mut self, id: u32) {
        self.persist.score_destroyed.remove(&id);
        self.persist.uuids.remove(&id);
        self.persist.raw.remove(&id);
    }
    /// The compound a crystal or the dragon was loaded with, found again by type
    /// and position.
    pub(super) fn persist_anon(&self, kind: &str, p: Vec3) -> Option<&Tag> {
        self.persist
            .anon
            .get(&anon_key(kind, p))
            .map(|(_, raw)| raw)
    }
    /// `Entity.applyEffectsFromBlocks` on a fire-immune entity that is not
    /// burning (`EndCrystal.tick`): the loaded `Fire` is reset to
    /// `-getFireImmuneTicks()`, which is 0.
    pub(super) fn clear_crystal_fire(&mut self) {
        for c in &self.crystals {
            let key = anon_key("minecraft:end_crystal", c.position);
            if let Some((_, Tag::Compound(raw))) = self.persist.anon.get_mut(&key)
                && let Some(fire) = raw.get_mut(&"Fire".into())
            {
                *fire = Tag::Short(0);
            }
        }
    }
    /// The compound an entity was loaded with (and commands have edited).
    pub(super) fn persist_raw(&self, id: u32) -> Option<&Tag> {
        self.persist.raw.get(&id)
    }
    /// An entity Horizon keeps no compound for (orbs, projectiles, the
    /// crystals and the dragon) still keeps its `Tags`, by entity id, so
    /// `@e[tag=]`, `/tag` and `/data get` see them, and the `Item` a
    /// fireball or a thrown item projectile was loaded with.
    pub(super) fn remember_tags(&mut self, id: u32, tag: &Tag) {
        let mut map = horizon_core::nbt::Compound::new();
        if let Some(tags @ Tag::List(_)) = tag.get("Tags") {
            map.insert("Tags".into(), tags.clone());
        }
        if let Some(item @ Tag::Compound(_)) = tag.get("Item") {
            map.insert("Item".into(), item.clone());
        }
        // Entity.saveWithoutId's optional fields Horizon models nowhere for
        // these entities, kept as loaded (written only when set).
        for key in [
            "Silent",
            "Glowing",
            "HasVisualFire",
            "Invulnerable",
            "CustomName",
            "CustomNameVisible",
        ] {
            if let Some(v) = tag.get(key) {
                map.insert(key.into(), v.clone());
            }
        }
        if !map.is_empty() {
            self.persist.raw.insert(id, Tag::Compound(map));
        }
    }
    /// `Entity.isSilent`: mobs model it directly; other entities retain it
    /// in their saved fields, including command changes to `Silent`.
    pub(super) fn entity_silent(&self, id: u32) -> bool {
        if self.ecs.is_active() {
            return self
                .ecs
                .entity_of(id)
                .and_then(|e| self.ecs.entity_nbt(e))
                .is_some_and(|c| entity_nbt::flag(&Tag::Compound(c), "Silent"));
        }
        self.remote_entities.iter().find(|m| m.id == id).map_or_else(
            || {
                self.persist
                    .raw
                    .get(&id)
                    .is_some_and(|tag| horizon_client::entity_nbt::flag(tag, "Silent"))
            },
            |m| m.silent,
        )
    }
    fn remember(&mut self, id: u32, uuid: Option<[i32; 4]>, mut tag: Tag) {
        self.restore_portal_cooldown(id, &tag);
        self.remember_uuid(id, uuid, &tag);
        // `Team` is read, never written: the team keeps its members itself.
        if let Tag::Compound(c) = &mut tag {
            c.remove(&"Team".into());
        }
        self.persist.raw.insert(id, tag);
    }
    /// The UUID entity `id` was loaded with, and the attachments `tag` saved under it.
    pub(super) fn remember_uuid(&mut self, id: u32, uuid: Option<[i32; 4]>, tag: &Tag) {
        if let Some(u) = uuid {
            self.persist.uuids.insert(id, u);
        }
        // `EntityType.create` then `Entity.load`: the constructing listeners set their
        // values first and the saved attachments are read over them. An entity whose map
        // is already live (told before) keeps it, as before.
        let live = self.entity_attachments.contains_key(&id);
        if let Some(kind) = text(tag.get("id")) {
            self.initialize_entity(mod_api::Holder::Entity(id), &kind);
        }
        read_attachments(tag, id, &mut self.entity_attachments, live);
        self.join_saved_team(id, tag);
    }
    fn join_saved_team(&mut self, id: u32, tag: &Tag) {
        if tag.get("Team").is_some() {
            let name = uuid_text(self.uuid_for(id));
            join_saved_team(&mut self.score.board, &name, tag);
        }
    }
    /// The entity a UUID read back from a save names, while it is in the level:
    /// the player, or a mob loaded or saved with that UUID. `None` for nothing
    /// loaded (an entity that is not here, or one `Who` cannot carry) - never
    /// some other entity as a stand-in.
    pub(super) fn who_by_uuid(&self, uuid: [i32; 4]) -> Option<Who> {
        let player = if self.player.is_some() {
            self.level.player_uuid
        } else {
            None
        };
        let legacy: &[Mob] = if self.ecs.is_active() { &[] } else { &self.remote_entities };
        entity_nbt::who_by_uuid(&self.persist.uuids, legacy, player, uuid).or_else(|| self.ecs_who_by_uuid(uuid))
    }
    /// The mob that was loaded or saved with a UUID.
    pub(super) fn mob_by_uuid(&self, uuid: [i32; 4]) -> Option<u32> {
        match self.who_by_uuid(uuid) {
            Some(Who::Mob(id)) => Some(id),
            _ => None,
        }
    }
    /// A mob's UUID, made up for new ones.
    pub(super) fn mob_uuid(&mut self, id: u32) -> [i32; 4] {
        self.uuid_for(id)
    }
    pub(super) fn uuid_for(&mut self, id: u32) -> [i32; 4] {
        if let Some(uuid) = self.known_resident_entity_uuid_at(self.dim.key(), id) {
            return uuid;
        }
        self.persist.uuids.get_or_insert_with(id, random_uuid)
    }
    /// `BlockState.CODEC`: a block name or `{id, properties}` (or legacy `{Name, Properties}`).
    pub(super) fn state_from_nbt(&self, tag: &Tag) -> Option<BlockStateId> {
        entity_nbt::state_from_nbt(self.pack.assets.registry, tag)
    }
    pub(in crate::game) fn state_to_nbt(&self, state: BlockStateId) -> Tag {
        entity_nbt::state_to_nbt(self.pack.assets.registry, state)
    }
    /// The entity NBT of every simulated entity in `keys`, plus the unknown
    /// entities kept for them. With `remove`, the entities leave the world
    /// (their chunks unload).
    pub(super) fn entity_chunks(
        &mut self,
        keys: &HashSet<(i32, i32)>,
        remove: bool,
    ) -> Vec<((i32, i32), Vec<Tag>)> {
        use horizon_client::join_seq::{JoinSeq, Joined};
        // Each root with its id and join number, ordered once all are in.
        let mut roots: HashMap<(i32, i32), Vec<(u32, JoinSeq, Tag)>> = HashMap::new();
        // Every kind in `entity_lists!` saves itself (`Entity::save_nbt`).
        self.save_every_entity(keys, &mut roots);
        if let Some(d) = self.dragon.as_ref()
            && keys.contains(&chunk_of(d.position))
        {
            let raw = anon_raw(&self.persist, "minecraft:ender_dragon", d.position);
            let uuid = saved_uuid(&self.persist, d.id);
            // The fields kept with it (`remember_tags`: Silent, Tags, a
            // name...) once it has flown from where it was loaded.
            let mut tag = with_raw(d.to_nbt(uuid, raw.as_ref()), self.persist.raw.get(&d.id));
            write_attachments(&mut tag, d.id, &self.entity_attachments);
            roots
                .entry(chunk_of(d.position))
                .or_default()
                .push((d.id, d.join_seq(), tag));
        }
        for c in &self.crystals {
            if keys.contains(&chunk_of(c.position)) {
                let raw = anon_raw(&self.persist, "minecraft:end_crystal", c.position);
                let uuid = saved_uuid(&self.persist, c.id);
                let mut tag = with_raw(c.to_nbt(uuid, raw.as_ref()), self.persist.raw.get(&c.id));
                write_attachments(&mut tag, c.id, &self.entity_attachments);
                roots
                    .entry(chunk_of(c.position))
                    .or_default()
                    .push((c.id, c.join_seq(), tag));
            }
        }
        for s in &self.ominous_spawners {
            let at = Vec3::new(s.position[0], s.position[1], s.position[2]);
            if keys.contains(&chunk_of(at)) {
                let mut tag = entity_nbt::ominous_item_spawner_to_nbt(
                    &s.item,
                    at,
                    s.spawn_item_after,
                    self.persist.uuids.get(&s.id).copied().unwrap_or(s.uuid),
                    s.raw.as_deref(),
                );
                write_attachments(&mut tag, s.id, &self.entity_attachments);
                roots
                    .entry(chunk_of(at))
                    .or_default()
                    .push((s.id, s.join_seq(), tag));
            }
        }
        for c in &self.cushions {
            if keys.contains(&chunk_of(c.position)) {
                let uuid = self
                    .persist
                    .uuids
                    .get(&c.id)
                    .copied()
                    .unwrap_or_else(random_uuid);
                let mut tag = entity_nbt::cushion_to_nbt(c, uuid, self.persist.raw.get(&c.id));
                write_attachments(&mut tag, c.id, &self.entity_attachments);
                roots
                    .entry(chunk_of(c.position))
                    .or_default()
                    .push((c.id, c.join_seq(), tag));
            }
        }
        // `AreaEffectCloud.addAdditionalSaveData`: a lingering potion's cloud is
        // an entity of its chunk. A dragon's own breath carries no potion and
        // is not written.
        for c in &self.breath_clouds {
            let at = Vec3::new(c.at[0], c.at[1], c.at[2]);
            if c.potion.is_none() || !keys.contains(&chunk_of(at)) {
                continue;
            }
            let uuid = self
                .persist
                .uuids
                .get(&c.id)
                .copied()
                .unwrap_or_else(random_uuid);
            let mut tag = super::potions::potion_cloud_to_nbt(c, uuid, self.persist.raw.get(&c.id));
            write_attachments(&mut tag, c.id, &self.entity_attachments);
            roots
                .entry(chunk_of(at))
                .or_default()
                .push((c.id, c.join_seq(), tag));
        }
        // `PersistentEntitySectionManager.storeChunkSections`: section by
        // section, each in the order its entities joined it. An entity not
        // yet given an id is not a lookup member: its join stands in.
        let mut out: HashMap<(i32, i32), Vec<Tag>> =
            keys.iter().map(|k| (*k, Vec::new())).collect();
        {
            let admission = self.resident_entities.table.get_mut().admission_order();
            for (key, mut tags) in roots {
                tags.sort_by_cached_key(|(id, join, tag)| {
                    (section_order(tag), admission(*id).unwrap_or(*join))
                });
                out.entry(key)
                    .or_default()
                    .extend(tags.into_iter().map(|(_, _, tag)| tag));
            }
        }
        if self.ecs.is_active() {
            for key in keys {
                let disk = self.ecs_disk_unknown(key.0, key.1);
                let saved = if remove { self.ecs_unload_chunk(key.0, key.1, disk) } else { self.ecs_save_chunk(key.0, key.1, disk) };
                out.entry(*key).or_default().extend(saved.into_iter().map(Tag::Compound));
            }
        }
        for key in keys {
            let unknown = if remove {
                self.persist.unknown.remove(key)
            } else {
                self.persist.unknown.get(key).cloned()
            };
            if let Some(list) = unknown {
                out.entry(*key).or_default().extend(list);
            }
        }
        for tags in out.values_mut() {
            // A rider is saved inside its vehicle, not as a root of the chunk.
            tags.retain(|tag| self.saved_entity_is_root(tag));
            for tag in tags {
                // The kinds whose own writers already save their riders stay as written.
                if let Some(id) = entity_nbt::uuid(tag).and_then(|u| self.vehicle_by_uuid(u))
                    && !matches!(
                        self.level_entity(id as i32),
                        Some(
                            super::commands::Target::Mob(_)
                                | super::commands::Target::Boat(_)
                                | super::commands::Target::Cart(_)
                                | super::commands::Target::Tnt(_)
                        )
                    )
                {
                    self.put_mob_passengers(tag, id);
                }
            }
        }
        if remove {
            // The trees of the vehicles leaving, found before any of them is removed.
            let tree_ids: Vec<u32> = out
                .values()
                .flatten()
                .filter_map(entity_nbt::uuid)
                .filter_map(|u| self.vehicle_by_uuid(u))
                .filter(|id| Some(*id) != self.riding)
                .flat_map(|id| self.passenger_tree(id))
                .collect();
            self.unload_removed_entities(keys);
            self.discard_passenger_entities(&tree_ids);
        }
        out.into_iter().collect()
    }
    /// Write the entities of every resident chunk (an autosave or quit).
    /// Without `wait` (the autosave) they are written on the region writer
    /// meanwhile: `PersistentEntitySectionManager.autoSave` stores them
    /// without `saveAll`'s flush.
    pub(super) fn save_entities(&mut self, wait: bool) -> Result<()> {
        self.spawn_loaded_entities();
        let keys: HashSet<(i32, i32)> = self.world.chunks.keys().copied().collect();
        let chunks = self.entity_chunks(&keys, false);
        self.prune_entity_memory();
        if wait {
            self.save.save_entities(chunks)
        } else {
            self.save.save_entities_later(chunks)
        }
    }
    /// Complete a removed vehicle's scoreboard callback at logical removal,
    /// even when physical removal waits for the native-removal sweep.
    pub(super) fn score_destroyed_vehicle_once(&mut self, id: u32) {
        if self.persist.score_destroyed.insert(id)
            && let Some(uuid) = self.persist.uuids.get(&id)
        {
            self.score.board.entity_removed(&uuid_text(*uuid));
        }
    }

    /// `ServerLevel.EntityCallbacks.onDestroyed` → `Scoreboard.entityRemoved`
    /// for every kind of entity: a mob's body once it is removed, an item
    /// picked up or killed, an expired cloud, a discarded projectile leaves
    /// the board and its team. Unloaded and
    /// travelling entities leave `persist.uuids` first (`forget_entity`,
    /// `detach_entity`), so they keep their scores, as vanilla's do. Once a
    /// tick, so a command in the same tick still sees them; nothing to do
    /// while no entity holds a score or a team.
    pub(super) fn score_destroyed_entities(&mut self) {
        if !self.persist.off_level.is_empty() {
            for (&id, object) in &self.persist.off_level {
                if object.is_removed()
                    && self.persist.score_destroyed.insert(id)
                    && let Some(uuid) = self.persist.uuids.get(&id)
                {
                    self.score.board.entity_removed(&uuid_text(*uuid));
                }
            }
        }
        for &id in self.persist.removed_vehicles.keys() {
            if self.persist.score_destroyed.insert(id)
                && let Some(uuid) = self.persist.uuids.get(&id)
            {
                self.score.board.entity_removed(&uuid_text(*uuid));
            }
        }
        let board = &self.score.board;
        let held: HashSet<[i32; 4]> = board
            .scores
            .keys()
            .map(String::as_str)
            .chain(board.team_members())
            .filter(|holder| holder.len() == 36)
            .filter_map(uuid_from_text)
            .collect();
        if held.is_empty() {
            return;
        }
        let candidates: Vec<(u32, [i32; 4])> = self
            .persist
            .uuids
            .iter()
            .filter(|(id, uuid)| {
                !self.persist.score_destroyed.contains(*id) && held.contains(*uuid)
            })
            .map(|(&id, &uuid)| (id, uuid))
            .collect();
        if candidates.is_empty() {
            return;
        }
        let mut alive = self.live_entity_ids();
        if !self.persist.off_level.is_empty() {
            // Query residency is not destruction. Rejected objects still own
            // their UUID state until removed, just like registered objects.
            alive.extend(
                self.persist
                    .off_level
                    .iter()
                    .filter_map(|(&id, object)| (!object.is_removed()).then_some(id)),
            );
        }
        for (id, uuid) in candidates {
            if !alive.contains(&id) {
                self.score.board.entity_removed(&uuid_text(uuid));
            }
        }
    }
    pub(super) fn prune_entity_memory(&mut self) {
        if !self.persist.removed_vehicles.is_empty()
            || self.persist.off_level.values().any(OffLevel::is_removed)
        {
            // Complete the former resident's destruction before graph GC can
            // forget the UUID retained solely for dismount geometry.
            self.score_destroyed_entities();
        }
        // Forget entities that no longer exist.
        let mut alive = self.live_entity_ids();
        if !self.persist.off_level.is_empty() || !self.persist.removed_vehicles.is_empty() {
            self.retain_off_level_objects(&mut alive);
        }
        for id in self.persist.uuids.keys() {
            if !alive.contains(id) {
                self.entity_attachments.remove(id);
            }
        }
        self.prune_entity_initializations(&alive);
        self.persist.uuids.retain(|id, _| alive.contains(id));
        self.persist.raw.retain(|id, _| alive.contains(id));
        self.persist.score_destroyed.retain(|id| alive.contains(id));
    }

    /// Save and remove the entities of chunks that unloaded.
    pub(super) fn unload_entities(&mut self, unloaded: &[Arc<Chunk>]) {
        if unloaded.is_empty() {
            return;
        }
        self.spawn_loaded_entities();
        let keys: HashSet<(i32, i32)> = unloaded.iter().map(|c| (c.x, c.z)).collect();
        let chunks = self.entity_chunks(&keys, true);
        let counts: Vec<_> = chunks
            .iter()
            .map(|(key, entities)| (*key, entities.len()))
            .collect();
        if let Err(e) = self.save.save_entities_later(chunks) {
            log::warn!("Horizon: failed to save entities: {e}");
        }
        for (chunk, count) in counts {
            self.native_entities_unload(self.dim.id(), [chunk.0, chunk.1], count);
        }
    }
}
impl Game<'_> {
    /// `Entity::save_nbt` for a mob: a rider is saved inside its root vehicle,
    /// in the root's chunk; a boat's or cart's passengers inside the vehicle.
    pub(super) fn save_mob(
        &mut self,
        i: usize,
        keys: &HashSet<ChunkKey>,
    ) -> Option<(ChunkKey, Tag)> {
        // The ECS world saves its entities by chunk (`entity_chunks`): no legacy mob is saved then.
        if self.ecs.is_active() {
            return None;
        }
        let m = &self.remote_entities[i];
        // `Entity.shouldBeSaved`: a passenger saves only under its root, even
        // when that root is a boat or cart retained off-level and never saved.
        let saves = self.mob_root(MobObject::Resident(i)) == MobObject::Resident(i)
            && m.extra
                .mount
                .is_none_or(|v| !self.is_boat(v) && self.ecs.cart_kind(v).is_none())
            && keys.contains(&chunk_of(m.position))
            && !m.removed;
        saves.then(|| (chunk_of(self.remote_entities[i].position), self.mob_tree_nbt(i)))
    }
    /// The UUID a save writes for a reference to `who`, as `mob_to_nbt`'s
    /// `angry_at` does: the bridge's `who_uuid` over `EntityMemory::uuid_of`,
    /// which answers the UUID the entity was loaded or saved with. A mob the
    /// level has none for writes nothing.
    pub(in crate::game) fn save_who_uuid(&self, who: Who) -> Option<[i32; 4]> {
        entity_nbt::who_uuid(who, &mut |id| {
            if self.ecs.is_active() { self.ecs.uuid_words(id) } else { self.persist.uuid_of(id) }
        })
    }
    /// A falling block is saved by P (the ECS level): the host writes none.
    pub(super) fn save_falling(
        &mut self,
        _i: usize,
        _keys: &HashSet<ChunkKey>,
    ) -> Option<(ChunkKey, Tag)> {
        None
    }
    pub(super) fn save_rocket(
        &mut self,
        _i: usize,
        _keys: &HashSet<ChunkKey>,
    ) -> Option<(ChunkKey, Tag)> {
        // Firework rockets are saved by the ECS (`save_chunk`): no host rocket is left to save.
        None
    }
    pub(super) fn save_trident(
        &mut self,
        _i: usize,
        _keys: &HashSet<ChunkKey>,
    ) -> Option<(ChunkKey, Tag)> {
        // Thrown tridents are saved by the ECS (`save_chunk`): no host trident is left to save.
        None
    }
    pub(super) fn save_tnt(
        &mut self,
        _i: usize,
        _keys: &HashSet<ChunkKey>,
    ) -> Option<(ChunkKey, Tag)> {
        // Primed TNT is saved by the ECS (`save_chunk`): no host TNT is left to save.
        None
    }
    pub(super) fn save_boat(
        &mut self,
        _i: usize,
        _keys: &HashSet<ChunkKey>,
    ) -> Option<(ChunkKey, Tag)> {
        // Boats are saved by the ECS (`save_chunk`): no host boat is left to save.
        None
    }
    pub(super) fn save_armor_stand(
        &mut self,
        i: usize,
        keys: &HashSet<ChunkKey>,
    ) -> Option<(ChunkKey, Tag)> {
        let a = &self.armor_stands[i];
        keys.contains(&chunk_of(a.position)).then(|| {
            let uuid = self.saved_uuid_or_random(a.id);
            let tag = entity_nbt::armor_stand_to_nbt(a, uuid, self.persist.raw.get(&a.id));
            (chunk_of(a.position), tag)
        })
    }
    /// The UUID kept for `id`, or a new one that is not remembered (what a
    /// saved vehicle or stand gets when it never had one).
    fn saved_uuid_or_random(&self, id: u32) -> [i32; 4] {
        self.persist
            .uuids
            .get(&id)
            .copied()
            .unwrap_or_else(random_uuid)
    }
}

/// The mob layer's [`horizon_client::mobs::WhoLookup`]: a warden's anger keeps
/// the suspects that left by UUID, and resolves the ones that are back through
/// the same map a save writes them from (`mob_to_nbt`'s `uuid_of`).
impl horizon_client::mobs::WhoLookup for Game<'_> {
    fn who_by_uuid(&self, uuid: [i32; 4]) -> Option<Who> {
        Game::who_by_uuid(self, uuid)
    }
}

/// A chunk's key, as `chunk_of` gives it.
pub(super) type ChunkKey = (i32, i32);

/// The raw compound kept for an entity loaded at `at`, found by kind and position.
fn anon_raw(persist: &EntityMemory, kind: &str, at: Vec3) -> Option<Tag> {
    persist
        .anon
        .get(&anon_key(kind, at))
        .map(|(_, raw)| raw.clone())
}
/// The UUID entity `id` was loaded with or given, else a new one for this save.
fn saved_uuid(persist: &EntityMemory, id: u32) -> [i32; 4] {
    persist.uuids.get(&id).copied().unwrap_or_else(random_uuid)
}

/// Only a mob is finalized; another entity drops the request.
fn strip_finalize(tag: &mut Tag) {
    if let Tag::Compound(c) = tag {
        c.remove(&"horizon:finalize".into());
    }
}
/// `riders` under `tag`'s `Passengers`, after the passengers it kept from
/// loading (`Entity.saveWithoutId`).
pub(super) fn add_passengers(tag: &mut Tag, riders: Vec<Tag>) {
    if let Tag::Compound(c) = tag {
        let mut list = match c.get(&"Passengers".into()) {
            Some(Tag::List(l)) => l.clone(),
            _ => Vec::new(),
        };
        list.extend(riders);
        if list.is_empty() {
            c.remove(&"Passengers".into());
        } else {
            c.insert("Passengers".into(), Tag::List(list));
        }
    }
}
/// `Entity.getRootVehicle`: up the vehicles `vehicle_of` gives, from `id`.
fn root_vehicle(id: u32, vehicle_of: impl Fn(u32) -> Option<u32>) -> u32 {
    let mut root = id;
    // A bound against a loop of mounts.
    for _ in 0..16 {
        match vehicle_of(root) {
            Some(v) if v != root => root = v,
            _ => break,
        }
    }
    root
}
/// `ServerPlayer.saveParentVehicle`'s root: that of `vehicle`, while no
/// other player rides its tree (`Entity.hasExactlyOnePlayerPassenger`);
/// `others` are the vehicles the other players ride.
fn parent_vehicle_root(
    vehicle: u32,
    others: &[u32],
    vehicle_of: impl Fn(u32) -> Option<u32>,
) -> Option<u32> {
    let root = root_vehicle(vehicle, &vehicle_of);
    let shared = others.iter().any(|&o| root_vehicle(o, &vehicle_of) == root);
    (!shared).then_some(root)
}
/// RootVehicle {Attach: the mount's UUID, Entity: the root's tree}.
fn root_vehicle_tag(attach: [i32; 4], entity: Tag) -> Tag {
    Tag::compound([
        ("Attach", Tag::IntArray(attach.to_vec())),
        ("Entity", entity),
    ])
}
/// `Entity.getPassengersAndSelf` of `root`, from each rider's mount.
pub(super) fn vehicle_tree(root: u32, mounts: &[(u32, Option<u32>)]) -> Vec<u32> {
    let mut tree = vec![root];
    let mut k = 0;
    while k < tree.len() {
        let v = tree[k];
        let riders: Vec<u32> = mounts
            .iter()
            .filter(|(id, m)| *m == Some(v) && !tree.contains(id))
            .map(|(id, _)| *id)
            .collect();
        tree.extend(riders);
        k += 1;
    }
    tree
}

/// Only a Mob or an AbstractBoat is Leashable: any other kind never reads
/// `leash`, so a summoned or loaded tag must not be kept and saved.
fn drop_unleashable_leash(tag: &mut Tag, kind: &str, is_mob: bool) {
    if !is_mob
        && super::boats::any_boat_wood(kind).is_none()
        && let Tag::Compound(c) = tag
    {
        c.remove(&"leash".into());
    }
}
/// `Entity.load`: a string `Team` adds the entity (`name`, its UUID) to that
/// team when the scoreboard has it; else vanilla only logs a warning.
fn join_saved_team(board: &mut Scoreboard, name: &str, tag: &Tag) -> bool {
    let Some(team) = text(tag.get("Team")) else {
        return false;
    };
    if board.team(&team).is_none() {
        log::warn!("Unable to add mob to team \"{team}\" (that team probably doesn't exist)");
        return false;
    }
    board.add_to_team(name, &team);
    true
}

#[cfg(test)]
#[path = "persist/tests.rs"]
mod tests;
#[cfg(test)]
mod item_display_saved_transform_tests {
    use super::*;
    use crate::game::{Audio, Options, Progress, Settings, commands::Target, load_resources};
    use horizon_client::{chunk::Chunk, motion::PlayerMotion};
    use horizon_core::{geometry::Vec3, position::ChunkPos, random::RandomSource};
    fn with_game(test: impl FnOnce(&mut Game<'_>)) {
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            std::time::Instant::now(),
            crate::game::execution_role::ExecutionRole::IntegratedServer,
            &Progress::new(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.game_dir = Some(
            std::path::PathBuf::from(std::env::var_os("HORIZON_HOME").unwrap())
                .join(format!("h132-{:?}", std::thread::current().id())),
        );
        options.execution_role = crate::game::execution_role::ExecutionRole::IntegratedServer;
        let mut game = Game::new(
            &loaded.resources,
            loaded.pack,
            options,
            settings,
            Audio::default(),
        )
        .unwrap();
        game.native_lifecycle
            .begin_open(mod_api::Authority::IntegratedServer);
        game.build_registries();
        game.set_allow_commands(true);

        game.world
            .chunks
            .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
        game.block_ticks
            .add_empty_chunk(ChunkPos::new(0, 0))
            .unwrap();
        assert!(game.run_command("difficulty normal"));
        assert!(game.run_command("forceload add 0 0"));
        for (x, z) in [(0, 1), (1, 0), (1, 1)] {
            game.world
                .chunks
                .insert((x, z), Arc::new(Chunk::empty(x, z)));
            game.block_ticks
                .add_empty_chunk(ChunkPos::new(x, z))
                .unwrap();
        }
        game.player = Some(PlayerMotion::new(game.motion, Vec3::new(2.5, 200.0, 2.5)).unwrap());
        game.creative = true;
        assert!(game.run_command("fill 0 199 0 31 199 31 stone"));
        assert!(
            game.run_command("summon pig 13.5 200 2.5 {UUID:[I;132,0,0,99],NoAI:1b,NoGravity:1b}")
        );
        game.world.random.set_seed(132);
        test(&mut game);
    }
    const IDENTITY: &str = "{translation:[0f,0f,0f],left_rotation:[0f,0f,0f,1f],scale:[1f,1f,1f],right_rotation:[0f,0f,0f,1f]}";
    const CHANGED: &str = "{translation:[1.25f,-2.5f,3f],left_rotation:[0f,1f,0f,0f],scale:[2f,3f,4f],right_rotation:[0f,0f,0f,1f]}";
    fn expected(translation: [f32; 3], left: [f32; 4], scale: [f32; 3]) -> Tag {
        let floats = |v: &[f32]| {
            Tag::List(
                v.iter()
                    .map(|x| Tag::Float(if *x == 0.0 { 0 } else { x.to_bits() }))
                    .collect(),
            )
        };
        Tag::Compound(
            [
                ("translation".into(), floats(&translation)),
                ("left_rotation".into(), floats(&left)),
                ("scale".into(), floats(&scale)),
                ("right_rotation".into(), floats(&[0.0, 0.0, 0.0, 1.0])),
            ]
            .into_iter()
            .collect(),
        )
    }
    fn summon(game: &mut Game<'_>, n: i32, transform: Option<&str>) -> u32 {
        let suffix = transform
            .map(|t| format!(",transformation:{t}"))
            .unwrap_or_default();
        assert!(game.run_command(&format!("summon item_display 6.5 200 6.5 {{UUID:[I;132,0,0,{n}],Rotation:[12.5f,-20f],NoGravity:1b,CustomName:\"display\",Tags:[\"h132\"],item:{{id:\"minecraft:diamond\",count:2,components:{{\"minecraft:custom_data\":{{h132:7}}}}}}{suffix}}}")));
        let id = game
            .ecs
            .species_ids("minecraft:item_display")
            .into_iter()
            .find(|&id| game.persist.uuid_of(id) == Some([132, 0, 0, n]))
            .unwrap();
        assert_eq!(game.ecs.kind(id), Some("minecraft:item_display"));
        assert_eq!(game.ecs.position(id), Some([6.5, 200.0, 6.5]));
        assert_eq!(game.ecs.rotation(id), Some([12.5, -20.0]));
        let saved = game.command_target_nbt(Target::Other(id)).unwrap();
        let stack = horizon_client::save::item_from_nbt(saved.get("item").unwrap()).unwrap();
        assert_eq!(&*stack.item, "minecraft:diamond");
        assert_eq!(stack.count, 2);
        assert_eq!(
            saved
                .get("item")
                .unwrap()
                .get("components")
                .unwrap()
                .get("minecraft:custom_data")
                .unwrap()
                .get("h132"),
            Some(&Tag::Int(7))
        );
        id
    }
    #[test]
    #[ignore = "daily"]
    fn omitted_item_display_transformation_is_saved_as_typed_identity() {
        with_game(|game| {
            let clock = game.world.game_time;
            let id = summon(game, 1, None);
            let identity = expected([0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0; 3]);
            let tag = game.command_target_nbt(Target::Other(id)).unwrap();
            assert_eq!(tag.get("transformation"), Some(&identity));
            let raw = game.persist.raw.remove(&id).unwrap();
            let fresh = game.command_target_nbt(Target::Other(id)).unwrap();
            assert_eq!(fresh.get("transformation"), Some(&identity));
            assert_eq!(fresh.get("UUID"), tag.get("UUID"));
            assert_eq!(fresh.get("Pos"), tag.get("Pos"));
            assert_eq!(fresh.get("interpolation_duration"), Some(&Tag::Int(0)));
            assert_eq!(fresh.get("teleport_duration"), Some(&Tag::Int(0)));
            assert!(fresh.get("start_interpolation").is_none());
            game.persist.raw.insert(id, raw);
            assert_eq!(game.world.game_time, clock);
        });
    }
    #[test]
    #[ignore = "daily"]
    fn admitted_compound_item_transforms_are_actual_saved_state() {
        with_game(|game| {
            let clock = game.world.game_time;
            for (n, text, want) in [
                (
                    2,
                    CHANGED,
                    expected([1.25, -2.5, 3.0], [0.0, 1.0, 0.0, 0.0], [2.0, 3.0, 4.0]),
                ),
                (
                    3,
                    IDENTITY,
                    expected([0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0; 3]),
                ),
            ] {
                let id = summon(game, n, Some(text));
                let before = game.command_target_nbt(Target::Other(id)).unwrap();
                assert_eq!(before.get("transformation"), Some(&want));
                let raw = game.persist.raw.remove(&id).unwrap();
                let fresh = game.command_target_nbt(Target::Other(id)).unwrap();
                assert_eq!(
                    fresh.get("transformation"),
                    Some(&want),
                    "real loaded state survives without raw"
                );
                game.persist.raw.insert(id, raw);
            }
            assert_eq!(game.world.game_time, clock);
        });
    }
    #[test]
    #[ignore = "daily"]
    fn existing_native_item_display_setter_carries_only_transform_state() {
        use mod_api::Transformation;
        with_game(|game| {
            // Same actual server-context setup as existing native machine/contact fixtures.
            game.options.dedicated = Some(Vec::new());
            let id = summon(game, 4, None);
            let uuid = game.persist.uuid_of(id);
            let clock = game.world.game_time;
            let update = Transformation {
                translation: [9.0, 8.0, 7.0],
                scale: [2.0; 3],
                ..Transformation::IDENTITY
            };
            let display = game.display(game.entity(id).unwrap()).unwrap();
            assert!(game.display_set_transformation(display, update));
            assert!(game.display_set_interpolation_duration(display, 13));
            assert!(game.display_set_teleport_duration(display, 5));
            assert!(game.display_set_start_interpolation(display, 0));
            let raw = game.persist.raw.get_mut(&id).unwrap();
            assert_eq!(raw.get("interpolation_duration"), Some(&Tag::Int(13)));
            assert_eq!(raw.get("teleport_duration"), Some(&Tag::Int(5)));
            assert_eq!(raw.get("start_interpolation"), Some(&Tag::Int(0)));
            let Tag::Compound(c) = raw else {
                panic!("compound")
            };
            c.insert(
                "transformation".into(),
                expected([0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0; 3]),
            );
            assert!(game.display_set_shadow_radius(display, 1.25));
            let want = expected([9.0, 8.0, 7.0], [0.0, 0.0, 0.0, 1.0], [2.0; 3]);
            assert_eq!(
                game.command_target_nbt(Target::Other(id))
                    .unwrap()
                    .get("transformation"),
                Some(&want)
            );
            let raw = game.persist.raw.remove(&id).unwrap();
            let fresh = game.command_target_nbt(Target::Other(id)).unwrap();
            assert_eq!(fresh.get("transformation"), Some(&want));
            assert_eq!(fresh.get("interpolation_duration"), Some(&Tag::Int(0)));
            assert_eq!(fresh.get("teleport_duration"), Some(&Tag::Int(0)));
            assert!(fresh.get("start_interpolation").is_none());
            game.persist.raw.insert(id, raw);
            let invalid = Transformation {
                left_rotation: [0.0; 4],
                ..update
            };
            let before = game.persist.raw.get(&id).unwrap().clone();
            assert!(!game.display_set_transformation(display, invalid));
            assert_eq!(game.persist.raw.get(&id), Some(&before));
            assert_eq!(
                game.command_target_nbt(Target::Other(id))
                    .unwrap()
                    .get("transformation"),
                Some(&want)
            );
            assert_eq!(game.persist.uuid_of(id), uuid);
            assert_eq!(game.world.game_time, clock);
        });
    }
}

#[cfg(test)]
#[path = "persist/summon_uuid_conflict_tests.rs"]
mod summon_uuid_conflict_tests;

#[cfg(test)]
mod sulfur_body_attribute_tests {
    use super::*;
    use crate::game::{Audio, Options, Progress, Settings, load_resources};
    use horizon_client::motion::PlayerMotion;

    fn with_game(test: impl FnOnce(&mut Game<'_>)) {
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            std::time::Instant::now(),
            crate::game::execution_role::ExecutionRole::IntegratedServer,
            &Progress::new(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.game_dir = Some(
            std::path::PathBuf::from(std::env::var_os("HORIZON_HOME").unwrap())
                .join(format!("h192-{:?}", std::thread::current().id())),
        );
        options.execution_role = crate::game::execution_role::ExecutionRole::IntegratedServer;
        let mut game = Game::new(
            &loaded.resources,
            loaded.pack,
            options,
            settings,
            Audio::default(),
        )
        .unwrap();
        game.build_registries();
        game.set_allow_commands(true);
        game.world
            .chunks
            .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
        game.block_ticks
            .add_empty_chunk(ChunkPos::new(0, 0))
            .unwrap();
        assert!(game.run_command("forceload add 0 0"));
        assert!(game.run_command("difficulty normal"));
        assert!(game.run_command("scoreboard objectives add h192 dummy"));
        game.player = Some(PlayerMotion::new(game.motion, Vec3::new(2.5, 200.0, 2.5)).unwrap());
        game.creative = true;
        game.load_ticking_neighbourhood(0, 0);
        test(&mut game);
    }
    /// `/attribute <cube> <attribute> get 1000`, read back through a score.
    fn read(game: &mut Game<'_>, n: i32, attribute: &str) -> i32 {
        let sel = format!("@e[type=minecraft:sulfur_cube,nbt={{UUID:[I;192,0,0,{n}]}},limit=1]");
        assert!(game.run_command(&format!(
            "execute store result score #v h192 run attribute {sel} minecraft:{attribute} get 1000"
        )));
        (-3000..=3000)
            .find(|v| game.run_command_quietly(&format!("execute if score #v h192 matches {v}")))
            .unwrap_or(i32::MIN)
    }

    /// Expected values from the official 26.3 server: a sulfur cube reads its
    /// defaults until a tick adopts its body (collectEquipmentChanges); then the
    /// body's archetype modifiers count (oak planks: bouncy).
    #[test]
    #[ignore = "daily"]
    fn a_sulfur_cube_reads_its_body_archetype_modifiers_after_a_tick() {
        with_game(|game| {
            for (n, body) in [
                (1, ",equipment:{body:{id:\"minecraft:oak_planks\"}}"),
                (2, ""),
            ] {
                assert!(game.run_command(&format!(
                        "summon minecraft:sulfur_cube {}.5 200 4.5 {{UUID:[I;192,0,0,{n}],NoAI:1b,NoGravity:1b,Silent:1b,PersistenceRequired:1b,Size:1{body}}}",
                        3 * n
                    )));
            }
            let attrs = [
                "knockback_resistance",
                "explosion_knockback_resistance",
                "bounciness",
                "friction_modifier",
                "air_drag_modifier",
            ];
            let defaults = [0, 0, 0, 1000, 1000];
            let mut failures = Vec::new();
            for (n, label) in [(1, "body"), (2, "none")] {
                for (a, d) in attrs.iter().zip(defaults) {
                    let got = read(game, n, a);
                    if got != d {
                        failures.push(format!("{label} {a} before a tick: {got}, expected {d}"));
                    }
                }
            }
            game.tick_world(true).unwrap();
            game.tick_world(true).unwrap();
            // Bouncy: -2, -2 clamped to 0, +0.9, x0.3, x0.01.
            let bouncy = [-2000, 0, 899, 300, 9];
            for (n, label, want) in [(1, "body", bouncy), (2, "none", defaults)] {
                for (a, d) in attrs.iter().zip(want) {
                    let got = read(game, n, a);
                    if got != d {
                        failures.push(format!("{label} {a} after two ticks: {got}, expected {d}"));
                    }
                }
            }
            assert!(failures.is_empty(), "sulfur body attributes: {failures:#?}");
        });
    }
}

#[cfg(test)]
mod summoned_knot_tests {
    use super::*;
    use crate::game::{Audio, Options, Progress, Settings, commands::Target, load_resources};
    use horizon_client::{chunk::Chunk, motion::PlayerMotion};
    use horizon_core::{geometry::Vec3, position::ChunkPos, random::RandomSource};
    fn with_game(test: impl FnOnce(&mut Game<'_>)) {
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            std::time::Instant::now(),
            crate::game::execution_role::ExecutionRole::IntegratedServer,
            &Progress::new(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.game_dir = Some(
            std::path::PathBuf::from(std::env::var_os("HORIZON_HOME").unwrap())
                .join(format!("h129-{:?}", std::thread::current().id())),
        );
        options.execution_role = crate::game::execution_role::ExecutionRole::IntegratedServer;
        let mut game = Game::new(
            &loaded.resources,
            loaded.pack,
            options,
            settings,
            Audio::default(),
        )
        .unwrap();
        game.build_registries();
        game.set_allow_commands(true);

        game.world
            .chunks
            .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
        game.block_ticks
            .add_empty_chunk(ChunkPos::new(0, 0))
            .unwrap();
        assert!(game.run_command("difficulty normal"));
        assert!(game.run_command("forceload add 0 0"));
        for (x, z) in [(0, 1), (1, 0), (1, 1)] {
            game.world
                .chunks
                .insert((x, z), Arc::new(Chunk::empty(x, z)));
            game.block_ticks
                .add_empty_chunk(ChunkPos::new(x, z))
                .unwrap();
        }
        game.player = Some(PlayerMotion::new(game.motion, Vec3::new(2.5, 200.0, 2.5)).unwrap());
        game.creative = true;
        assert!(game.run_command("fill 0 199 0 31 199 31 stone"));
        assert!(
            game.run_command("summon pig 13.5 200 2.5 {UUID:[I;129,0,0,99],NoAI:1b,NoGravity:1b}")
        );
        game.world.random.set_seed(129);
        game.load_ticking_neighbourhood(0, 0);
        test(&mut game);
    }
    /// The network ids of the ECS entities of `kind`, in id order.
    fn ecs_ids(game: &Game<'_>, kind: &str) -> Vec<u32> {
        let Some(w) = game.ecs.world() else { return Vec::new() };
        let q = horizon_ecs_proto::bridge::host::Select { kind: Some(kind.into()), tag: None, near: None, limit: usize::MAX };
        let mut ids: Vec<u32> = game.ecs.select(&q).into_iter().filter_map(|e| horizon_ecs_proto::bridge::ids::id_of(w, e)).collect();
        ids.sort_unstable();
        ids
    }
    /// The UUID the ECS entity `id` saves.
    fn ecs_uuid(game: &Game<'_>, id: u32) -> Option<[i32; 4]> {
        let c = game.ecs.entity_nbt(game.ecs.entity_of(id)?)?;
        horizon_client::entity_nbt::uuid(&Tag::Compound(c))
    }
    fn knot_id(game: &Game<'_>, n: i32) -> u32 {
        ecs_ids(game, "minecraft:leash_knot").into_iter().find(|&id| ecs_uuid(game, id) == Some([129, 0, 0, n])).unwrap()
    }
    fn summon_knots(game: &mut Game<'_>) -> [u32; 3] {
        for x in [6, 14] {
            assert!(game.run_command(&format!("setblock {x} 200 6 oak_fence strict")));
        }
        for (n, x) in [(1, 6), (2, 10), (3, 14)] {
            assert!(game.run_command(&format!("summon leash_knot {x}.2 200.8 6.9 {{UUID:[I;129,0,0,{n}],CustomName:\"knot\",Tags:[\"h129\"]}}")));
        }
        [1, 2, 3].map(|n| knot_id(game, n))
    }
    #[test]
    #[ignore = "daily"]
    fn summoned_knots_adopt_normalized_position_and_save_common_defaults() {
        with_game(|game| {
            let clock = game.world.game_time;
            let ids = summon_knots(game);
            for (i, id) in ids.into_iter().enumerate() {
                assert_eq!(game.ecs.kind(id), Some("minecraft:leash_knot"));
                assert_eq!(ecs_uuid(game, id), Some([129, 0, 0, i as i32 + 1]));
                let expected = Vec3::new(6.5 + 4.0 * i as f64, 200.375, 6.5);
                assert_eq!(
                    game.ecs.position(id).map(|p| Vec3::new(p[0], p[1], p[2])), Some(expected),
                    "actual loaded model, not a getter-only answer"
                );
                let tag = game.command_target_nbt(Target::Other(id)).unwrap();
                assert_eq!(
                    tag.get("Pos"),
                    Some(&Tag::List(
                        expected
                            .components()
                            .map(|v| Tag::Double(v.to_bits()))
                            .to_vec()
                    ))
                );
                for (key, value) in [
                    ("Air", Tag::Short(300)),
                    ("Invulnerable", Tag::Byte(0)),
                    ("OnGround", Tag::Byte(0)),
                    ("PortalCooldown", Tag::Int(0)),
                    ("fall_distance", Tag::Double(0.0f64.to_bits())),
                ] {
                    assert_eq!(tag.get(key), Some(&value), "{key}");
                }
                assert_eq!(
                    tag.get("Tags"),
                    Some(&Tag::List(vec![Tag::String("h129".into())]))
                );
                let fresh = game.command_target_nbt(Target::Other(id)).unwrap();
                assert_eq!(fresh.get("Air"), Some(&Tag::Short(300)));
                assert_eq!(fresh.get("Pos"), tag.get("Pos"));
            }
            assert_eq!(game.world.game_time, clock);
        });
    }
    #[test]
    #[ignore = "daily"]
    fn negative_fractional_knot_position_uses_floor_before_centering() {
        with_game(|game| {
            // Chunk (-1, -1) is already loaded by the fixture's neighbourhood.
            assert!(game.world.chunks.contains_key(&(-1, -1)));
            assert!(game.run_command("setblock -2 199 -3 stone strict"));
            assert!(game.run_command("setblock -2 200 -3 oak_fence strict"));
            assert!(game.run_command("summon leash_knot -1.2 200.8 -2.1 {UUID:[I;129,0,0,5]}"));
            let id = knot_id(game, 5);
            assert_eq!(game.ecs.position(id), Some([-1.5, 200.375, -2.5]));
        });
    }
    #[test]
    #[ignore = "daily"]
    fn explicit_knot_check_counts_only_running_entity_ticking_turns() {
        with_game(|game| {
            let ids = summon_knots(game);
            let clock = game.world.game_time;
            for _ in 0..100 {
                game.tick().unwrap();
            }
            assert!(ids.iter().all(|&id| game.ecs.exists(id)));
            assert!(game.run_command("tick freeze"));
            for _ in 0..3 {
                game.tick().unwrap();
            }
            assert_eq!(game.world.game_time, clock + 100);
            assert!(ids.iter().all(|&id| game.ecs.exists(id)));
            assert!(game.run_command("gamerule spectators_generate_chunks false"));
            assert!(game.run_command("gamemode spectator @s"));
            assert!(game.run_command("forceload remove all"));
            assert!(game.run_command("tick unfreeze"));
            for _ in 0..3 {
                game.tick().unwrap();
                assert!(!game.entity_ticking_area()(10, 6));
            }
            assert_eq!(game.world.game_time, clock + 103);
            assert!(ids.iter().all(|&id| game.ecs.exists(id)));
            assert!(game.run_command("gamemode creative @s"));
            assert!(game.run_command("forceload add 0 0"));
            game.tick().unwrap();
            assert!(game.entity_ticking_area()(10, 6));
            assert!(game.ecs.exists(ids[0]));
            assert!(
                !game.ecs.exists(ids[1]),
                "101 eligible ticks, not elapsed world time"
            );
        });
    }
}
