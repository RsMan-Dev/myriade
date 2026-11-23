//! Mobs: entity types (dimensions and default attributes exported from vanilla), models (vanilla
//! `LayerDefinitions` baked part trees) and the `Mob` entity with vanilla's AI framework: goal and
//! target selectors with priorities and control flags (`goals`), A* pathfinding with walk, swim,
//! fly and amphibious node evaluators (`path`, `nav`), move/look/jump/body controls (`control`),
//! `LivingEntity` physics (`living`) and per-species behaviour, models, animations and sounds.
pub mod ai;
mod ai_pause;
pub mod allay;
pub mod ambient;
pub mod animal_brain;
pub mod aquatic;
pub mod armadillo;
pub mod armor;
pub mod axolotl;
pub mod bat;
pub mod bee;
pub mod brain;
pub mod breeze;
pub mod camel;
pub mod caravan;
pub mod combat;
pub mod conversion;
pub mod copper_golem;
pub mod creaking;
pub mod creaking_heart;
pub mod cube;
pub mod dab;
pub mod data;
pub mod dolphin;
pub mod doors;
pub mod drops;
pub mod enderman;
pub mod entity_sounds;
pub mod equine;
pub mod equip;
pub mod farm;
pub mod finalize;
pub mod fox;
pub mod frog;
pub mod gear;
pub mod goals;
pub mod goat;
pub mod golem;
pub mod guardian;
pub mod happy_ghast;
pub mod hostile;
mod hurt_by;
pub mod illager;
pub mod illusioner;
pub mod invulnerable;
pub mod item_break;
pub mod keyframes;
pub mod leash;
pub mod living;
pub mod llama;
mod mob_slot;
pub mod monster_brain;
mod model;
pub mod nautilus;
pub mod neutral_anger;
pub use mob_slot::MobSlot;
mod push;
mod type_attributes;
pub use neutral_anger::{
    adopt_anger_end, anger_left, persistent_anger, resume_anger, settle_anger_end,
};
mod hit_attempt;
mod knockback;
pub mod nav;
pub mod nether;
pub mod panda;
pub mod parrot;
pub mod path;
pub mod phantom;
pub mod piglin;
pub mod player_selection;
pub mod polar_bear;
pub mod rabbit;
pub mod raider;
pub mod ravager;
pub mod riders;
pub mod cow_memo;
pub mod cow_rest;
pub mod seen_grid;
pub mod sheep_dye;
pub mod shulker;
pub mod sniffer;
pub mod spawn_rules;
pub mod spawn_place;
pub mod spawner;
pub mod spawning;
pub mod spear_use;
pub mod species;
pub mod species_slots;
pub mod strider;
pub mod stuck;
pub mod suffocation;
pub mod sulfur_cube;
pub mod sync;
pub mod tame;
mod target;
pub mod trader;
pub mod transport_items;
pub mod trial_spawner;
pub mod turtle;
pub mod undead;
pub mod vex;
pub mod villager;
pub mod villager_brain;
pub mod visibility;
pub use visibility::mob_visibility;
pub mod warden;
pub mod warden_brain;
pub mod witch;
pub mod wither;

pub mod world;
pub use world::{TargetInfo, VisualShape, single_player_pos, visual_clip, visual_shape};
pub mod zombie_villager;
/// The dye colours in `DyeColor`'s order.
pub const DYE_NAMES: [&str; 16] = [
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light_gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];
/// The villager types in `VillagerType`'s order.
pub const VILLAGER_TYPES: [&str; 7] = [
    "plains", "desert", "jungle", "savanna", "snow", "swamp", "taiga",
];

/// A mob's stored active effect with its registry index resolved once at creation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StoredEffect {
    pub id: &'static str,
    pub duration: i32,
    pub amplifier: i32,
    pub registry_index: Option<usize>,
    /// `MobEffectInstance.showIcon`, kept apart from the particle flag: the saved `show_icon`.
    pub show_icon: bool,
}
impl StoredEffect {
    /// `new MobEffectInstance(effect, duration, amplifier)`: its icon shows.
    pub fn new(id: &'static str, duration: i32, amplifier: i32) -> Self {
        Self {
            id,
            duration,
            amplifier,
            registry_index: horizon_client_core::potions::effect(id).map(|def| def.index),
            show_icon: true,
        }
    }
    /// Copy a registered instance after an update or hidden-effect promotion, without resolving again.
    pub fn from_instance(id: &'static str, instance: &crate::effects::EffectInstance) -> Self {
        Self {
            id,
            duration: instance.duration,
            amplifier: instance.amplifier,
            registry_index: instance.registry_index,
            show_icon: instance.show_icon,
        }
    }
}

use crate::{
    blocks::BlockFacts,
    health::{DamageContainer, DamageLevel, Reduction},
    world::World,
};
pub use data::MobData;
use horizon_core::{
    geometry::{Aabb, Vec3},
    random::{EntityRandom, LegacyRandom, RandomSource},
};
use horizon_world::block_collision::BlockCollisionRegistry;
use horizon_world::native_load_context::LoopContext;
use serde_json::Value;
use std::{collections::HashMap, fs, path::Path, sync::Arc};

pub use model::{Part, Polygon};

/// Kept for callers of the earlier simple AI; species now select behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Brain {
    Passive,
    Chicken,
    Zombie,
    Skeleton,
    Spider,
    Creeper,
}
/// Model animation family (vanilla `EntityModel.setupAnim` implementations).
pub use anim::Anim;
pub use species::Species;

mod breeding;
pub use breeding::{draws_parent_variant, orb_after_child};

mod collision_subject;
mod flags;
pub use flags::Flags;

#[derive(Clone, Debug)]
pub struct MobType {
    pub id: String,
    pub species: Species,
    pub width: f64,
    pub height: f64,
    pub eye_height: f64,
    pub category: String,
    pub max_health: f32,
    pub speed: f64,
    pub flying_speed: f64,
    pub attack_damage: f32,
    pub attack_knockback: f64,
    pub follow_range: f64,
    pub step_height: f64,
    pub armor: f32,
    pub armor_toughness: f32,
    pub knockback_resistance: f64,
    pub jump_strength: f64,
    /// Every default attribute value of the type (`entity-types.json`).
    pub attributes: Arc<HashMap<String, f64>>,
    /// Item defaults used when loading inventory-bearing mobs before a Game exists.
    pub item_stack_limits: Arc<HashMap<String, u32>>,
    /// The bounds `finalize`'s attribute record clamps to, from the catalog.
    pub attribute_bounds: finalize::AttrBounds,
    pub gravity: f64,
    pub safe_fall_distance: f64,
    pub fall_damage_multiplier: f64,
    pub water_efficiency: f64,
    pub water_slowdown: f32,
    /// Air speed for travelFlying mobs (ghast 0.02, phantom 0.2).
    pub air_speed: f32,
    pub baby_scale: f64,
    pub head_rot_speed: f32,
    pub max_head_x_rot: f32,
    pub max_head_y_rot: f32,
    pub ambient_interval: i32,
    pub flags: Flags,
    pub model: Arc<Part>,
    /// Every baked layer of this entity's models keyed by `model#layer`
    /// (e.g. `sheep#wool`, `zombie_baby#main`).
    pub layers: crate::fast_hash::FxHashMap<String, Arc<Part>>,
    pub texture: String,
    pub brain: Brain,
    pub anim: Anim,
    pub sounds: [&'static str; 3],
    pub sound_overrides: entity_sounds::Sounds,
    /// Animal.isFood items (resolved from the item tag).
    pub food: Arc<[String]>,
    /// TemptGoal items when they differ from the food.
    pub tempt: Arc<[String]>,
    pub passenger_attachments: Vec<[f32; 3]>,
}
impl MobType {
    pub fn inventory_stack_limit(&self, stack: &crate::items::ItemStack) -> u32 {
        stack
            .data()
            .and_then(|d| d.other("minecraft:max_stack_size"))
            .and_then(|t| match t {
                horizon_core::nbt::Tag::Int(n) => u32::try_from(*n).ok(),
                _ => None,
            })
            .unwrap_or_else(|| {
                self.item_stack_limits
                    .get(stack.item.as_ref())
                    .copied()
                    .unwrap_or(64)
            })
    }

    pub fn layer(&self, key: &str) -> Option<&Arc<Part>> {
        self.layers.get(key)
    }
}
/// Each type's default attribute values, parsed exactly (serde_json's
/// default float parsing can miss the nearest double by one ulp).
pub(crate) fn exact_attributes(bytes: &[u8]) -> HashMap<String, HashMap<String, f64>> {
    use serde_json::value::RawValue;
    type Raw = HashMap<String, Box<RawValue>>;
    let Ok(types) = serde_json::from_slice::<Raw>(bytes) else {
        return HashMap::new();
    };
    types
        .into_iter()
        .filter_map(|(id, t)| {
            let t: Raw = serde_json::from_str(t.get()).ok()?;
            let attrs: Raw = serde_json::from_str(t.get("attributes")?.get()).ok()?;
            let values = attrs
                .into_iter()
                .filter_map(|(k, v)| v.get().trim().parse::<f64>().ok().map(|v| (k, v)))
                .collect();
            Some((id, values))
        })
        .collect()
}

pub struct MobTypes {
    pub types: HashMap<String, MobType>,
}
impl MobTypes {
    /// The types with the attribute catalog of the same resources
    /// (`attributes.json`), for a caller without the game's own.
    pub fn load(root: &Path) -> crate::Result<Self> {
        Self::load_with_models(root, true, None, None, &Self::root_catalog(root)?)
    }

    /// Load client mob data using defaults already decoded by the resource
    /// bootstrap. This avoids decoding `items.json` a second time for Allay
    /// inventory limits.
    pub fn load_with_defaults(
        root: &Path,
        defaults: &horizon_world::item_defaults::Defaults,
        attributes: &horizon_world::attributes::entity::Catalog,
    ) -> crate::Result<Self> {
        Self::load_with_models(root, true, None, Some(defaults), attributes)
    }

    /// The dedicated server needs entity facts and AI, but never their render
    /// geometry. Keep one empty part for the fields shared with the client
    /// rather than parsing and retaining every entity-model layer.
    pub fn load_for_server(
        root: &Path,
        attributes: &horizon_world::attributes::entity::Catalog,
    ) -> crate::Result<Self> {
        Self::load_with_models(root, false, None, None, attributes)
    }

    /// The client startup already parsed `entity-models.json` for the handoff
    /// and the other model loaders; read the model layers from that copy.
    pub fn load_with_document(root: &Path, document: &EntityModelDocument) -> crate::Result<Self> {
        Self::load_with_models(root, true, Some(document), None, &Self::root_catalog(root)?)
    }

    /// The client startup already has the entity model document, the item
    /// defaults and the attribute catalog. Reuse them instead of decoding
    /// their files again.
    pub fn load_with_document_and_defaults(
        root: &Path,
        document: &EntityModelDocument,
        defaults: &horizon_world::item_defaults::Defaults,
        attributes: &horizon_world::attributes::entity::Catalog,
    ) -> crate::Result<Self> {
        Self::load_with_models(root, true, Some(document), Some(defaults), attributes)
    }

    fn root_catalog(root: &Path) -> crate::Result<horizon_world::attributes::entity::Catalog> {
        let json = horizon_world::json_file::read(&root.join("attributes.json"))?;
        Ok(horizon_world::attributes::entity::Catalog::from_json(
            &json,
        )?)
    }

    /// `attributes`: the catalog whose bounds the types' mobs clamp to.
    fn load_with_models(
        root: &Path,
        models_needed: bool,
        document: Option<&EntityModelDocument>,
        item_defaults: Option<&horizon_world::item_defaults::Defaults>,
        attributes: &horizon_world::attributes::entity::Catalog,
    ) -> crate::Result<Self> {
        let attribute_bounds = finalize::AttrBounds::of(attributes);
        let bytes = fs::read(root.join("entity-types.json"))?;
        let types: Value = serde_json::from_slice(&bytes)?;
        let exact = exact_attributes(&bytes);
        let models = models_needed
            .then(|| -> crate::Result<EntityModelDocument> {
                match document {
                    Some(document) => Ok(document.clone()),
                    None => EntityModelDocument::load(root),
                }
            })
            .transpose()?;
        let entity_type_tags =
            crate::items::load_tags(root, "entity_type", &horizon_world::loot::PackOverlay::NONE)?;
        let empty_model = Arc::new(Part::parse("root", &Value::Null));
        let item_tags =
            crate::items::load_tags(root, "item", &horizon_world::loot::PackOverlay::NONE)
                .unwrap_or_default();
        sulfur_cube::load_archetype_items(&item_tags);
        let tag = |name: &str| -> Arc<[String]> {
            let mut v: Vec<String> = item_tags
                .get(&format!("minecraft:{name}"))
                .into_iter()
                .flatten()
                .cloned()
                .collect();
            v.sort();
            v.into()
        };
        // The Allay's InventoryCarrier loading needs item limits before a
        // Game exists. Other mob-only resource sets need no item data.
        let item_stack_limits: Arc<HashMap<String, u32>> = if types.get("minecraft:allay").is_some()
        {
            match item_defaults {
                Some(items) => Arc::new(
                    items
                        .iter()
                        .map(|(id, components)| {
                            (
                                id.clone(),
                                components["minecraft:max_stack_size"]
                                    .as_u64()
                                    .unwrap_or(64) as u32,
                            )
                        })
                        .collect(),
                ),
                None => {
                    let items: HashMap<String, Value> =
                        serde_json::from_slice(&fs::read(root.join("items.json"))?)?;
                    Arc::new(
                        items
                            .into_iter()
                            .map(|(id, components)| {
                                (
                                    id,
                                    components["minecraft:max_stack_size"]
                                        .as_u64()
                                        .unwrap_or(64) as u32,
                                )
                            })
                            .collect(),
                    )
                }
            }
        } else {
            Arc::new(HashMap::new())
        };
        let mut out = HashMap::new();
        // A content mod's living type (its row names a vanilla `template`) is
        // its template's mob under its own id: the template's model, sounds
        // and behaviour, with the attributes its row has.
        let mod_types = types
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(id, row)| {
                let template = row["template"].as_str()?.strip_prefix("minecraft:")?;
                let def = species::ALL.iter().find(|d| d.name == template)?;
                Some((id.clone(), def))
            });
        let vanilla = species::ALL
            .iter()
            .map(|def| (format!("minecraft:{}", def.name), def));
        for (id, def) in vanilla.chain(mod_types) {
            let t = &types[&id];
            if t.is_null() {
                continue;
            }
            let model_key = format!("minecraft:{}", def.model);
            let model = models
                .as_ref()
                .and_then(|models| models.value().get(&model_key));
            if models_needed && model.is_none() {
                continue;
            }
            let defaults = exact.get(&id).cloned().unwrap_or_default();
            let fields = MobType::catalog_fields(&defaults, attributes);
            let mut layers = crate::fast_hash::FxHashMap::default();
            if let Some(all) = models.as_ref().and_then(|m| m.value().as_object()) {
                for prefix in def.layer_prefixes() {
                    let start = format!("minecraft:{prefix}#");
                    for (k, v) in all.iter().filter(|(k, _)| k.starts_with(&start)) {
                        let part = Part::parse("root", v);
                        layers.insert(
                            k.trim_start_matches("minecraft:").to_owned(),
                            Arc::new(part),
                        );
                    }
                }
            }
            let category = t["category"].as_str().unwrap_or("creature").to_owned();
            let mut flags = def.flags(t["fire_immune"].as_bool().unwrap_or(false));
            flags.powder_snow_walkable = entity_type_tags
                .get("minecraft:powder_snow_walkable_mobs")
                .is_some_and(|members| members.contains(&id));
            let passenger_attachments =
                riders::parse_passenger_attachments(&t["passenger_attachments"]);
            out.insert(
                id.clone(),
                MobType {
                    species: def.species,
                    item_stack_limits: item_stack_limits.clone(),
                    attribute_bounds,
                    width: t["width"].as_f64().unwrap_or(0.6),
                    height: t["height"].as_f64().unwrap_or(1.8),
                    eye_height: t["eye_height"].as_f64().unwrap_or(1.6),
                    max_health: fields.max_health,
                    speed: fields.speed,
                    flying_speed: fields.flying_speed,
                    attack_damage: fields.attack_damage,
                    attack_knockback: fields.attack_knockback,
                    follow_range: fields.follow_range,
                    step_height: fields.step_height,
                    armor: fields.armor,
                    armor_toughness: fields.armor_toughness,
                    knockback_resistance: fields.knockback_resistance,
                    jump_strength: fields.jump_strength,
                    attributes: Arc::new(defaults.clone()),
                    gravity: fields.gravity,
                    safe_fall_distance: fields.safe_fall_distance,
                    fall_damage_multiplier: fields.fall_damage_multiplier,
                    water_efficiency: fields.water_efficiency,
                    water_slowdown: def.water_slowdown(),
                    air_speed: def.air_speed(),
                    baby_scale: def.baby_scale(),
                    head_rot_speed: def.head_rot_speed(),
                    max_head_x_rot: def.max_head_x_rot(),
                    max_head_y_rot: def.max_head_y_rot(),
                    ambient_interval: def.ambient_interval(),
                    flags,
                    model: model.map_or_else(
                        || empty_model.clone(),
                        |model| {
                            Arc::new(Part::parse("root", model))
                        },
                    ),
                    layers,
                    texture: def.texture.to_owned(),
                    brain: def.brain(),
                    anim: def.anim,
                    sounds: def.sounds,
                    sound_overrides: entity_sounds::Sounds::load(&id, t)?,
                    food: def.food_tag().map_or_else(|| Arc::from(Vec::new()), tag),
                    tempt: def
                        .tempt_tag()
                        .or(def.food_tag())
                        .map_or_else(|| Arc::from(Vec::new()), tag),
                    passenger_attachments,
                    category,
                    id,
                },
            );
        }
        Ok(Self { types: out })
    }
}
pub use horizon_client_core::who::Who;
/// A native damage type's registry name and the tags consumed by mob combat.
///
/// Native code supplies a static name and flags matching its data-pack damage
/// tags; `NamedDamage::registered` builds both from a damage registry entry,
/// interning its name. Register the same name/data for player damage, death
/// messages and loot predicates. Unset tags are false.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NamedDamage {
    pub name: &'static str,
    pub tags: u32,
}
impl NamedDamage {
    pub const IS_FIRE: u32 = 1 << 0;
    pub const BYPASSES_ARMOR: u32 = 1 << 1;
    pub const NO_KNOCKBACK: u32 = 1 << 2;
    pub const BYPASSES_INVULNERABILITY: u32 = 1 << 3;
    pub const BYPASSES_EFFECTS: u32 = 1 << 4;
    pub const BYPASSES_RESISTANCE: u32 = 1 << 5;
    pub const PANIC_CAUSES: u32 = 1 << 6;
    pub const IS_PROJECTILE: u32 = 1 << 7;
    pub const IS_EXPLOSION: u32 = 1 << 8;
    pub const IS_FALL: u32 = 1 << 9;
    pub const BYPASSES_ENCHANTMENTS: u32 = 1 << 10;

    /// Each flag with the damage tag it stands for.
    const TAG_FLAGS: [(u32, &'static str); 11] = [
        (Self::IS_FIRE, "minecraft:is_fire"),
        (Self::BYPASSES_ARMOR, "minecraft:bypasses_armor"),
        (Self::NO_KNOCKBACK, "minecraft:no_knockback"),
        (
            Self::BYPASSES_INVULNERABILITY,
            "minecraft:bypasses_invulnerability",
        ),
        (Self::BYPASSES_EFFECTS, "minecraft:bypasses_effects"),
        (Self::BYPASSES_RESISTANCE, "minecraft:bypasses_resistance"),
        (Self::PANIC_CAUSES, "minecraft:panic_causes"),
        (Self::IS_PROJECTILE, "minecraft:is_projectile"),
        (Self::IS_EXPLOSION, "minecraft:is_explosion"),
        (Self::IS_FALL, "minecraft:is_fall"),
        (
            Self::BYPASSES_ENCHANTMENTS,
            "minecraft:bypasses_enchantments",
        ),
    ];

    pub const fn new(name: &'static str, tags: u32) -> Self {
        Self { name, tags }
    }

    /// The registered damage type `name` with the tags `catalog` gives it,
    /// or None when the registry has no such type. The name is interned: the
    /// registries hold a handful of ids, so one copy of each is kept for the
    /// rest of the run.
    pub fn registered(catalog: &horizon_world::damage_source::Catalog, name: &str) -> Option<Self> {
        use std::{collections::HashSet, sync::Mutex};
        static NAMES: Mutex<Option<HashSet<&'static str>>> = Mutex::new(None);
        let id = catalog.id(name)?;
        let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
        let names = names.get_or_insert_with(HashSet::new);
        let name = match names.get(name) {
            Some(&found) => found,
            None => {
                let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
                names.insert(leaked);
                leaked
            }
        };
        let tags = Self::TAG_FLAGS
            .iter()
            .filter(|(_, tag)| catalog.in_tag(id, tag))
            .fold(0, |tags, (flag, _)| tags | flag);
        Some(Self { name, tags })
    }
    pub const fn has(self, tag: u32) -> bool {
        self.tags & tag != 0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageKind {
    /// A registered native damage type; vanilla kinds keep their existing tags.
    Named(NamedDamage),
    PlayerAttack,
    /// `mace_smash`: a mace hit after a fall (the player attack tags too).
    MaceSmash,
    /// `spear`: a stab of a spear (the player attack tags too).
    Spear,
    MobAttack,
    /// `mob_attack_no_aggro`: a goat's ram, the one type in `#no_anger`.
    MobAttackNoAggro,
    Arrow,
    Thrown,
    Fireball,
    Explosion,
    Fall,
    /// `fly_into_wall`: gliding into a horizontal collision.
    FlyIntoWall,
    FallingBlock,
    FallingStalactite,
    OnFire,
    InFire,
    /// `campfire`: standing in a lit campfire (`CampfireBlock.entityInside`).
    Campfire,
    Lava,
    Drown,
    /// `dry_out`: an axolotl out of water for too long.
    DryOut,
    Magic,
    /// `indirect_magic`: a thrown potion's hit (`DamageSources.indirectMagic`),
    /// which, unlike `magic`, knocks back.
    IndirectMagic,
    Sting,
    Cactus,
    /// `sweet_berry_bush`.
    SweetBerryBush,
    Generic,
    InWall,
    Thorns,
    SonicBoom,
    /// `freeze`: a fully frozen mob in powder snow.
    Freeze,
    /// `fell_out_of_world`: below the world's floor.
    OutOfWorld,
    /// `cramming`: crushed by the mobs around it.
    Cramming,
    /// `falling_anvil`: an anvil landing on it (it damages a helmet).
    FallingAnvil,
    /// `fireworks`: a firework rocket's burst (an explosion for Blast Protection).
    Fireworks,
    /// `generic_kill`: a kill that bypasses invulnerability.
    GenericKill,
    /// `lightning_bolt`.
    LightningBolt,
    /// `outside_border`: past the world border.
    OutsideBorder,
    /// `spit`: a llama's spit.
    Spit,
    /// `stalagmite`: a fall onto a stalagmite (a fall for Feather Falling).
    Stalagmite,
    /// `starve`: no food left.
    Starve,
    /// `hot_floor`: stepping on a magma block.
    HotFloor,
    /// `sulfur_cube_hot`: a sulfur cube's hot floor (a fire).
    SulfurCubeHot,
    /// `wind_charge`: a wind charge's burst (a projectile).
    WindCharge,
    /// `wither`: the wither effect.
    Wither,
    /// `wither_skull`: an owned wither skull's hit (`DamageSources.witherSkull`).
    WitherSkull,
}
impl DamageKind {
    /// DamageTypeTags.BYPASSES_INVULNERABILITY
    pub fn bypasses_invulnerability(self) -> bool {
        if let Self::Named(named) = self {
            return named.has(NamedDamage::BYPASSES_INVULNERABILITY);
        }
        matches!(self, DamageKind::OutOfWorld | DamageKind::GenericKill)
    }
    /// DamageTypeTags.BYPASSES_EFFECTS
    pub fn bypasses_effects(self) -> bool {
        if let Self::Named(named) = self {
            return named.has(NamedDamage::BYPASSES_EFFECTS);
        }
        matches!(self, DamageKind::Starve)
    }
    /// DamageTypeTags.BYPASSES_RESISTANCE
    pub fn bypasses_resistance(self) -> bool {
        if let Self::Named(named) = self {
            return named.has(NamedDamage::BYPASSES_RESISTANCE);
        }
        matches!(self, DamageKind::OutOfWorld | DamageKind::GenericKill)
    }
    /// DamageTypeTags.BYPASSES_ENCHANTMENTS: `sonic_boom` alone in vanilla.
    pub fn bypasses_enchantments(self) -> bool {
        if let Self::Named(named) = self {
            return named.has(NamedDamage::BYPASSES_ENCHANTMENTS);
        }
        matches!(self, DamageKind::SonicBoom)
    }
}
/// `source.getEntity()` of a blow, as `SulfurCube.knockback` reads it: `getEyePosition()`,
/// `getLookAngle()` and `position()`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aim {
    pub eye: [f64; 3],
    pub look: [f64; 3],
    pub feet: [f64; 3],
}
/// A damage source: its type, the attacking entity and where it came from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Damage {
    pub kind: DamageKind,
    pub attacker: Option<Who>,
    pub from: Option<[f64; 3]>,
    pub attacker_species: Option<Species>,
}
impl Damage {
    pub fn fall() -> Self {
        Self {
            kind: DamageKind::Fall,
            attacker: None,
            from: None,
            attacker_species: None,
        }
    }
    pub fn of(kind: DamageKind) -> Self {
        Self {
            kind,
            attacker: None,
            from: None,
            attacker_species: None,
        }
    }
    pub fn player(from: [f64; 3]) -> Self {
        Self {
            kind: DamageKind::PlayerAttack,
            attacker: Some(Who::Player),
            from: Some(from),
            attacker_species: None,
        }
    }
    /// `DamageSources.mace`: a player's smash attack.
    pub fn mace(from: [f64; 3]) -> Self {
        Self {
            kind: DamageKind::MaceSmash,
            attacker: Some(Who::Player),
            from: Some(from),
            attacker_species: None,
        }
    }
    /// `DamageSources.spear`: a player's stab with a spear.
    pub fn spear(from: [f64; 3]) -> Self {
        Self {
            kind: DamageKind::Spear,
            attacker: Some(Who::Player),
            from: Some(from),
            attacker_species: None,
        }
    }
    /// `DamageSources.indirectMagic(potion, owner)`: `attacker` is the owner
    /// (none for a dispenser's potion), `from` the potion's position, which
    /// the knockback pushes away from.
    pub fn indirect_magic(attacker: Option<Who>, from: [f64; 3]) -> Self {
        Self {
            kind: DamageKind::IndirectMagic,
            attacker,
            from: Some(from),
            attacker_species: None,
        }
    }
    /// `DamageSources.fireworks(rocket, owner)`: `attacker` is the owner
    /// (none for a rocket nobody fired), `from` the rocket's position, which
    /// the knockback pushes away from (`calculateHorizontalHurtKnockbackDirection`).
    pub fn fireworks(attacker: Option<Who>, from: [f64; 3]) -> Self {
        Self {
            kind: DamageKind::Fireworks,
            attacker,
            from: Some(from),
            attacker_species: None,
        }
    }
    pub fn mob(id: u32, from: [f64; 3]) -> Self {
        Self {
            kind: DamageKind::MobAttack,
            attacker: Some(Who::Mob(id)),
            from: Some(from),
            attacker_species: None,
        }
    }
    pub fn with_species(mut self, species: Species) -> Self {
        self.attacker_species = Some(species);
        self
    }
    /// DamageTypeTags.PANIC_CAUSES
    pub fn causes_panic(&self) -> bool {
        if let DamageKind::Named(named) = self.kind {
            return named.has(NamedDamage::PANIC_CAUSES);
        }
        !matches!(
            self.kind,
            DamageKind::Fall
                | DamageKind::Drown
                | DamageKind::DryOut
                | DamageKind::Generic
                | DamageKind::InWall
                | DamageKind::SweetBerryBush
                | DamageKind::OutOfWorld
                | DamageKind::FallingBlock
                | DamageKind::FallingAnvil
                | DamageKind::Cramming
                | DamageKind::GenericKill
                | DamageKind::OutsideBorder
                | DamageKind::Spit
                | DamageKind::Stalagmite
                | DamageKind::Starve
                | DamageKind::Campfire
                // `mob_attack` panics; its no-aggro twin is not in the tag.
                | DamageKind::MobAttackNoAggro
        )
    }
    /// DamageTypeTags.BYPASSES_ARMOR
    pub fn bypasses_armor(&self) -> bool {
        if let DamageKind::Named(named) = self.kind {
            return named.has(NamedDamage::BYPASSES_ARMOR);
        }
        matches!(
            self.kind,
            DamageKind::Fall
                | DamageKind::FlyIntoWall
                | DamageKind::OnFire
                | DamageKind::Drown
                | DamageKind::Magic
                | DamageKind::IndirectMagic
                | DamageKind::InWall
                | DamageKind::Generic
                | DamageKind::SonicBoom
                | DamageKind::Freeze
                | DamageKind::OutOfWorld
                | DamageKind::Cramming
                | DamageKind::GenericKill
                | DamageKind::OutsideBorder
                | DamageKind::Stalagmite
                | DamageKind::Starve
                | DamageKind::Wither
        )
    }
    pub fn is_fire(&self) -> bool {
        if let DamageKind::Named(named) = self.kind {
            return named.has(NamedDamage::IS_FIRE);
        }
        matches!(
            self.kind,
            DamageKind::OnFire
                | DamageKind::InFire
                | DamageKind::Campfire
                | DamageKind::HotFloor
                | DamageKind::Lava
                | DamageKind::Fireball
                | DamageKind::SulfurCubeHot
        )
    }
    /// DamageTypeTags.NO_KNOCKBACK
    pub fn no_knockback(&self) -> bool {
        if let DamageKind::Named(named) = self.kind {
            return named.has(NamedDamage::NO_KNOCKBACK);
        }
        matches!(
            self.kind,
            DamageKind::Explosion
                | DamageKind::Fall
                | DamageKind::FlyIntoWall
                | DamageKind::OnFire
                | DamageKind::InFire
                | DamageKind::Campfire
                | DamageKind::HotFloor
                | DamageKind::Lava
                | DamageKind::Drown
                | DamageKind::DryOut
                | DamageKind::Magic
                | DamageKind::Cactus
                | DamageKind::SweetBerryBush
                | DamageKind::Generic
                | DamageKind::InWall
                | DamageKind::Freeze
                | DamageKind::OutOfWorld
                | DamageKind::LightningBolt
                | DamageKind::Cramming
                | DamageKind::GenericKill
                | DamageKind::OutsideBorder
                | DamageKind::Stalagmite
                | DamageKind::Starve
                | DamageKind::SulfurCubeHot
                | DamageKind::Wither
                | DamageKind::Spear
        )
    }
    pub fn is_projectile(&self) -> bool {
        if let DamageKind::Named(named) = self.kind {
            return named.has(NamedDamage::IS_PROJECTILE);
        }
        matches!(
            self.kind,
            DamageKind::Arrow
                | DamageKind::Thrown
                | DamageKind::Fireball
                | DamageKind::WindCharge
                | DamageKind::WitherSkull
        )
    }
}
/// Per-mob attribute base values (the type's defaults unless changed).
#[derive(Clone, Copy, Debug)]
pub struct Attrs {
    pub speed: f64,
    pub attack_damage: f32,
    /// `Attributes.ATTACK_KNOCKBACK`: what a melee hit adds to its push (half of it).
    pub attack_knockback: f64,
    pub max_health: f32,
    pub follow_range: f64,
    pub knockback_resistance: f64,
    pub spawn_reinforcements: f64,
    pub jump_strength: f64,
}
/// `minecraft:wolf_armor`'s maximum damage.
pub const WOLF_ARMOR_DURABILITY: i32 = 64;
/// Species-specific persistent state (variants, colours, taming, ageing...).
#[derive(Clone, Debug, Default)]
pub struct Extra {
    pub client_side: bool, // `Level.isClientSide`: a client copy, which `Entity.hurt` leaves alone.
    /// Saved fish/axolotl bucket flag; tadpoles save a constant true value.
    pub from_bucket: bool,
    /// Tags a mod gave the mob before it joined a level (`finalize_spawn`'s
    /// `UnspawnedMobEdits`): the level keeps a joined entity's tags with its
    /// saved data, so they move there when the mob joins.
    pub spawn_tags: Vec<String>,
    /// Command-created armor instances, shared by commands and runtime damage.
    pub armor_attributes: Option<Box<horizon_world::attributes::entity::EntityAttributes>>,
    /// `Mob.lootTable`: the table its death rolls instead of its type's
    /// (saved as `DeathLootTable`).
    pub death_loot_table: Option<String>,
    /// `Mob.lootTableSeed` (`DeathLootTableSeed`): 0 for none.
    pub death_table_seed: i64,
    /// `Rabbit.moreCarrotTicks`: how long it is full of carrots (saved as `MoreCarrotTicks`).
    pub more_carrot_ticks: i32,
    /// `Ocelot.DATA_TRUSTING`: it no longer runs from players.
    pub trusting: bool,
    /// `Mannequin.DATA_IMMOVABLE`: it neither moves nor falls (`isEffectiveAi` is false).
    pub immovable: bool,
    /// `Mannequin.DATA_DESCRIPTION`: the text under its name.
    pub description: MannequinDescription,
    /// `LivingEntity`'s `minecraft:scale` (`getScale`), which scales its box
    /// (`getDimensions`), eye height and model (`LivingEntityRenderState.scale`):
    /// the local player's own, and on a server every living entity's from its
    /// attribute updates; 0 is the default, 1 (`Mob::model_scale`).
    pub model_scale: f32,
    /// Players: crouching and riding (HumanoidModel's poses).
    pub crouching: bool,
    /// A player's or mannequin's synced pose (`Entity.DATA_POSE`), which
    /// sets its box (`Avatar.POSES`).
    pub avatar_pose: crate::pose::Pose,
    /// `Entity.isSprinting` (`Entity.DATA_SHARED_FLAGS_ID`'s flag 3): the dust a
    /// player raises; a cat or ocelot dashing (`Cat/Ocelot.customServerAiStep`),
    /// which `LivingEntity.setSprinting`'s speed modifier makes faster.
    pub sprinting: bool,
    pub passenger: bool,
    /// AgeableMob.age: negative while a baby (a tadpole counts up to 24000).
    pub age: i32,
    /// `AgeLocked` (tadpoles fed a slime ball... and ageable mobs).
    pub age_locked: bool,
    /// `AgeableMob.ageLockParticleTimer`: growth particles left, and the cooldown. Not saved.
    pub age_lock_particle_timer: i32,
    /// AgeableMob.forcedAge: the age fed food added, restored as the age
    /// when it reaches 0 (`ForcedAge`).
    pub forced_age: i32,
    /// AgeableMob.forcedAgeTimer: ticks of happy villager particles left.
    pub forced_age_timer: i32,
    pub in_love: i32,
    pub love_player: bool,
    /// Texture/model variant index (cat type, horse colour, rabbit type...).
    pub variant: u8,
    /// Secondary variant (horse markings, tropical fish pattern colours, panda hidden gene).
    pub variant2: u8,
    /// Villager or zombie-villager profession; no numeric mirror is stored.
    pub profession: villager::ProfessionState,
    pub variant3: u8,
    /// VillagerData.level for villagers and zombie villagers.
    pub villager_level: i32,
    /// `MushroomCow.stewEffects`: what a brown one took from a flower, given
    /// back in its next bowl of stew. `None` is vanilla's null.
    pub stew_effects: Option<Vec<(Arc<str>, i32)>>,
    /// Dye colour id (sheep wool, collars).
    pub color: u8,
    /// `Fox.DATA_TRUSTED_ID_0` and `_1`: the players a fox trusts, first free
    /// slot first, and a third replaces the second (`Fox.addTrustedEntity`).
    pub trusted: [Option<[i32; 4]>; 2],
    pub last_stare_sound: Option<u32>,
    /// `Animal.LoveCause`: the player who fed this animal into love, which a
    /// fox's breeding hands to its baby as trust (`Fox.FoxBreedGoal.breed`).
    pub love_cause: Option<[i32; 4]>,
    pub sheared: bool,
    /// `Mob.canPickUpLoot` (also lets dispensers equip it).
    pub pick_up_loot: bool,
    /// What the mob holds and wears, in each `EquipmentSlot` (`LivingEntity.equipment`):
    /// the hands, armour, body (horse and wolf armour, harnesses, carpets)
    /// and saddle, with their count, damage and components. The enchantments
    /// and trims of the hands and armour are overlaid from `gear`.
    pub equipment: horizon_items::equipment::Equipment,
    /// `Cat.RELAX_STATE_ONE`: watching a sleeping owner before lying down.
    pub relax_one: bool,
    /// `Cat.isLyingOnTopOfSleepingPlayer` (the client's own, from `Cat.handleLieDown`).
    pub on_sleeper: bool,
    pub chested: bool,
    /// The chest contents of a donkey or mule (15 slots).
    pub items: Vec<Option<crate::items::ItemStack>>,
    /// Occupied InventoryCarrier stacks: Piglin/trader (8 slots), Pillager (5).
    pub inventory: Vec<crate::items::ItemStack>,
    /// `AbstractHorse.temper` of every equine (horses, llamas, camels).
    /// Loaded signed values are independent of the gameplay modification cap.
    pub equine_temper: i32,
    pub tame: bool,
    /// `TamableAnimal.isOrderedToSit` (the saved order), and in `sitting_pose`
    /// `isInSittingPose` (bit 1 of `DATA_FLAGS_ID`, which only a sit goal writes).
    pub sitting: bool,
    pub sitting_pose: bool,
    /// `Parrot.partyParrot` and `jukebox`: the playing jukebox it dances to.
    pub party_jukebox: Option<[i32; 3]>,
    /// The owner's UUID: a tamed animal's, or the reference a vex was loaded with.
    pub owner: Option<[i32; 4]>,
    /// `Vindicator.isJohnny`: a custom name that was ever `Johnny`, which
    /// `setCustomName` latches and nothing clears.
    pub johnny: bool,
    /// Slime/magma cube size (0 = not sized).
    pub size: i32,
    /// Sheep eat-grass / iron golem attack / ravager animation ticks.
    pub anim_ticks: i32,
    pub egg_time: i32,
    pub anger: i32,
    /// `NeutralMob.getPersistentAngerEndTime`: the absolute game time the
    /// anger ends at, -1 when the mob is calm. The save keeps it, so the
    /// anger goes on while the chunk is unloaded.
    pub anger_end: i64,
    /// Countdown and age when `anger_end` was last settled (`neutral_anger`).
    pub anger_stamp: Option<(i32, u32)>,
    /// `NeutralMob.getPersistentAngerTarget`: the entity the mob is angry
    /// at, which the save keeps with it.
    pub angry_at: Option<Who>,
    /// `IronGolem.isPlayerCreated` (`PlayerCreated`): built from blocks.
    pub player_created: bool,
    pub carried: Option<horizon_world::registry::BlockStateId>,
    pub charged: bool,
    /// `Creeper.explosionRadius` (`ExplosionRadius`, a byte): the blast's
    /// radius, 3 unless the save or a command says otherwise; a powered
    /// creeper blows with twice it (`explodeCreeper`).
    pub explosion_radius: i8,
    pub adopted_body: Option<Arc<str>>,
    pub using_item: i32,
    pub using_off_hand: bool,
    /// `LivingEntity.recentKineticEnemies`: when the entities this mob's
    /// spear stabbed in the current use were stabbed (game time). It exists
    /// only while a kinetic weapon is in use.
    pub stabbed: Option<Vec<(Who, i64)>>,
    /// `ItemStack.getUseAnimation` of the item in use (`None` when nothing is);
    /// a player model's arm pose and `isBlocking` follow it.
    pub use_animation: horizon_core::use_animation::UseAnimation,
    /// The `arm_pose` the items in the main and off hand declare, if any: the pose
    /// a humanoid model's arm takes holding it, in place of vanilla's.
    pub held_arm_pose: [Option<horizon_core::arm_pose::ArmPose>; 2],
    pub fall_flying: bool,
    pub fall_fly_ticks: i32,
    /// `Entity.deltaMovementOnPreviousTick`: the velocity before this tick's
    /// move, which the elytra lean interpolates from (`getDeltaMovementLerped`).
    pub previous_velocity: Vec3,
    /// `Entity.DATA_POSE` of another player, as the server sent it.
    pub synced_pose: u8,
    /// `LivingEntity.isAutoSpinAttack` (Riptide).
    pub auto_spin_attack: bool,
    /// `LivingEntity.DATA_ARROW_COUNT_ID` and `DATA_STINGER_COUNT_ID`: the
    /// arrows and bee stingers stuck in the body (`ArrowLayer`,
    /// `BeeStingerLayer`), with the timers that pull them out.
    pub stuck: stuck::Stuck,
    /// `Player.DATA_SHOULDER_PARROT_LEFT` and `_RIGHT`: the variant of the
    /// parrot on each shoulder (`ParrotOnShoulderLayer`).
    pub shoulder_parrots: [Option<i32>; 2],
    pub main_hand_left: bool,
    /// The skin layers a player hides (`PlayerModelPart` bits, set to
    /// `Avatar.DATA_PLAYER_MODE_CUSTOMISATION` inverted); 0 shows all.
    pub hidden_model_parts: u8,
    /// `AvatarRenderState.isSpectator`: a player in spectator mode shows only its head.
    pub spectator: bool,
    pub elytra: ElytraAnimation,
    pub swim: SwimAnimation,
    /// The arm the current swing moves (`currentSwing.hand().asArm`): `Some(true)`
    /// for the right arm.
    pub swing_right_arm: Option<bool>,
    /// The current swing is a STAB (`SwingAnimationType` 2): a spear's jab.
    pub swing_stab: bool,
    pub flag: bool,
    pub flag2: bool,
    /// `AbstractHorse`'s `Bred` save flag: nothing sets it at runtime (no
    /// vanilla code calls `setBred` outside `readAdditionalSaveData`), so only
    /// a save or `/summon` gives it, and `followMommy` reads it.
    pub bred: bool,
    /// `Hoglin`'s `CannotBeHunted` save flag: a bastion's own hoglins have it,
    /// so its piglins leave them alone (`Hoglin.canBeHunted`).
    pub cannot_be_hunted: bool,
    /// `AbstractHorse.allowStandSliding`: the rider's jump sets it, and a
    /// horse still rearing then goes on with the rider's keys until the rear
    /// ends (`tick` clears it).
    pub stand_sliding: bool,
    /// Species scratch values (the wither's head targets and clocks, a ghast's power).
    pub ints: [i32; 6],
    pub counter: i32,
    /// Nautilus ATTACK_TARGET_COOLDOWN saved slot: None is absent,
    /// Some(MAX) is permanent, otherwise the remaining lifetime. The value is `counter`.
    pub nautilus_attack_cooldown_ttl: Option<i64>,
    pub counter2: i32,
    pub home: Option<[i32; 3]>,
    /// A copper golem's `openedChestPos`.
    pub opened_chest: Option<[i32; 3]>,
    pub point: Option<[f64; 3]>,
    /// `NEAREST_REPELLENT`: the nearest repelling block a hoglin or piglin
    /// sensed, looked up every 20 ticks.
    pub repellent: Option<[i32; 3]>,
    pub vector: [f64; 3],
    /// Active status effects: (effect id, ticks left, amplifier).
    pub effects: Vec<StoredEffect>,
    /// Hidden effect instances behind an active effect, keyed by effect id.
    pub effect_chains: Vec<crate::effects::EffectInstance>,
    /// The effects above given with `hideParticles`: no swirls for them.
    pub hidden_effects: Vec<&'static str>,
    /// The effects above that are ambient (`MobEffectInstance.isAmbient`: a beacon's), whose swirls
    /// are faint.
    pub ambient_effects: Vec<&'static str>,
    /// `LivingEntity.DATA_EFFECT_PARTICLES` and `DATA_EFFECT_AMBIENCE_ID` as a
    /// server gave them: the swirls around it, and whether they are all faint.
    pub effect_particles: Vec<crate::particles::ParticleOptions>,
    pub effect_ambient: bool,
    /// The swirls of `effects` as the native mods left them
    /// (`effect_particle_modification`), sent while one listens.
    pub modified_effect_particles: Option<crate::effects::ModifiedEffectParticles>,
    /// An invisible illusioner's mirror images.
    pub illusions: Option<illusioner::Illusions>,
    /// A villager's claimed bed head and meeting bell.
    pub bed: Option<[i32; 3]>,
    pub meeting: Option<[i32; 3]>,
    /// Villager and wandering trader offers and experience.
    pub trade: Option<Box<crate::merchant::Trader>>,
    /// Who holds this mob's lead, a holder not yet found after loading, and
    /// the spin the rope's torque gives it.
    pub leash: Option<leash::Holder>,
    pub leash_pending: Option<leash::Pending>,
    /// The UUID of the player behind `Holder::Player` (host or guest).
    pub leash_player: Option<[i32; 4]>,
    pub leash_spin: f64,
    /// More species scratch values.
    pub scratch: [i32; 4],
    /// Block positions a species remembers (a sniffer's explored ground).
    pub positions: Vec<[i32; 3]>,
    /// A warden's brain, anger and vibrations.
    pub warden: Option<Box<warden::Warden>>,
    /// An allay's vibration listener and liked note block.
    pub allay: Option<Box<allay::State>>,
    /// A raider's patrol and raid state.
    pub raider: Option<Box<raider::State>>,
    /// A villager's raid activity and gossip.
    pub villager: Option<Box<villager::Ext>>,
    /// The sniffer's and the ghastling's brain.
    pub animal_brain: Option<Box<animal_brain::Brain>>,
    pub axolotl: axolotl::Animation,
    /// The mob or boat this one rides, and the mob riding it (`getVehicle`,
    /// `getFirstPassenger`).
    pub mount: Option<u32>,
    pub rider: Option<u32>,
    /// This rider's place in its vehicle's `getPassengers`, beside `Guest.vehicle_order`: the
    /// lowest goes first. Passengers that load together keep 0 and their list order.
    pub vehicle_order: i64,
    /// Where `positionRider` last seated this rider: its own vehicle
    /// attachment (`getPassengerRidingPosition(this).y` is its feet plus this)
    /// and the vehicle's box (`Mob.getAttackBoundingBox`, `getHitbox`).
    pub seat: Option<(u32, f64, Aabb)>,
    /// The living vehicle `positionRider` last seated this rider on, with
    /// its `yBodyRotO` and `yBodyRot`: a rider of a living vehicle is drawn
    /// from that body (`LivingEntityRenderer.solveBodyRot`), not its own.
    pub vehicle_body: Option<(u32, f32, f32)>,
    /// `Entity.boardingCooldown`: ticks after leaving a boat before a boat
    /// can pick this mob up again.
    pub boarding_cooldown: i32,
    /// A chicken carrying a zombie (`IsChickenJockey`).
    pub jockey: bool,
    /// A skeleton horse waiting to spring its trap, and how long it has waited.
    pub trap: bool,
    pub trap_time: i32,
    /// Enchantments on the equipment.
    pub gear: Option<Box<gear::Gear>>,
    /// `Mob.dropChances` by `EquipmentSlot` (`drops::SLOTS` order) when set
    /// explicitly; None uses the defaults of `drops::drop_chance`.
    pub drop_chances: Option<Box<[f32; 8]>>,
    /// `Mob.leftHanded`.
    pub left_handed: bool,
    /// The sound variant: index in the species' `*_sound_variant` registry
    /// (`finalize::sound_variants`); None is the default, `classic`.
    pub sound_variant: Option<u8>,
    /// `Zombie.canBreakDoors`.
    pub can_break_doors: bool,
    /// A goat's `IsScreamingGoat` and the horns it lost (left, right).
    pub screaming: bool,
    pub lost_horns: [bool; 2],
    /// A goat's lowered head and ram behaviours.
    pub goat: goat::State,
    /// Attribute bases and permanent modifiers set by `finalizeSpawn`.
    pub attributes: Option<Box<finalize::Attributes>>,
    /// Brain memories `finalizeSpawn` sets: (memory, value, ticks to expiry).
    pub memories: Vec<(&'static str, i64, i64)>,
    /// A camel's `LAST_POSE_CHANGE_TICK`, and its game time minus age at the last tick.
    pub pose_change_tick: i64,
    pub pose_epoch: Option<i64>,
    /// Whether a camel sits (`egg_time` is then the age it sat down at).
    pub camel_sitting: bool,
    /// `Entity.DATA_POSE` as a mob of a server has it (`sync::pose_id`).
    pub pose: u8,
    /// The school leader a fish follows, and a tropical fish that swims alone.
    pub leader: Option<u32>,
    pub lone: bool,
    /// The fish that follow this one (`AbstractSchoolingFish.schoolSize` - 1).
    pub followers: i32,
    /// The entity section it is in and when it entered it (a global sequence):
    /// vanilla's entity queries list the entities of a section in the order they
    /// entered it, the sections by x, z, y.
    pub section: [i32; 3],
    pub section_seq: u64,
    /// A breeze's or creaking's brain (`monster_brain`).
    pub brain: Option<Box<monster_brain::Brain>>,
    /// A bee's nectar, anger, flower and hive state.
    pub bee: Option<Box<bee::Bee>>,
    /// An armadillo's state and scare brain.
    pub armadillo: armadillo::State,
    /// A frog's brain memories and tongue target.
    pub frog: frog::State,
    /// A panda's flags, counters and amounts.
    pub panda: Option<Box<panda::State>>,
    /// What a piglin's or hoglin's brain remembers: its avoid target, the
    /// adults its sensor sees and the activity it is in.
    pub piglin: piglin::Memories,
    /// What the native mods last heard it hold and wear
    /// (`LivingEntity.lastEquipmentItems`, empty at first), by
    /// `EquipmentSlot` ordinal: see `Mob::equipment_stacks`.
    pub equipment_heard: Option<Box<horizon_items::equipment::Equipment>>,
    /// The live attribute map (`AttributeMap`), built when an item with
    /// attribute modifiers is worn or it is read. Mob logic still reads
    /// `MobType::attributes` and `attributes` above.
    pub live: crate::living_attributes::LivingAttributes,
    /// The knockback resistance modifiers of the items it wears or holds.
    pub gear_knockback: Option<Box<GearKnockback>>,
    /// What the game last read from the hands' `minecraft:death_protection`
    /// components (`game/totem.rs`); `None` until it has, when the totem's id
    /// stands in for the component.
    pub hand_protection: Option<HandProtection>,
}
/// The knockback resistance modifiers of the items a mob wears or holds, with
/// the stacks they were read from: the game reads them again only when one
/// of those stacks changes (`ItemStack.matches`).
#[derive(Clone, Debug, Default)]
pub struct GearKnockback {
    /// The hand and armor stacks, mainhand to head.
    pub worn: [Option<crate::items::ItemStack>; 6],
    pub modifiers: Vec<(f64, finalize::Op)>,
}
/// The `death_effects` of the `death_protection` component each hand's stack
/// carries; `None` for a hand without the component.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HandProtection {
    pub main: Option<Vec<crate::items::ConsumeEffect>>,
    pub off: Option<Vec<crate::items::ConsumeEffect>>,
}
impl Extra {
    /// The body item id (`getItemBySlot(BODY)`).
    pub fn body_id(&self) -> Option<&str> {
        self.body_stack().map(|s| &*s.item)
    }
    /// The body stack, with its count, damage and components.
    pub fn body_stack(&self) -> Option<&crate::items::ItemStack> {
        self.equipment.get(horizon_items::equipment::BODY)
    }
    pub fn set_body_stack(&mut self, stack: Option<crate::items::ItemStack>) {
        self.equipment.set(horizon_items::equipment::BODY, stack);
    }
    /// Wears a plain body item of this id.
    pub fn set_body(&mut self, item: Option<Arc<str>>) {
        self.set_body_stack(item.map(|id| crate::items::ItemStack::new(&id, 1)));
    }
    /// The durability the body armour has lost.
    pub fn body_damage(&self) -> i32 {
        self.body_stack().map_or(0, |s| s.damage as i32)
    }
    pub fn set_body_damage(&mut self, damage: i32) {
        if let Some(stack) = &mut self.equipment[horizon_items::equipment::BODY] {
            stack.damage = damage.max(0) as u32;
        }
    }
    /// `Mob.isSaddled`.
    pub fn saddled(&self) -> bool {
        self.equipment
            .get(horizon_items::equipment::SADDLE)
            .is_some()
    }
    /// Wears or takes off a plain saddle.
    pub fn set_saddled(&mut self, saddled: bool) {
        let saddle = saddled.then(|| crate::items::ItemStack::new("minecraft:saddle", 1));
        self.equipment.set(horizon_items::equipment::SADDLE, saddle);
    }

    /// The id of the item in `slot` (`EquipmentSlot` ordinal).
    pub fn slot_id(&self, slot: usize) -> Option<&str> {
        self.equipment.get(slot).map(|s| &*s.item)
    }
    /// The stack in `slot`, as it was kept (the `gear` enchantments and trim
    /// are not on it: `finalize::item_stack` overlays them).
    pub fn slot_stack(&self, slot: usize) -> Option<&crate::items::ItemStack> {
        self.equipment.get(slot)
    }
    pub fn set_slot_stack(&mut self, slot: usize, stack: Option<crate::items::ItemStack>) {
        self.equipment.set(slot, stack);
    }
    /// Puts one plain item of this id in `slot`.
    pub fn set_slot_id(&mut self, slot: usize, item: Option<Arc<str>>) {
        self.set_slot_stack(slot, item.map(|id| crate::items::ItemStack::new(&id, 1)));
    }
    pub fn main_hand_id(&self) -> Option<&str> {
        self.slot_id(horizon_items::equipment::MAINHAND)
    }
    pub fn off_hand_id(&self) -> Option<&str> {
        self.slot_id(horizon_items::equipment::OFFHAND)
    }
    pub fn head_id(&self) -> Option<&str> {
        self.slot_id(horizon_items::equipment::HEAD)
    }
    pub fn set_main_hand(&mut self, item: Option<Arc<str>>) {
        self.set_slot_id(horizon_items::equipment::MAINHAND, item);
    }
    pub fn set_main_hand_stack(&mut self, stack: Option<crate::items::ItemStack>) {
        self.set_slot_stack(horizon_items::equipment::MAINHAND, stack);
    }
    pub fn main_hand_stack(&self) -> Option<&crate::items::ItemStack> {
        self.slot_stack(horizon_items::equipment::MAINHAND)
    }
    /// `LivingEntity.swapHandItems`: the two hands trade their whole stacks,
    /// with the enchantments and trims `gear` keeps for them.
    pub fn swap_hands(&mut self) {
        use horizon_items::equipment::{MAINHAND, OFFHAND};
        let (main, off) = (
            self.equipment[MAINHAND].take(),
            self.equipment[OFFHAND].take(),
        );
        self.equipment[MAINHAND] = off;
        self.equipment[OFFHAND] = main;
        if let Some(g) = self.gear.as_deref_mut() {
            g.slots.swap(gear::MAIN, gear::OFF);
            g.trims.swap(gear::MAIN, gear::OFF);
        }
    }
    /// Empties `slot` and returns the id of what it held.
    pub fn take_slot_id(&mut self, slot: usize) -> Option<Arc<str>> {
        self.equipment[slot].take().map(|s| s.item)
    }
}
/// How far the boxes of a set of mobs reach from their positions: the widest reach sideways, below
/// and above among them, worked out from each mob's real box ([`Mob::bounds`]: a shulker's grows
/// toward its open lid, sideways or below its position). A box query over many mobs (a vehicle's,
/// against every mob of the level) tests each mob's position against this first, and works out the
/// box only for the mobs whose position comes close.
#[derive(Clone, Copy, Debug, Default)]
pub struct BoxReach {
    half_width: f64,
    below: f64,
    height: f64,
}
impl BoxReach {
}

/// A mannequin's `description` and `hide_description` (`Mannequin.updateDescription`):
/// the default `entity.minecraft.mannequin.label`, none at all, or a component.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum MannequinDescription {
    #[default]
    Default,
    Hidden,
    Custom(horizon_core::nbt::Tag),
}
impl MannequinDescription {
    /// `Mannequin.getDescription`: the component `DATA_DESCRIPTION` holds, none when hidden.
    pub fn component(&self) -> Option<horizon_core::nbt::Tag> {
        match self {
            Self::Default => Some(horizon_core::nbt::Tag::compound([(
                "translate",
                horizon_core::nbt::Tag::String("entity.minecraft.mannequin.label".into()),
            )])),
            Self::Hidden => None,
            Self::Custom(tag) => Some(tag.clone()),
        }
    }
    /// `DATA_DESCRIPTION` as a client reads it.
    pub fn from_component(tag: Option<&horizon_core::nbt::Tag>) -> Self {
        match tag {
            None => Self::Hidden,
            Some(tag) if Some(tag) == Self::Default.component().as_ref() => Self::Default,
            Some(tag) => Self::Custom(tag.clone()),
        }
    }
}
/// Whether breeding experience was drawn in the parent's tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreedExperience {
    Deferred,
    Drawn(Option<u32>),
}

/// Makes the events pushed since `start` (the effects of a break a mod may stop)
/// conditional on the game having broken one of `mob`'s blocks. Unless a listener
/// can cancel the break (`MobWorld::asks_living_destroy_block`, passed as `asks`)
/// they stay plain events: nothing is allocated.
pub fn after_break(events: &mut Vec<MobEvent>, asks: bool, mob: u32, start: usize) {
    if asks {
        let effects = events.split_off(start);
        events.push(MobEvent::IfBroke {
            mob,
            effects,
            otherwise: Vec::new(),
        });
    }
}

/// Events a mob tick asks the game to act on.
#[derive(Clone, Debug)]
pub enum MobEvent {
    /// A player-directed event remembers the player selected at emission.
    /// `None` is the server's own player; `Some` is a guest connection.
    ForPlayer {
        conn: Option<u32>,
        event: Box<MobEvent>,
    },
    /// Squid.handleEntityEvent(19), resetting the remote tentacle cycle.
    SquidCycle {
        mob: u32,
        at: [f64; 3],
    },
    /// Rabbit entity event 1, sent only when a ground jump actually happens.
    RabbitHop {
        mob: u32,
        at: [f64; 3],
    },
    /// `LivingEntity.baseTick`'s drowning: entity event 67 for 8 `bubbles`, else that many particles.
    Drown {
        mob: u32,
        at: [f64; 3],
        bubbles: i32,
    },
    /// PowderSnowBlock.entityInside's snowflake with vanilla's sampled velocity.
    PowderSnowParticle {
        at: [f64; 3],
        velocity: [f64; 3],
    },
    /// `Mob.burnUndead` in daylight on a mob wearing something in its
    /// `sunProtectionSlot`: the item loses up to 1 durability (the game knows
    /// the item data) instead of the mob catching fire.
    SunProtectionWear {
        mob: u32,
    },
    /// `Level.broadcastEntityEvent` for a mob: guests run its
    /// `handleEntityEvent`.
    EntityEvent {
        mob: u32,
        event: i8,
        at: [f64; 3],
    },
    /// `ServerLevel.sendParticles`: drawn here and sent to the players within
    /// 32 blocks, which spread and speed the way the client does for a count
    /// (a count of 0 is one particle moving by speed times spread).
    LevelParticles {
        kind: &'static str,
        at: [f64; 3],
        spread: [f64; 3],
        speed: f64,
        count: u32,
    },
    /// `ServerLevel.sendParticles` of an `ItemParticleOption`: `count` crumbs of
    /// `item`, drawn in this window.
    ItemParticles {
        item: &'static str,
        at: [f64; 3],
        spread: [f64; 3],
        speed: f64,
        count: u32,
    },
    Sound(&'static str, [f64; 3], f32, f32),
    ProfessionWorkSound(Arc<str>, [f64; 3], f32, f32),
    /// `level.playSound(null, mob, sound, source, volume, pitch)`: a sound
    /// bound to mob `mob` (`ServerLevel.playSeededSound`'s entity overload),
    /// in vanilla's `source`, from where the mob stands.
    BoundSound {
        mob: u32,
        sound: &'static str,
        source: mod_api::SoundSource,
        at: [f64; 3],
        volume: f32,
        pitch: f32,
    },
    /// `Entity.sendBubbleColumnParticles`: a mob at the top of the bubble column
    /// block at `pos` (the splash and bubble particles over it).
    BubbleColumnParticles {
        pos: [i32; 3],
    },
    /// A `BlockParticleOption` of `state` drawn `count` times at `at`, each crumb
    /// leaving at `speed`: the dust `LivingEntity.checkFallDamage` throws up from
    /// the block a mob landed on, and the puffs `Breeze.emitGroundParticles`
    /// leaves at a breeze's feet (which it draws at rest).
    LandingDust {
        state: horizon_world::registry::BlockStateId,
        at: [f64; 3],
        count: i32,
        speed: f64,
    },
    /// `UseBonemeal`: villager `mob`'s bone meal on the crop at `pos`
    /// (`BoneMealItem.growCrop`, then level event 1505 when it grew).
    Bonemeal {
        mob: u32,
        pos: [i32; 3],
    },
    /// `RingBell`: the villager rings the meeting-point bell
    /// (`BellBlock.attemptToRing`: the bell shakes, the sound and the game
    /// event follow).
    RingBell {
        bell: [i32; 3],
    },
    /// `ChestBlockEntity.startOpen` / `stopOpen` by a copper golem: the
    /// chest's opener count goes up or down (interaction range 3).
    ChestOpen {
        chest: [i32; 3],
        open: bool,
        /// The golem, the source of the container's game event.
        mob: u32,
    },
    /// `BeeEnterHiveGoal.start`: the bee goes into its hive (the host stores
    /// its tag with `BeehiveBlockEntity.addOccupant` and discards it).
    EnterHive {
        mob: u32,
        hive: [i32; 3],
    },
    /// `LandOnOwnersShoulderGoal`: a tame parrot touched its owner and sits
    /// on their shoulder (the host stores its tag and discards it).
    ParrotToShoulder {
        mob: u32,
    },
    /// `CopperGolem.turnToStatue`.
    BecomeStatue {
        mob: u32,
    },
    /// A copper golem took from or filled a chest slot.
    ChestSlot {
        chest: [i32; 3],
        slot: usize,
        stack: Option<crate::items::ItemStack>,
    },
    /// `InfestedMobEffect.onMobHurt`: the mob was hurt while infested and its
    /// draws (`infested_silverfish`) spawn silverfish at `at`, half its height
    /// up where it was hurt, each thrown with its velocity.
    Infested {
        at: [f64; 3],
        velocities: Vec<Vec3>,
    },
    /// `LivingEntity.hurtArmor` (`doHurtEquipment`): a hit that landed on a mob
    /// wearing body armour (a horse's) leaves the piece owing
    /// `max(1, damage / 4)` durability. The host applies it: it is the one
    /// that knows the item's maximum, whether it wears when hurt at all and
    /// whether a fire hit leaves it alone (`ItemStack.canBeHurtBy`).
    ArmorHurt {
        mob: u32,
        amount: f32,
        source: crate::health::HurtSource<'static>,
    },
    /// `LivingEntity.updateFallFlying`: every 20 glide ticks the glider in
    /// `slot` (the one `Util.getRandom` picked) takes one point of wear,
    /// `ItemStack.hurtAndBreak(1, this, slot)`. The host applies it with the
    /// item's maximum and Unbreaking.
    GliderWear {
        mob: u32,
        slot: horizon_core::equipment_slot::EquipmentSlot,
    },
    /// `OfferFlowerGoal.stop`: the item goes to the target's antenna (saddle)
    /// slot if it is empty, as a guaranteed drop.
    Gift {
        target: u32,
        item: &'static str,
    },
    /// `AnimalMakeLove`: the partner remembers this mob as its breed target
    /// and walks to it and looks at it.
    BreedTarget {
        mob: u32,
        partner: u32,
    },
    /// `BehaviorUtils.lockGazeAndWalkToEachOther`'s other half: the partner
    /// looks at this mob and walks to it.
    LockGaze {
        mob: u32,
        partner: u32,
    },
    /// `AbstractSchoolingFish.startFollowing` on a fish a leader recruits:
    /// it follows the leader, whose school grows by one.
    StartFollowing {
        fish: u32,
        leader: u32,
    },
    /// `addFollower` / `removeFollower`: a leader's school grows or shrinks.
    SchoolSize {
        leader: u32,
        by: i32,
    },
    /// A sound event id that is not a static string (variant sounds).
    SoundId(String, [f64; 3], f32, f32),
    /// `Level.playLocalSound`: a sound the client plays for itself, never
    /// sent by a server (a breeze's whirl and idle sounds).
    LocalSound(&'static str, [f64; 3], f32, f32),
    /// A level event's sound (`levelEvent`/`globalLevelEvent`): the client
    /// plays it with its own random pitch spread (`(a - b) * spread + 1`),
    /// nothing is drawn from the mob.
    LevelEventSound {
        sound: &'static str,
        at: [f64; 3],
        volume: f32,
        spread: f32,
    },
    /// `Level.gameEvent` from a mob's own movement (`GameEvent.STEP`): the
    /// game's sculk layer tracks steps itself, the mob AI probe hears them
    /// through the wardens (`warden::listen`).
    GameEvent {
        event: &'static str,
        at: [f64; 3],
        source: u32,
        state: horizon_world::registry::BlockStateId,
    },
    /// `Level.gameEvent` from the block a mob landed on (`FarmlandBlock.turnToBaseBlock`'s
    /// BLOCK_CHANGE from the mob that trampled it, `TurtleEggBlock.decreaseEggs`' and
    /// `Level.destroyBlock`'s BLOCK_DESTROY with no source): the game posts it, a
    /// sculk sensor, a calibrated sensor and a warden hear it.
    LandingGameEvent {
        event: &'static str,
        at: [i32; 3],
        /// The entity that caused the change, `None` for a destroyed egg.
        source: Option<u32>,
        state: horizon_world::registry::BlockStateId,
    },
    AttackPlayer(f32, [f64; 3]),
    /// `Bee.doHurtTarget`: a sting that lands leaves its stinger and poisons.
    Sting {
        target: Who,
        amount: f32,
        poison_ticks: i32,
        source: Damage,
    },
    /// A melee hit whose effect comes only if the hit lands (husk hunger,
    /// cave spider poison, wither skeleton wither): a shield-blocked hit gives
    /// nothing.
    AttackPlayerWithEffect {
        amount: f32,
        from: [f64; 3],
        effect: &'static str,
        ticks: i32,
        amplifier: i32,
    },
    /// Damage of a given type to the player (guardian lasers, thorns).
    DamagePlayer {
        amount: f32,
        from: [f64; 3],
        kind: &'static str,
    },
    /// `ElderGuardian`'s curse on players within 50 blocks; a silent elder's
    /// shows the apparition without the curse sound.
    ElderCurse {
        at: [f64; 3],
        silent: bool,
    },
    /// `Entity.setDeltaMovement`: replace a target's velocity.
    SetVelocity {
        target: Who,
        velocity: [f64; 3],
    },
    /// `Entity.remove(RemovalReason.KILLED)` of a mob that is no longer alive: it vanishes at once,
    /// with no death animation (`ShootTongue.eatEntity`).
    RemoveKilled {
        target: u32,
    },
    /// `Entity.push`: add to a target's velocity.
    Push {
        target: Who,
        velocity: [f64; 3],
    },
    /// `SonicBoom`: 10 damage to the target, then a push along `direction`
    /// (the unit vector from the warden's chest to the target's eyes) only
    /// if the hit landed, less the target's knockback resistance.
    SonicBoom {
        target: Who,
        attacker: u32,
        from: [f64; 3],
        direction: [f64; 3],
    },
    /// `Shulker.onPeekAmountChange`: the opening lid of `shulker` moves every
    /// entity it may push whose box meets `area` (the box the lid swept) by
    /// `Entity.move(MoverType.SHULKER, delta)`.
    LidPush {
        shulker: u32,
        area: Aabb,
        delta: [f64; 3],
    },
    /// `EvokerWololoSpellGoal`: a blue sheep turns red.
    Wololo {
        mob: u32,
    },
    /// `EvokerFangs` rising at a point after `delay` ticks.
    Fangs {
        at: [f64; 3],
        yaw: f32,
        delay: i32,
        owner: u32,
    },
    /// The mob the `Explode` right after this comes from
    /// (`ServerExplosion.source`), and whether it is a charged creeper: the
    /// exploding creeper is already out of the mob list when the game applies
    /// the blast, so its victims' loot cannot look it up, nor the death
    /// message its name (`name`, `getDisplayName`).
    ExplosionSource {
        mob: u32,
        charged: bool,
        name: crate::text::Text,
        /// `Entity.isInWater` of the source: `BlockAttached.ignoreExplosion`.
        in_water: bool,
        /// Its entity type: a player the blast kills is still credited to it
        /// (`getKillCredit` finds the source during its own explosion).
        kind: String,
    },
    /// The centre, the radius and the kind of the mob blowing up (it may be
    /// gone from the list by the time the blast is handled), and its `Silent`.
    Explode([f64; 3], f32, Species, bool),
    /// `Creeper.spawnLingeringCloud`, right after its `Explode`: an `AreaEffectCloud` at the
    /// creeper with its active effects `(id, remaining ticks, amplifier)`, in their mixed colour.
    LingeringCloud {
        at: [f64; 3],
        effects: Vec<(&'static str, i32, i32)>,
        /// The effects among them added hidden (`hideParticles`).
        hidden: Vec<&'static str>,
        /// The effects among them that are ambient.
        ambient: Vec<&'static str>,
        color: u32,
    },
    /// Damage that bypasses `EnchantmentHelper.doPostAttackEffects` (roars,
    /// beams, retaliation and hot-cube contact).
    DamageMob {
        target: u32,
        amount: f32,
        source: Damage,
    },
    /// An attack that runs post-attack effects when the hit lands.
    AttackMob {
        target: u32,
        amount: f32,
        source: Damage,
    },
    /// A melee hit on another mob whose effect rides the hit:
    /// `CaveSpider`, `Husk` and `WitherSkeleton.doHurtTarget` add it inside
    /// the `super.doHurtTarget` branch, to any `LivingEntity`, so a mob the
    /// hit lands on gets it too.
    AttackMobWithEffect {
        target: u32,
        amount: f32,
        source: Damage,
        effect: &'static str,
        ticks: i32,
        amplifier: i32,
    },
    /// `AbstractCubeMob.dealDamage` on the player (`playerTouch`): the hit,
    /// then, only when `hurtServer` let it land, the cube's
    /// `entity.slime.attack` with its pitch drawn from the cube's random.
    CubeHurtPlayer {
        cube: u32,
        amount: f32,
        from: [f64; 3],
    },
    /// `AbstractCubeMob.dealDamage` on a mob (an iron golem it touches): the
    /// hit, then, only when `hurtServer` let it land, the cube's
    /// `entity.slime.attack` with its pitch drawn from the cube's random.
    CubeHurtMob {
        cube: u32,
        target: u32,
        amount: f32,
        source: Damage,
    },
    /// `Pufferfish.touch` on a mob (`Pufferfish.aiStep`): the hit, then, only
    /// when `hurtServer` let it land, the fish's `entity.puffer_fish.sting`
    /// and the poison it gives (`poison_ticks`, `60 * getPuffState()`).
    PufferfishSting {
        mob: u32,
        target: u32,
        amount: f32,
        source: Damage,
        poison_ticks: i32,
    },
    /// `LivingEntity.stabAttack`'s `target.stopRiding()`: a spear knocks the
    /// target off its mount.
    StabDismount {
        target: Who,
    },
    /// `LivingEntity.knockback(power, x, z, source)`; a ram's blocked again first (`RamTarget.tick`).
    Knockback {
        target: Who,
        power: f64,
        x: f64,
        z: f64,
        source: Damage,
        apply_blocking: Option<f32>,
    },
    /// Set another mob's attack target (HurtByTargetGoal alerts).
    SetTarget {
        mob: u32,
        target: Who,
    },
    PiglinAlertTarget(undead::PiglinAlert),
    /// `HoglinAi.broadcastAttackTarget`: receiver and source player connection.
    HoglinAttackTarget {
        mob: u32,
        target: Who,
        player: Option<u32>,
    },
    /// `StartHuntingHoglin` for a nearby adult piglin: it takes the hoglin as
    /// its anger target (when it has none) and starts its `HUNTED_RECENTLY` wait.
    HuntHoglin {
        mob: u32,
        target: u32,
        ticks: i32,
    },
    /// `HoglinAi.broadcastRetreat` and `PiglinAi.broadcastRetreat` for one
    /// adult that sees the broadcaster: it retreats from `from`, standing at
    /// `at`, or from what it already avoids or attacks if that is nearer.
    Retreat {
        mob: u32,
        from: Who,
        at: [f64; 3],
    },
    /// `Entity.startRiding` from a mob's brain (`Mount.create`): `rider` boards
    /// `vehicle` if `Piglin.startRiding` and `canRide` allow it.
    StartRiding {
        rider: u32,
        vehicle: u32,
    },
    Projectile(crate::projectiles::Projectile),
    /// An arrow `mob` shot with `weapon` (`AbstractSkeleton.performRangedAttack`):
    /// the game asks for the ammunition (`getProjectile`) before it flies.
    BowShot {
        mob: u32,
        weapon: crate::items::ItemStack,
        arrow: crate::projectiles::Projectile,
    },
    /// `mob` finished charging `weapon` (`CrossbowItem.onUseTick`): the game
    /// asks for the ammunition (`getProjectile`), and plays the end sound
    /// at `at` only when the crossbow loaded.
    CrossbowLoad {
        mob: u32,
        weapon: crate::items::ItemStack,
        at: [f64; 3],
        pitch: f32,
    },
    /// `arrow` shot from the `ammo` a mob's crossbow was charged with.
    CrossbowShot {
        ammo: crate::items::ItemStack,
        arrow: crate::projectiles::Projectile,
    },    /// Spawn a mob by type id at a position (conversions, splits).
    SpawnKind {
        kind: String,
        at: [f64; 3],
        setup: SpawnSetup,
    },    DropItem {
        mob: u32,
        item: Arc<str>,
        count: u32,
        at: [f64; 3],
    },
    /// `Entity.spawnAtLocation` with the complete stack and its components.
    DropStack {
        mob: u32,
        stack: crate::items::ItemStack,
        at: [f64; 3],
    },
    /// `new ItemEntity(.., stack)` thrown by `mob` (`setThrower`): `Dolphin.ItemGoal.drop`.
    ThrowItem {
        mob: u32,
        stack: crate::items::ItemStack,
        at: [f64; 3],
        motion: [f64; 3],
        pickup_delay: u32,
    },
    /// `Goat.dropHorn`: a horn from goat `mob` (`Goat.createHorn` picks the
    /// instrument from its UUID), thrown with `motion` as
    /// `new ItemEntity(.., deltaX, deltaY, deltaZ)`.
    DropGoatHorn {
        mob: u32,
        screaming: bool,
        at: [f64; 3],
        motion: [f64; 3],
    },
    /// `Mob.dropFromGiftLootTable`: a gift loot table's items dropped at `at`.
    DropGift {
        table: &'static str,
        at: [f64; 3],
    },
    /// A leave-block goal's sampled candidate, before neighbor-shape updates.
    EndermanPlaceBlock {
        mob: u32,
        pos: [i32; 3],
        carried: horizon_world::registry::BlockStateId,
        occupants: Vec<horizon_core::geometry::Aabb>,
    },
    SetBlock {
        pos: [i32; 3],
        state: horizon_world::registry::BlockStateId,
    },
    /// A mob placing a block (such as a snow golem's trail): `entity_place`
    /// may refuse it. Enderman leave-block attempts use `EndermanPlaceBlock`.
    PlaceBlock {
        mob: u32,
        pos: [i32; 3],
        state: horizon_world::registry::BlockStateId,
    },
    /// `TurtleEggBlock.decreaseEggs` after a mob's step won its 1 in 100: the
    /// break sound, then one egg fewer, or the block destroyed when it was
    /// the last.
    CrackTurtleEgg {
        pos: [i32; 3],
    },
    /// `LivingEntity.startSleeping` / `stopSleeping`: the bed head at `pos`
    /// gets `occupied` (`setBlockAndUpdate`; the foot follows it).
    BedOccupied {
        pos: [i32; 3],
        occupied: bool,
    },
    /// `DoorBlock.setOpen` by a mob (`InteractWithDoor`): the door opens or
    /// closes, both halves, with its sound and game event.
    SetDoorOpen {
        pos: [i32; 3],
        open: bool,
    },
    /// `BreakDoorGoal.tick` at its end: the door is removed (`removeBlock`),
    /// then level events 1021 and 2001 with the state that is left there.
    BreakDoor {
        mob: u32,
        pos: [i32; 3],
    },
    /// `Level.destroyBlockProgress` for a mob's crack on a block (`-1`
    /// clears it).
    DestroyProgress {
        mob: u32,
        pos: [i32; 3],
        progress: i32,
    },
    /// `Rabbit.RaidGardenGoal.tick`: the carrots at `pos` become `state` (`setBlock` with
    /// flags 2); a crop that only lost an age shows the break particles of the state it had
    /// (`levelEvent(2001, pos, old)`) and fires `BLOCK_CHANGE` for the rabbit.
    CropRaided {
        pos: [i32; 3],
        state: horizon_world::registry::BlockStateId,
        old: Option<horizon_world::registry::BlockStateId>,
        mob: u32,
    },
    /// `Level.levelEvent`: the game plays and sends the event.
    LevelEvent {
        id: i32,
        pos: [i32; 3],
        data: i32,
    },
    /// `InfestedBlock.infestedStateByHost` (`infested`) or `hostStateByInfested`
    /// for the block there, keeping its axis; `poof` is the silverfish that
    /// hid in it (`spawnAnim`).
    Infest {
        pos: [i32; 3],
        infested: bool,
        poof: Option<u32>,
    },
    /// `Level.destroyBlock(pos, drop)`: the block gives way to its fluid's
    /// block (air when it has none). Sheep eating grass becomes dirt via SetBlock.
    BreakBlock {
        pos: [i32; 3],
        drop: bool,
        /// The mob whose breaking this is, when it is one `LivingDestroyBlockEvent`
        /// hears (a wither's, a ravager's, a silverfish's, a zombie's).
        by: Option<u32>,
        /// The entity `destroyBlock` names when that event doesn't hear of it (a
        /// villager's harvest, a ravager's crop): it asks the mods nothing, and
        /// its loot's `LootContextParams.THIS_ENTITY` is this one, as `by`'s is.
        breaker: Option<u32>,
    },
    /// `EndermanTakeBlockGoal`: the enderman `mob` picks up the block `state` at
    /// `pos` (it already holds it); the block turns to air unless a mod stops it
    /// or it is no longer that block.
    TakeBlock {
        mob: u32,
        pos: [i32; 3],
        state: horizon_world::registry::BlockStateId,
    },
    /// What follows a `BreakBlock { by: Some(mob) }` of this batch: `effects` when
    /// the game broke at least one of the mob's blocks, `otherwise` when a mod kept
    /// them all (vanilla plays the break's sound and particles only on a destruction).
    /// Only made while a mod is loaded; without one the effects are plain events.
    IfBroke {
        mob: u32,
        effects: Vec<MobEvent>,
        otherwise: Vec<MobEvent>,
    },
    /// The mob `mob` jumps (`jumpFromGround`'s flag, set after a ravager broke no leaves).
    Jump {
        mob: u32,
    },
    Particles {
        kind: &'static str,
        at: [f64; 3],
        count: u32,
    },
    /// Client particles about a mob's box (`particles::around_self`) and the
    /// entity event `event` that shows them, sent for mob `mob`.
    AroundSelf {
        mob: u32,
        event: i8,
        kind: &'static str,
        feet: [f64; 3],
        width: f64,
        height: f64,
        burst: crate::particles::Burst,
    },
    /// One particle exactly at `at`, moving at `velocity`.
    ParticleAt {
        kind: &'static str,
        at: [f64; 3],
        velocity: [f64; 3],
    },
    /// One `minecraft:entity_effect` particle at `at` in the colour `color`
    /// (`ColorParticleOption.create(ENTITY_EFFECT, r, g, b)`), standing still.
    ParticleColored {
        at: [f64; 3],
        color: [f32; 3],
    },
    Experience {
        at: [f64; 3],
        amount: u32,
    },
    /// Regeneration support after an axolotl-assisted kill, from the axolotl `mob`.
    AxolotlSupportPlayer {
        mob: u32,
    },
    /// A dolphin that was given a fish asks the game for the nearest
    /// shipwreck or ocean ruin (`DolphinSwimToTreasureGoal.start`): the game
    /// sets the dolphin's `extra.home` to it, or leaves it empty.
    DolphinTreasure {
        mob: u32,
    },
    /// Apply an effect to the player (witch potions, bee stings, cave spider poison).
    EffectPlayer {
        effect: &'static str,
        ticks: i32,
        amplifier: i32,
        /// `MobEffectInstance` ambient flag (the beacon-style translucent icon).
        ambient: bool,
    },
    /// Apply an effect to another mob: `IllusionerBlindnessSpellGoal.performSpellCasting`
    /// blinds whatever `LivingEntity` the illusioner targets, not only a player.
    EffectMob {
        target: u32,
        effect: &'static str,
        ticks: i32,
        amplifier: i32,
    },
    /// Set an entity on fire for so many seconds.
    Ignite(Who, i32),
    /// `Zombie.doHurtTarget`'s burn: lit only if the attack event before it hurt.
    ZombieIgnite(Who, i32),
    /// Two animals bred: the game spawns the baby and resets the partner.
    Breed {
        parent: u32,
        partner: u32,
        /// Equine inheritance, drawn at `getBreedOffspring`'s goal moment,
        /// before later same-tick healing and grazing rolls.
        equine: Option<Box<equine::Offspring>>,
        /// Whether the parent supplies the variant, if already drawn.
        parent_variant: Option<bool>,
        /// Wolf voice drawn before experience, unless baby-spawn approval defers inheritance.
        wolf_voice: Option<u8>,
        /// The parent-RNG experience decision, if already made.
        experience: BreedExperience,
    },
    /// Replace a block by name (sheep turning grass to dirt), optionally with
    /// the destroy effect.
    SetBlockName {
        mob: Option<u32>,
        pos: [i32; 3],
        name: &'static str,
        effect: bool,
        flags: i32,
    },
    /// A cat hisses (phantoms flee).
    Hiss {
        mob: u32,
    },
    /// A horse's rider tamed it.
    Tamed {
        mob: u32,
    },
    /// A horse throws its rider off.
    Buck {
        mob: u32,
    },
    /// `Entity.unRide`'s `ejectPassengers` before a move (a shulker's
    /// teleport): whoever rides `mob` gets off where it stood, `from`.
    EjectPassengers {
        mob: u32,
        from: Vec3,
    },
    /// A piglin's admiring is over: barter or keep the item.
    FinishAdmiring {
        mob: u32,
    },
    /// `IronGolem.doHurtTarget`: the hit, then, only when `hurtServer` let it
    /// land, the target thrown up by `0.4f` scaled by
    /// `max(0, 1 - KNOCKBACK_RESISTANCE)`.
    IronGolemHit {
        target: Who,
        amount: f32,
        source: Damage,
    },
    /// Damage the mob itself (snow golems melting).
    HurtSelf {
        mob: u32,
        kind: DamageKind,
        amount: f32,
    },
    /// Two villagers are close: swap gossip and share food and wares.
    VillagerMeet {
        mob: u32,
        other: u32,
    },
    /// `Villager.customServerAiStep`: deliver the pending TRADE reputation
    /// event for the actual player recorded by rewardTradeXp.
    VillagerTrade {
        mob: u32,
        player: [i32; 4],
    },
    /// `AbstractVillager.getOffers` for a villager whose offers are not made
    /// yet: the level rolls them (the brain has no trade data), so the
    /// behaviour that asked reads them from the next tick on.
    VillagerOffers {
        mob: u32,
    },
    /// `Villager.restock` / `catchUpDemand`: refresh the trading player.
    VillagerRestocked {
        mob: u32,
    },
    /// Two villagers make a baby (`VillagerMakeLove.tryToGiveBirth`): the
    /// parent has eaten and, with a bed for the child, rests; the partner
    /// eats, and rests too if `child` holds the child's bed and type
    /// (`None`: the biome's).
    VillagerBirth {
        parent: u32,
        partner: u32,
        child: Option<([i32; 3], Option<u8>)>,
    },
    /// `WorkAtComposter.useWorkstation`: a farmer at its composter makes
    /// bread and composts seeds (the game applies it to the villager's
    /// inventory and the block).
    WorkAtComposter {
        mob: u32,
        pos: [i32; 3],
    },
    /// A villager asks whether enough neighbours want an iron golem.
    GolemCheck {
        mob: u32,
        needed: usize,
    },
    /// A villager launches a firework rocket (celebrating a won raid).
    Firework {
        at: [f64; 3],
        color: usize,
        flight: u8,
    },
    /// A villager throws a hero's gift (a loot table) toward the player.
    ThrowGift {
        table: String,
        from: [f64; 3],
        to: [f64; 3],
    },
    /// A patrol leader hands its patrol target to a companion.
    PatrolTarget {
        mob: u32,
        target: [i32; 3],
    },
    /// A raider is told to attack (`HoldGroundAttackGoal` shouting).
    RaiderTarget {
        mob: u32,
        target: Who,
        aggressive: bool,
    },
    /// Drop the loot of a gift table (chicken eggs, sniffer seeds) with the
    /// mob as `this` (`LivingEntity.dropFromGiftLootTable`).
    GiftLoot {
        mob: u32,
        table: &'static str,
        at: [f64; 3],
    },
    /// `Chicken.aiStep`: the egg timer ran out. The `chicken_lay` gift table
    /// drops at the chicken; only when it dropped something does the egg sound
    /// play (at `pitch`) and `entity_place` reach listeners.
    LayEgg {
        mob: u32,
        at: [f64; 3],
        pitch: f32,
    },
    /// `Armadillo.customServerAiStep`: the armadillo sheds a scute.
    ShedScute {
        mob: u32,
        at: [f64; 3],
        pitch: f32,
    },
    /// A lead snaps or loses its holder: the lead drops when asked, at `at` or else the mob.
    LeashBreak {
        mob: u32,
        drop: bool,
        sound: bool,
        at: Option<[f64; 3]>,
    },
    /// `Leashable.whenLeashedTo`: the holder is told a quad-leashed mob hangs
    /// on it (`HappyGhast.notifyLeashHolder`).
    NotifyLeashHolder {
        holder: u32,
    },
    /// A zombie villager's cure is complete: the game turns it back into a villager.
    CureZombieVillager {
        mob: u32,
    },
    /// A vibration travelling to a listener (`VibrationParticleOption`).
    Vibration {
        from: [f64; 3],
        to: [f64; 3],
        ticks: i32,
        /// `VibrationSystem.User.getPositionSource`: the listener itself.
        source: horizon_protocol::entity_data::PositionSource,
    },
    /// The crumbs of the block under a digging or emerging warden.
    DigParticles {
        at: [f64; 3],
    },
    /// `Zombie.hurtServer`: a zombie hurt with a target in sight (its own,
    /// else the attacker) may call a reinforcement against it on Hard.
    Reinforcements {
        mob: u32,
        target: Who,
    },
    /// `LevelEvent.PARTICLES_DESTROY_BLOCK` (2001) at `pos` for the block at
    /// `block`: a turtle's digging.
    BlockBreakEffect {
        pos: [i32; 3],
        block: [i32; 3],
    },
    /// `Warden.applyDarknessAround`: darkness for survival players nearby.
    Darkness {
        at: [f64; 3],
        radius: f64,
    },
    /// A creaking bound to the heart at `home` was hurt (`creakingHurt` when a player did it).
    CreakingHurt {
        home: [i32; 3],
        by_player: bool,
        id: u32,
    },
    /// `Creaking.tearDown`: crumbling particles around the creaking.
    CreakingTearDown {
        center: [f64; 3],
        spread: [f64; 3],
    },
    /// `Panda.afterSneeze`: the grown pandas within 10 blocks start.
    PandaSneezed {
        mob: u32,
    },
    /// `Entity.propagateFallToPassengers`: the mob landed and what rides it
    /// (a player or a mob) takes `causeFallDamage` of the same fall.
    FallToPassengers {
        mob: u32,
        distance: f64,
        modifier: f32,
    },
}

impl MobEvent {
    /// Whether this event acts on the `Who::Player` seen by the mob.
    pub fn targets_player(&self) -> bool {
        match self {
            Self::AttackPlayer(..)
            | Self::CubeHurtPlayer { .. }
            | Self::AttackPlayerWithEffect { .. }
            | Self::DamagePlayer { .. }
            | Self::EffectPlayer { .. }
            | Self::AxolotlSupportPlayer { .. }
            | Self::ParrotToShoulder { .. } => true,
            Self::SonicBoom { target, .. }
            | Self::Push { target, .. }
            | Self::Knockback { target, .. }
            | Self::Sting { target, .. }
            | Self::StabDismount { target }
            | Self::Ignite(target, _)
            | Self::ZombieIgnite(target, _)
            | Self::IronGolemHit { target, .. } => *target == Who::Player,
            Self::Projectile(projectile) => matches!(
                &projectile.kind,
                crate::projectiles::ProjectileKind::ShulkerBullet {
                    target: Some(Who::Player),
                    ..
                }
            ),
            _ => false,
        }
    }
}
/// How a spawned-by-kind mob should be set up.
#[derive(Clone, Debug, Default)]
pub struct SpawnSetup {
    pub baby: bool,
    pub size: i32,
    pub yaw: f32,
    pub velocity: [f64; 3],
    pub health: Option<f32>,
    pub persistent: bool,
    pub variant: Option<u8>,
    /// Zombified piglins and zoglins wake up nauseous (`ConversionParams`).
    pub nausea: bool,
    /// Vexes: the summoning mob, the bound origin and the life in ticks.
    pub owner_mob: Option<u32>,
    pub home: Option<[i32; 3]>,
    pub life: Option<i32>,
    /// Whether to skip `finalizeSpawn`'s rolls (e.g. conversions or EntityType.create).
    pub skip_finalize: bool,
}
include!("mobs/seen.rs");

pub fn take_mob_pushes(events: &mut Vec<MobEvent>, from: usize, pending: &mut PendingPushes) {
    // One pass: a crowd's tick holds a push per neighbour, and removing each
    // in place would shift the rest every time.
    let mut kept = from;
    for i in from..events.len() {
        if let MobEvent::Push {
            target: Who::Mob(id),
            velocity,
        } = events[i]
        {
            pending.push(id, Vec3::new(velocity[0], velocity[1], velocity[2]));
        } else {
            events.swap(kept, i);
            kept += 1;
        }
    }
    events.truncate(kept);
}
/// Pushes waiting for their mob's tick, in push order. A list while short;
/// past [`PendingPushes::LIST_MAX`] (a crowd pushing each other) one list
/// per mob, as scanning the whole list for each mob grows with its square.
#[derive(Clone, Default)]
pub struct PendingPushes {
    list: Vec<(u32, Vec3)>,
    by_mob: crate::fast_hash::FxHashMap<u32, Vec<Vec3>>,
}
impl PendingPushes {
    /// Whether a push waits for mob `id`'s turn.
    pub fn waiting_for(&self, id: u32) -> bool {
        self.list.iter().any(|(t, _)| *t == id) || self.by_mob.contains_key(&id)
    }
    /// `other`'s pushes after these, each mob's in their order.
    pub fn append(&mut self, other: PendingPushes) {
        for (id, v) in other.list {
            self.push(id, v);
        }
        for (id, list) in other.by_mob {
            for v in list {
                self.push(id, v);
            }
        }
    }
    const LIST_MAX: usize = 64;
    /// Whether every queued push has been consumed or retired with its target.
    pub fn is_empty(&self) -> bool {
        self.list.is_empty() && self.by_mob.is_empty()
    }
    pub fn push(&mut self, id: u32, v: Vec3) {
        if self.by_mob.is_empty() && self.list.len() < Self::LIST_MAX {
            self.list.push((id, v));
            return;
        }
        // Older pushes first, so each mob's stay in order.
        for (id, v) in self.list.drain(..) {
            self.by_mob.entry(id).or_default().push(v);
        }
        self.by_mob.entry(id).or_default().push(v);
    }
}
#[cfg(test)]
mod pending_push_tests {
    use super::PendingPushes;
    use horizon_core::geometry::Vec3;

    /// Each mob's pushes, in the order they apply.
    fn per_mob(p: &PendingPushes) -> std::collections::BTreeMap<u32, Vec<[f64; 3]>> {
        let mut out = std::collections::BTreeMap::<u32, Vec<[f64; 3]>>::new();
        for (id, v) in &p.list {
            out.entry(*id).or_default().push([v.x, v.y, v.z]);
        }
        for (id, list) in &p.by_mob {
            out.entry(*id)
                .or_default()
                .extend(list.iter().map(|v| [v.x, v.y, v.z]));
        }
        out
    }

    /// Two lists of pushes, the second appended to the first, apply as one list of them all in
    /// that order, on either side of the switch from one list to a list per mob.
    #[test]
    fn appended_pushes_apply_in_order() {
        let pushes: Vec<(u32, Vec3)> = (0..200u32)
            .map(|k| (k * 7 % 11, Vec3::new(f64::from(k), 0.0, -f64::from(k))))
            .collect();
        let mut all = PendingPushes::default();
        for &(id, v) in &pushes {
            all.push(id, v);
        }
        for split in [0, 30, 64, 70, 150, 200] {
            let (mut first, mut second) = (PendingPushes::default(), PendingPushes::default());
            for &(id, v) in &pushes[..split] {
                first.push(id, v);
            }
            for &(id, v) in &pushes[split..] {
                second.push(id, v);
            }
            first.append(second);
            assert_eq!(per_mob(&first), per_mob(&all), "split at {split}");
        }
    }
}
/// The writes of `writes` that changed the block's collision shape, in order:
/// `sendBlockUpdated` compares the two shapes once per write, not per mob.
pub fn collision_changes(w: &MobWorld, writes: &[((i32, i32, i32), u32, u32)]) -> Vec<[i32; 3]> {
    writes
        .iter()
        .filter_map(|&((x, y, z), old, new)| {
            let at = [x, y, z];
            let shape =
                |state| w.collision_bounds_of(horizon_world::registry::BlockStateId(state), at);
            (shape(old) != shape(new)).then_some(at)
        })
        .collect()
}
/// The player as mobs see it.
#[derive(Clone, Debug)]
pub struct PlayerView {
    pub position: Vec3,
    pub bounds: Aabb,
    pub eye: [f64; 3],
    pub alive: bool,
    /// The `invulnerable` ability, the `Invulnerable` flag or its window,
    /// which `asValidTarget` refuses a target by (arismoko/horizon#1175, arismoko/horizon#916): a
    /// valid-target rule a goal tick's view asks.
    pub invulnerable: bool,
    pub creative: bool,
    pub sneaking: bool,
    /// `Entity.isSwimming`.
    pub swimming: bool,
    pub look: [f64; 3],
    pub main_hand: Option<Arc<str>>,
    pub off_hand: Option<Arc<str>>,
    pub head: Option<Arc<str>>,
    /// `getItemBySlot` for chest, legs and feet.
    pub chest: Option<Arc<str>>,
    pub legs: Option<Arc<str>>,
    pub feet: Option<Arc<str>>,
    pub armor_cover: f32,
    /// The Invisibility effect (`isInvisible`).
    pub invisible: bool,
    pub spectator: bool,
    /// `PiglinAi.isWearingSafeArmor`: some golden armour.
    pub gold_armor: bool,
    /// `getLastHurtByMob` (within 100 ticks) and when.
    pub hurt_by: Option<(Who, i64)>,
    /// `getLastHurtMob` and when.
    pub last_hurt: Option<(Who, i64)>,
    pub velocity: Vec3,
    pub yaw: f32,
    /// `Player.isSprinting` and `isPassenger` (what scares an armadillo).
    pub sprinting: bool,
    pub riding: bool,
    /// The Hero of the Village effect.
    pub hero: bool,
    /// The active effects, without namespace (`slowness`, `poison`...).
    pub effects: Vec<&'static str>,
    /// The player's UUID (villagers' gossip is keyed by it).
    pub uuid: [i32; 4],
    /// The mobs on the player's team (`isAlliedTo`): combat targeting leaves the player alone.
    pub allies: std::sync::Arc<std::collections::HashSet<u32>>,
    /// A LAN guest: its own client moves it, and touching mobs push it there
    /// (`EntitySelector.pushableBy` on the client).
    pub remote: bool,
    /// `isSleeping` and `getSleepTimer` (100 once slept through, then the
    /// ticks fading out after waking up).
    pub sleeping: bool,
    pub sleep_timer: u32,
    /// `isPassenger`.
    pub passenger: bool,
    /// The vehicle entity id, including mounts without a `Vehicle` follow view.
    pub riding_vehicle: Option<u32>,
    /// The boat or nautilus the player rides and steers
    /// (`getControllingPassenger`).
    pub vehicle: Option<Vehicle>,
}
/// What a player rides, for the mobs that follow it (`FollowPlayerRiddenEntityGoal`).
#[derive(Clone, Debug)]
pub struct Vehicle {
    pub nautilus: bool,
    pub bounds: Aabb,
    /// `Entity.getKnownSpeed`: what it moved in its last tick.
    pub velocity: Vec3,
}
impl PlayerView {
    /// `LivingEntity.getVisibilityPercent(level, looking)` before the visibility
    /// event and the clamp: sneaking, invisibility by armour cover, and the
    /// worn head's `mob_visibility` for the `looking` mob's type.
    pub fn visibility_percent(&self, looking: Species) -> f64 {
        let mut visibility = 1.0;
        if self.sneaking {
            visibility *= 0.8;
        }
        if self.invisible {
            visibility *= 0.7 * f64::from(self.armor_cover).max(0.1);
        }
        if let Some(head) = self.head.as_deref() {
            visibility *= f64::from(mob_visibility(head, looking));
        }
        visibility
    }
    /// `LivingEntity.hasEffect`, by id without namespace.
    pub fn has_effect(&self, effect: &str) -> bool {
        self.effects.contains(&effect)
    }
}

/// Read-only access to the live mobs outside the actor's current tick loan.
pub trait LiveMobs {
}
#[derive(Clone)]
pub struct MobWorld<'a> {
    pub mob_maps: &'a crate::neoforge_mob_maps::NeoForgeMobMaps,
    pub professions: &'a villager::ResolvedProfessions,
    /// What the motion value hooks are given: the loop context of the side that
    /// ticks these mobs.
    pub loop_context: &'a horizon_world::native_load_context::LoopContext,
    pub world: &'a World,
    pub registry: &'a horizon_world::registry::BlockRegistry,
    pub facts: &'a BlockFacts,
    /// Fluid states and flow facts, for the currents that push mobs.
    pub fluid_tables: &'a crate::motion::FluidResources,
    /// The owning side's attribute definitions for species-specific writes.
    pub attributes: &'a Arc<horizon_world::attributes::entity::Catalog>,
    pub movement: &'a horizon_world::movement_properties::BoundMovementProperties,
    pub collision: &'a BlockCollisionRegistry<'a>,
    pub suffocation: &'a horizon_world::suffocation::SuffocationRegistry<'a>,
    pub data: &'a MobData,
    pub player: Option<PlayerView>,
    /// Switchable per-tick player views, present only when target listeners
    /// can redirect a mob to a different real player.
    pub player_selection: Option<&'a player_selection::PlayerSelection>,
    /// The uuid and entity id of the player whose view `player` is (arismoko/horizon#1077):
    /// captured once where the tick lends its player — the game's own, or the
    /// guest a server's mob sees — so a goal resolves that player without a
    /// scan, and a fake player's view resolves as none. None when `player` is
    /// None.
    pub player_identity: Option<([i32; 4], u32)>,
    /// `getKnownMovement` of a mob's controlling player passenger, if any.
    pub controlling_player_movement: Option<Vec3>,
    /// `getNearestPlayer(mob, -1)`: where the nearest non-spectator of every
    /// player in the level stands, alive or not (the despawn distance).
    pub nearest_player: Option<Vec3>,
    pub others: &'a [Seen],
    /// Retained passenger objects for topology only, never targets or colliders.
    pub passenger_objects: &'a [Seen],
    /// Disjoint live mob state, available only while an actor is ticked.
    pub live_mobs: Option<&'a dyn LiveMobs>,
    /// The separately simulated dragon, visible to revenge and melee goals.
    pub dragon: Option<(u32, TargetInfo)>,
    /// The boats and rafts a lead can hang on (`Holder::Boat`): id and state.
    /// `others` by position, when the caller keeps one (the mob ticks).
    pub others_grid: Option<&'a seen_grid::SeenGrid>,
    /// Where each id stands in `others`, when the caller keeps one.
    pub others_index: Option<&'a crate::fast_hash::FxHashMap<u32, u32>>,
    /// The slots of `others` holding a villager, in order, when the caller
    /// keeps them (the mob ticks: a slot keeps its mob, and its species, all
    /// tick). Each zombie looking for a village read every slot.
    pub others_villagers: Option<&'a [u32]>,
    /// The slots of `others` by species, when the caller keeps them (the mob loops).
    pub species_slots: Option<&'a species_slots::SpeciesSlots>,
    /// The slots of `others` that may stop a moving mob
    /// ([`Seen::may_block_mobs`]), when the caller keeps them; None: any.
    pub others_solid: Option<&'a [u32]>,
    pub daylight: bool,
    /// The `gameplay/monsters_burn` attribute: the overworld day timeline's
    /// step track, false in every other dimension.
    pub monsters_burn: bool,
    pub game_time: i64,
    pub day_time: i64,
    pub raining: bool,
    pub thundering: bool,
    pub level: horizon_core::LevelKey,
    pub block_tags: &'a HashMap<String, std::collections::HashSet<String>>,
    /// Each block's `fallOn` rule (the player's landing registry).
    pub landing: &'a horizon_world::block_landing::BlockLandingRegistry,
    pub biomes: &'a crate::biome::Biomes,
    /// The `mob_griefing` game rule.
    pub mob_griefing: bool,
    /// The `universal_anger` game rule (`NeutralMob.isAngryAtAllPlayers`).
    pub universal_anger: bool,
    /// The `mob_drops` game rule (`shouldDropLoot`).
    pub mob_drops: bool,
    /// The `max_entity_cramming` game rule.
    pub max_entity_cramming: i32,
    /// `getCurrentDifficultyAt` at the mob being ticked (near the player
    /// outside the mob tick).
    pub difficulty: spawning::LocalDifficulty,
    /// The level's dimension type.
    pub dimension_type: &'a horizon_world::dimension_type::DimensionFacts,
    /// The world seed (slime chunks).
    pub world_seed: i64,
    /// `Level.getSeaLevel` (63 in the overworld, -63 in a flat world).
    pub sea_level: i32,
    /// The scoreboard teams of the player and the mobs (collision rules),
    /// shared: the mob ticks build them once a tick, not once a mob.
    pub teams: std::sync::Arc<Teams<'a>>,
    /// Item data (max stack sizes), when the host has it.
    pub items: Option<&'a crate::items::Items>,
    /// `ChestBlockEntity.getOpenCount`: the players with each chest open.
    pub lid_openers: &'a HashMap<(i32, i32, i32), i32>,
    /// What a content mod's effects do, on a server (None on a client).
    pub mod_effects: Option<&'a dyn ModEffectTicks>,
    /// The mod hooks the AI asks mid-tick, on a server where a mod listens
    /// (None otherwise: the AI asks nothing).
    pub mob_hooks: Option<&'a dyn MobHooks>,
    /// Whether a listener can cancel animal taming.
    pub asks_animal_tame: bool,
    /// Whether a listener can cancel a mob's block destruction.
    pub asks_living_destroy_block: bool,
    /// Whether a server listener can cancel baby spawning.
    pub asks_baby_spawn: bool,
    /// `Level.getEntity(uuid)`, for the UUIDs a mob kept of entities that were
    /// not in the level (a warden's anger, `AngerManagement.angerByUuid`).
    /// None: this world keeps no entity UUIDs, so nothing is resolved.
    pub who_lookup: Option<&'a dyn WhoLookup>,
    /// Another thread's mobs, for a lookup by id that misses `others`.
    pub foreign: Option<&'a dyn ForeignSeen>,
    /// The level's world border, whose walls stop a mob's moves
    /// (`Entity.collectColliders`); None: nothing stops them there.
    pub border: Option<&'a horizon_core::geometry::border::BorderGeometry>,
    pub section_writer: Option<living::section_writes::Observer>,
}
/// `MobEffectInstance.tickServer` for a content mod's effect: its
/// `shouldApplyEffectTickThisTick` and `applyEffectTick`, which the game's
/// native mods answer. False removes the effect.
pub trait ModEffectTicks {
    /// `MobEffectEvent.Expired`: `mob`'s `effect` (`amplifier` from 0)
    /// counted down to nothing. A mod that cancels refuses the expiry: the
    /// effect stays at no ticks and asks again next tick.
    fn effect_expired(&self, mob: u32, effect: usize, amplifier: i32, cancel: &mut bool);
}
mod mob_hooks;
pub use mob_hooks::MobHooks;
/// Outcome of a target-change event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetChange {
    /// Apply the resulting target.
    Set(Option<Who>),
    /// Keep the previous target.
    Keep,
}
/// The block whose noise cell `BiomeManager.getBiome` reads for the block `x, y, z` (the zoom's quart, as a block).
pub fn zoomed_block(zoom: horizon_core::biome::BiomeZoom, x: i32, y: i32, z: i32) -> [i32; 3] {
    let [qx, qy, qz] = zoom.quart_at_block(horizon_core::position::BlockPos::new(x, y, z));
    [qx << 2, qy << 2, qz << 2]
}
/// The biome index (`mw.biomes`) that `BiomeManager.getBiome` reads at a block; `None` where the chunk is not loaded.
pub fn zoomed_biome_index(mw: &MobWorld, zoom: horizon_core::biome::BiomeZoom, x: i32, y: i32, z: i32) -> Option<u16> {
    let [x, y, z] = zoomed_block(zoom, x, y, z);
    mw.world.noise_biome(x, y, z)
}
impl MobWorld<'_> {
    /// The currently selected player view. The ordinary tick uses `player`;
    /// target-change callbacks can switch a scoped selection during a tick.
    pub fn selected_player(&self) -> Option<&PlayerView> {
        match self.player_selection {
            Some(selection) => selection.selected().map(|player| &player.view),
            None => self.player.as_ref(),
        }
    }

    /// UUID and entity ID of the currently selected player, if any.
    pub fn selected_player_identity(&self) -> Option<([i32; 4], u32)> {
        match self.player_selection {
            Some(selection) => selection
                .selected()
                .map(|player| (player.view.uuid, player.entity_id)),
            None => self.player_identity,
        }
    }

    /// Capture a player-directed event's recipient after a listener switches
    /// the selected player. Before the first switch retain the plain event and
    /// the existing allocation-free routing path.
    pub fn route_player_event(&self, event: MobEvent) -> MobEvent {
        if event.targets_player()
            && let Some(selection) = self.player_selection
            && selection.has_switched()
            && let Some(player) = selection.selected()
        {
            return MobEvent::ForPlayer {
                conn: player.connection,
                event: Box::new(event),
            };
        }
        event
    }

    /// An action directed at the player already bound at tick start, such as
    /// retaliation against the attacker, must not follow a later target change.
    pub fn route_initial_player_event(&self, event: MobEvent) -> MobEvent {
        if event.targets_player()
            && let Some(selection) = self.player_selection
            && selection.has_switched()
            && let Some(player) = selection.initial()
        {
            return MobEvent::ForPlayer {
                conn: player.connection,
                event: Box::new(event),
            };
        }
        event
    }

    /// Keep a mount's rider as the recipient even when a target-change hook
    /// redirects the mount to a different player later in the same tick.
    /// Before any switch the existing allocation-free routing is unchanged.
    pub fn route_rider_event(&self, mount: u32, event: MobEvent) -> MobEvent {
        if event.targets_player()
            && let Some(selection) = self.player_selection
            && selection.has_switched()
            && let Some(rider) = selection.rider(mount).or_else(|| selection.initial())
        {
            return MobEvent::ForPlayer {
                conn: rider.connection,
                event: Box::new(event),
            };
        }
        event
    }

    /// Whether a mob may grief here at all: the rule, or a mod that may say
    /// yes against it.
    pub fn griefing_possible(&self) -> bool {
        self.mob_griefing || self.mob_hooks.is_some_and(|hooks| hooks.grief_hooked())
    }
    /// `EventHooks.canEntityGrief(level, mob)`: the `mob_griefing` rule, or
    /// what a mod answers for this mob (`native_entity_mob_griefing`); the
    /// places vanilla reads the rule with no entity still read `mob_griefing`.
    pub fn can_grief(&self, mob: u32) -> bool {
        match self.mob_hooks {
            Some(hooks) => hooks.can_grief(mob, self.mob_griefing),
            None => self.mob_griefing,
        }
    }
    /// `LivingEntity.getVisibilityPercent(level, looking)` of the player `p`:
    /// `visibility` holds vanilla's factors; NeoForge's `LivingVisibilityEvent`
    /// scales it last, before the clamp to 0..=10.
    pub fn player_visibility(&self, p: &PlayerView, looking: u32, visibility: f64) -> f64 {
        let visibility = match self.mob_hooks {
            Some(hooks) => hooks.living_visibility(p, looking, visibility),
            None => visibility,
        };
        visibility.clamp(0.0, 10.0)
    }
}
/// `Level.getEntity(UUID)` for a reference a mob stored: the entity that UUID
/// names while it is in the level, never some other entity as a stand-in. Only
/// the game layer keeps entity UUIDs, so it answers this (`Game::who_by_uuid`)
/// and the mob layer reaches it through `MobWorld`.
/// A mob outside this thread's part of the mob loop, as the serial loop would see it now.
pub trait ForeignSeen {
    fn seen(&self, id: u32) -> Option<&Seen>;
}
pub trait WhoLookup {
    fn who_by_uuid(&self, uuid: [i32; 4]) -> Option<Who>;
}
/// `Entity.getTeam` of the player and of the mobs on a team.
#[derive(Clone, Debug, Default)]
pub struct Teams<'a> {
    pub player: Option<&'a crate::scoreboard::Team>,
    pub mobs: HashMap<u32, &'a crate::scoreboard::Team>,
}
impl<'a> Teams<'a> {
    pub fn of(&self, who: Who) -> Option<&'a crate::scoreboard::Team> {
        match who {
            Who::Player => self.player,
            Who::Mob(id) => self.mobs.get(&id).copied(),
        }
    }
}
/// `InfestedMobEffect.onMobHurt`'s draws from the bearer's random
/// (`mob.getRandom()`): a 10% chance of one or two silverfish, each thrown
/// along `look` (`spawnSilverfish`). Their velocities; none when the chance
/// fails.
pub fn infested_silverfish(random: &mut impl RandomSource, look: [f64; 3]) -> Vec<Vec3> {
    if random.next_float() > 0.1 {
        return Vec::new();
    }
    let count = 1 + random.next_int_bound(2).unwrap_or(0);
    (0..count)
        .map(|_| {
            // Mth.randomBetween(-PI / 2, PI / 2), then in floats (JOML
            // Vector3f): the look direction `.mul(0.3f).mul(1, 1.5f, 1)`,
            // `.rotateY(angle)`.
            let half = std::f32::consts::FRAC_PI_2;
            let angle = random.next_float() * (2.0 * half) - half;
            let sin = horizon_assets::math::joml_sin(angle);
            let cos = horizon_assets::math::cos_from_sin(sin, angle);
            let x = look[0] as f32 * 0.3;
            let y = look[1] as f32 * 0.3 * 1.5;
            let z = look[2] as f32 * 0.3;
            Vec3::new(
                f64::from(x * cos + z * sin),
                f64::from(y),
                f64::from(-x * sin + z * cos),
            )
        })
        .collect()
}
/// `Entity.random` of a client's copy of entity `id` first seen at `position`: vanilla's is its
/// own (`RandomSource.create()`), drawn on by nothing else; this one is seeded from the two.
pub fn entity_random(id: u32, position: Vec3) -> EntityRandom {
    let mut random = LegacyRandom::new(
        i64::from(id).wrapping_mul(0x5DEE_CE66D)
            ^ (position.x.to_bits() as i64)
            ^ (position.z.to_bits() as i64).rotate_left(17),
    );
    let _ = random.next_int();
    EntityRandom::own(random)
}
/// The own random (`Entity.random`) of a living entity the server makes, as
/// `LivingEntity.<init>` leaves it: its first two draws made the UUID
/// (`Mth.createInsecureUUID`), returned here, then the kind's constructor draws its synced data and yaw in `constructor_draws`.
pub fn living_random() -> (EntityRandom, [i32; 4]) {
    let mut random = crate::random_source::entity();
    let uuid = crate::save::insecure_uuid(&mut random);
    (EntityRandom::own(random), uuid)
}
/// A parsed `entity-models.json` document, shared while loading model batches.
/// Each lookup still constructs an independent part tree for the caller.
#[derive(Clone)]
pub struct EntityModelDocument(Arc<Value>);

impl EntityModelDocument {
    pub fn load(root: &Path) -> crate::Result<Self> {
        Ok(Self(Arc::new(serde_json::from_slice(&fs::read(
            root.join("entity-models.json"),
        )?)?)))
    }

    pub fn model(&self, key: &str) -> Option<Part> {
        self.0.get(key).map(|m| Part::parse("root", m))
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.0
            .as_object()
            .into_iter()
            .flat_map(|all| all.keys().map(String::as_str))
    }

    /// The parsed document, for loaders that read several keys from it.
    pub fn value(&self) -> &Value {
        &self.0
    }
}



/// `LivingEntity.elytraAnimationState`: eased wing angles and render interpolation.
#[derive(Clone, Copy, Debug, Default)]
pub struct ElytraAnimation {
    pub old: [f32; 3],
    pub angles: [f32; 3],
}
impl ElytraAnimation {
    /// The angles `ElytraAnimationState` eases to when not gliding or crouching.
    pub const RESTING: [f32; 3] = [
        std::f32::consts::PI / 12.0,
        0.0,
        -std::f32::consts::PI / 12.0,
    ];
    pub fn tick(&mut self, flying: bool, crouching: bool, velocity: horizon_core::geometry::Vec3) {
        use std::f32::consts::PI;
        self.old = self.angles;
        let target = if flying {
            let length =
                (velocity.x * velocity.x + velocity.y * velocity.y + velocity.z * velocity.z)
                    .sqrt();
            let ratio = if velocity.y < 0.0 && length > 0.0 {
                1.0 - horizon_core::jmath::pow(-velocity.y / length, 1.5) as f32
            } else {
                1.0
            };
            [
                PI / 12.0 + (PI / 9.0 - PI / 12.0) * ratio,
                0.0,
                -PI / 12.0 + (-PI / 2.0 + PI / 12.0) * ratio,
            ]
        } else if crouching {
            [PI * 2.0 / 9.0, 0.08726646, -PI / 4.0]
        } else {
            Self::RESTING
        };
        for (angle, target) in self.angles.iter_mut().zip(target) {
            *angle += (target - *angle) * 0.3;
        }
    }
    pub fn angles(&self, partial: f32) -> [f32; 3] {
        std::array::from_fn(|i| self.old[i] + (self.angles[i] - self.old[i]) * partial)
    }
}

/// `LivingEntity.swimAmount` and the render state the swimming pose reads
/// (`isVisuallySwimming`, `isInWater`).
#[derive(Clone, Copy, Debug, Default)]
pub struct SwimAnimation {
    pub old: f32,
    pub amount: f32,
    /// `isVisuallySwimming`: in the swimming pose (or still in the gliding
    /// pose after a glide ended).
    pub visually: bool,
    pub in_water: bool,
}
impl SwimAnimation {
    /// `LivingEntity.updateSwimAmount`.
    pub fn tick(&mut self, visually: bool, in_water: bool) {
        self.old = self.amount;
        self.amount = if visually {
            (self.amount + 0.09).min(1.0)
        } else {
            (self.amount - 0.09).max(0.0)
        };
        self.visually = visually;
        self.in_water = in_water;
    }
    /// `LivingEntity.getSwimAmount`.
    pub fn amount(&self, partial: f32) -> f32 {
        self.old + partial * (self.amount - self.old)
    }
}


#[cfg(test)]
mod elytra_tests {
    use super::*;
    #[test]
    fn wings_ease_between_ground_flight_and_crouch() {
        let mut wings = ElytraAnimation::default();
        wings.tick(false, false, Vec3::ZERO);
        assert!((wings.angles[0] - std::f32::consts::PI / 12.0 * 0.3).abs() < 0.000001);
        assert_eq!(wings.angles(0.0), wings.old);
        assert_eq!(wings.angles(1.0), wings.angles);
        for _ in 0..40 {
            wings.tick(true, false, Vec3::new(1.0, 0.0, 0.0));
        }
        assert!((wings.angles[2] + std::f32::consts::FRAC_PI_2).abs() < 0.00001);
        for _ in 0..40 {
            wings.tick(false, true, Vec3::ZERO);
        }
        assert!((wings.angles[1] - 0.08726646).abs() < 0.00001);
    }
    /// An armour stand never glides or crouches: its wings stay at the angles
    /// a standing wearer eases to.
    #[test]
    fn resting_wings_are_the_standing_target() {
        let mut wings = ElytraAnimation::default();
        for _ in 0..80 {
            wings.tick(false, false, Vec3::ZERO);
        }
        for (angle, resting) in wings.angles.iter().zip(ElytraAnimation::RESTING) {
            assert!((angle - resting).abs() < 0.00001);
        }
    }
}
#[cfg(test)]
mod spear_knockback_tests {
    use super::sensor_scan_tests::kind;
    use super::*;
    use horizon_core::geometry::Vec3;


}
#[cfg(test)]
mod teams_tests {
    use super::*;
    use crate::save::uuid_text;
    use crate::scoreboard::{Scoreboard, Team};

    /// A board where every other mob is on the player's team, as with a
    /// datapack that colours its mobs' names.
    fn board(mobs: u32) -> (Scoreboard, HashMap<u32, [i32; 4]>) {
        let mut board = Scoreboard::default();
        board.add_team("red");
        board.add_to_team("Steve", "red");
        let uuids: HashMap<u32, [i32; 4]> = (0..mobs)
            .map(|id| (id, [id as i32, 7, -(id as i32), 11]))
            .collect();
        for (id, uuid) in &uuids {
            if id % 2 == 0 {
                board.add_to_team(&uuid_text(*uuid), "red");
            }
        }
        (board, uuids)
    }
    /// `Game::mob_teams` as built for each mob before: owned clones of the teams.
    fn owned(board: &Scoreboard, uuids: &HashMap<u32, [i32; 4]>) -> HashMap<u32, Team> {
        uuids
            .iter()
            .filter_map(|(id, u)| Some((*id, board.players_team(&uuid_text(*u))?.clone())))
            .collect()
    }
    fn borrowed<'a>(board: &'a Scoreboard, uuids: &HashMap<u32, [i32; 4]>) -> Teams<'a> {
        Teams {
            player: board.players_team("Steve"),
            mobs: uuids
                .iter()
                .filter_map(|(id, u)| Some((*id, board.players_team(&uuid_text(*u))?)))
                .collect(),
        }
    }

    #[test]
    fn borrowed_teams_answer_as_the_clones() {
        let (board, uuids) = board(40);
        let teams = borrowed(&board, &uuids);
        let old = owned(&board, &uuids);
        for id in 0..41 {
            assert_eq!(teams.of(Who::Mob(id)), old.get(&id), "mob {id}");
        }
        assert_eq!(teams.of(Who::Player).map(|t| t.name.as_str()), Some("red"));
    }

    /// One tick's worth of team building: once per mob (before) against once.
    #[test]
    #[ignore]
    fn teams_cost() {
        for mobs in [100u32, 250, 500] {
            let (board, uuids) = board(mobs);
            let t = std::time::Instant::now();
            for _ in 0..mobs {
                std::hint::black_box(owned(&board, &uuids));
            }
            let before = t.elapsed();
            let t = std::time::Instant::now();
            let reps = 20;
            for _ in 0..reps {
                std::hint::black_box(borrowed(&board, &uuids));
            }
            let after = t.elapsed() / reps;
            println!(
                "{mobs} mobs, half on a team: per tick {:.3} ms -> {:.3} ms",
                before.as_secs_f64() * 1e3,
                after.as_secs_f64() * 1e3
            );
        }
    }
}
#[cfg(test)]
pub(crate) mod sensor_scan_tests {
    use super::*;
    use horizon_core::geometry::Vec3;
    use serde_json::json;

    pub(crate) fn kind() -> Arc<MobType> {
        Arc::new(MobType {
            shadow: None,
            id: "minecraft:cow".into(),
            species: Species::Cow,
            width: 0.9,
            height: 1.4,
            eye_height: 1.3,
            category: "creature".into(),
            max_health: 10.0,
            speed: 0.2,
            flying_speed: 0.4,
            attack_damage: 0.0,
            attack_knockback: 0.0,
            follow_range: 16.0,
            step_height: 0.6,
            armor: 0.0,
            armor_toughness: 0.0,
            knockback_resistance: 0.0,
            jump_strength: 0.42,
            attributes: Arc::default(),
            item_stack_limits: Default::default(),
            attribute_bounds: Default::default(),
            gravity: 0.08,
            safe_fall_distance: 3.0,
            fall_damage_multiplier: 1.0,
            water_efficiency: 0.0,
            water_slowdown: 0.8,
            air_speed: 0.02,
            baby_scale: 0.5,
            head_rot_speed: 10.0,
            max_head_x_rot: 40.0,
            max_head_y_rot: 75.0,
            ambient_interval: 120,
            flags: Flags {
                animal: true,
                ..Default::default()
            },
            model: Arc::new(Part::parse("root", &json!({}))),
            layers: Default::default(),
            texture: String::new(),
            brain: Brain::Passive,
            anim: Anim::Rabbit,
            sounds: [""; 3],
            sound_overrides: Default::default(),
            food: Arc::default(),
            tempt: Arc::default(),
            passenger_attachments: Vec::new(),
        })
    }

    /// `NearestLivingEntitySensor` over every mob, as the brains ran it.
    fn scan(seen: &[Seen], me: &Seen, area: Aabb) -> Vec<u32> {
        seen.iter()
            .filter(|o| o.id != me.id && o.alive && o.bounds.intersects(area))
            .map(|o| o.id)
            .collect()
    }
    fn indexed(seen: &[Seen], grid: &seen_grid::SeenGrid, me: &Seen, area: Aabb) -> Vec<u32> {
        let mut near = Vec::new();
        assert!(grid.candidates(area, &mut near));
        near.iter()
            .map(|&i| &seen[i as usize])
            .filter(|o| o.id != me.id && o.alive && o.bounds.intersects(area))
            .map(|o| o.id)
            .collect()
    }

    #[test]
    fn the_grid_finds_what_the_scan_finds() {
        let (seen, grid) = herd(600, 160.0);
        for me in &seen {
            let area = me.bounds.inflate(Vec3::new(16.0, 16.0, 16.0));
            assert_eq!(indexed(&seen, &grid, me, area), scan(&seen, me, area));
        }
    }

    /// `MobWorld::seen` once per mob (a target, a breeding partner...), by
    /// scan as before and by the tick's id index.
    #[test]
    #[ignore]
    fn lookup_cost() {
        for n in [500u32, 1000, 2000] {
            let (seen, _) = herd(n, 256.0);
            let index: crate::fast_hash::FxHashMap<u32, u32> = seen
                .iter()
                .enumerate()
                .map(|(i, s)| (s.id, i as u32))
                .collect();
            let wanted: Vec<u32> = (0..n).map(|i| (i * 7919) % n).collect();
            let t = std::time::Instant::now();
            let a: usize = wanted
                .iter()
                .filter_map(|&id| seen.iter().find(|s| s.id == id))
                .map(|s| s.id as usize)
                .sum();
            let before = t.elapsed().as_secs_f64() * 1e3;
            let t = std::time::Instant::now();
            let b: usize = wanted
                .iter()
                .filter_map(|&id| index.get(&id).map(|&i| &seen[i as usize]))
                .map(|s| s.id as usize)
                .sum();
            let after = t.elapsed().as_secs_f64() * 1e3;
            assert_eq!(a, b);
            println!(
                "{n} lookups over {n} mobs ({} B each): {before:.3} ms -> {after:.3} ms",
                std::mem::size_of::<Seen>()
            );
        }
    }


    /// One tick of sensing: a twentieth of the mobs fire their sensor.
    #[test]
    #[ignore]
    fn sensor_cost() {
        for (n, side) in [(500u32, 256.0), (1000, 384.0), (2000, 512.0)] {
            let (seen, grid) = herd(n, side);
            let time = |f: &dyn Fn(&Seen, Aabb) -> usize| {
                let t = std::time::Instant::now();
                let mut found = 0;
                for me in &seen {
                    found += f(me, me.bounds.inflate(Vec3::new(16.0, 16.0, 16.0)));
                }
                (t.elapsed().as_secs_f64() * 1e3 / 20.0, found)
            };
            let (before, a) = time(&|me, area| scan(&seen, me, area).len());
            let (after, b) = time(&|me, area| indexed(&seen, &grid, me, area).len());
            assert_eq!(a, b);
            println!("{n} mobs: per tick {before:.3} ms -> {after:.3} ms");
        }
    }
}
#[cfg(test)]
mod push_tests {
    use super::sensor_scan_tests::kind;
    use super::*;
    use horizon_core::geometry::Vec3;

    /// The pending pushes as one list, scanned for each mob, as before.
    fn take_old(events: &mut Vec<MobEvent>, from: usize, pending: &mut Vec<(u32, Vec3)>) {
        let mut i = from;
        while i < events.len() {
            if let MobEvent::Push {
                target: Who::Mob(id),
                velocity,
            } = events[i]
            {
                pending.push((id, Vec3::new(velocity[0], velocity[1], velocity[2])));
                events.remove(i);
            } else {
                i += 1;
            }
        }
    }
    fn apply_old(velocity: &mut Vec3, id: u32, pending: &mut Vec<(u32, Vec3)>) {
        pending.retain(|(to, v)| {
            if *to != id {
                return true;
            }
            *velocity = Vec3::new(velocity.x + v.x, velocity.y + v.y, velocity.z + v.z);
            false
        });
    }
    /// A crowd tick: each mob takes its pushes, then pushes every other
    /// mob (and the player, and hisses now and then); then all take theirs.
    fn crowd_tick(
        n: u32,
        tick: u32,
        mut on_push: impl FnMut(&mut Vec<MobEvent>, usize),
        mut on_apply: impl FnMut(u32),
    ) -> Vec<MobEvent> {
        let mut events = Vec::new();
        for i in 0..n {
            on_apply(i);
            let from = events.len();
            for j in 0..n {
                if j != i && !(i * 31 + j * 17 + tick).is_multiple_of(5) {
                    let f = f64::from(i * 7 + j + tick) * 0.001;
                    events.push(MobEvent::Push {
                        target: Who::Mob(j),
                        velocity: [f, -f * 0.5, f * 0.25],
                    });
                }
                if j.is_multiple_of(9) {
                    events.push(MobEvent::Hiss { mob: j });
                }
            }
            events.push(MobEvent::Push {
                target: Who::Player,
                velocity: [0.1, 0.0, 0.1],
            });
            on_push(&mut events, from);
        }
        for i in 0..n {
            on_apply(i);
        }
        events
    }

    #[test]
    fn pushes_land_as_with_the_list() {
        for n in [1u32, 3, 8, 20, 60] {
            let mut new_mobs = mobs(n);
            let mut old_v = vec![Vec3::ZERO; n as usize];
            let mut pending = PendingPushes::default();
            let mut old_pending = Vec::new();
            for tick in 0..4 {
                let new_events = {
                    let pending = std::cell::RefCell::new(&mut pending);
                    let mobs = std::cell::RefCell::new(&mut new_mobs);
                    crowd_tick(
                        n,
                        tick,
                        |e, from| take_mob_pushes(e, from, &mut pending.borrow_mut()),
                        |i| {
                            apply_mob_pushes(
                                &mut mobs.borrow_mut()[i as usize],
                                &mut pending.borrow_mut(),
                            )
                        },
                    )
                };
                let old_events = {
                    let pending = std::cell::RefCell::new(&mut old_pending);
                    let v = std::cell::RefCell::new(&mut old_v);
                    crowd_tick(
                        n,
                        tick,
                        |e, from| take_old(e, from, &mut pending.borrow_mut()),
                        |i| {
                            apply_old(
                                &mut v.borrow_mut()[i as usize],
                                i,
                                &mut pending.borrow_mut(),
                            )
                        },
                    )
                };
                assert_eq!(format!("{new_events:?}"), format!("{old_events:?}"));
                for i in 0..n as usize {
                    let (a, b) = (new_mobs[i].velocity, old_v[i]);
                    assert_eq!(
                        (a.x.to_bits(), a.y.to_bits(), a.z.to_bits()),
                        (b.x.to_bits(), b.y.to_bits(), b.z.to_bits()),
                        "{n} mobs, tick {tick}, mob {i}"
                    );
                }
            }
        }
    }

    /// One tick of a pen of `n` mobs pushing each other.
    #[test]
    #[ignore]
    fn push_cost() {
        for n in [10u32, 30, 100, 200] {
            let mut ms = mobs(n);
            let mut v = vec![Vec3::ZERO; n as usize];
            let t = std::time::Instant::now();
            {
                let pending = std::cell::RefCell::new(Vec::new());
                let v = std::cell::RefCell::new(&mut v);
                crowd_tick(
                    n,
                    0,
                    |e, from| take_old(e, from, &mut pending.borrow_mut()),
                    |i| {
                        apply_old(
                            &mut v.borrow_mut()[i as usize],
                            i,
                            &mut pending.borrow_mut(),
                        )
                    },
                );
            }
            let before = t.elapsed().as_secs_f64() * 1e3;
            let t = std::time::Instant::now();
            {
                let pending = std::cell::RefCell::new(PendingPushes::default());
                let ms = std::cell::RefCell::new(&mut ms);
                crowd_tick(
                    n,
                    0,
                    |e, from| take_mob_pushes(e, from, &mut pending.borrow_mut()),
                    |i| {
                        apply_mob_pushes(
                            &mut ms.borrow_mut()[i as usize],
                            &mut pending.borrow_mut(),
                        )
                    },
                );
            }
            let after = t.elapsed().as_secs_f64() * 1e3;
            println!("{n} mobs pushing each other: {before:.3} ms -> {after:.3} ms");
        }
    }
}

#[cfg(test)]
mod held_stack_tests {
    use super::*;

    #[test]
    fn replacing_even_the_same_item_clears_its_old_components() {
        let mut extra = Extra::default();
        let mut named = crate::items::ItemStack::new("minecraft:diamond_sword", 1);
        named.data_mut().custom_name = Some(horizon_core::nbt::Tag::String("Old Sword".into()));
        extra.set_main_hand_stack(Some(named.clone()));
        assert_eq!(
            extra.main_hand_stack().unwrap().custom_name().as_deref(),
            Some("Old Sword")
        );
        extra.set_main_hand(Some(named.item.clone()));
        assert_eq!(extra.main_hand_stack().unwrap().custom_name(), None);
        extra.set_main_hand_stack(Some(named));
        extra.take_slot_id(horizon_items::equipment::MAINHAND);
        assert!(extra.main_hand_id().is_none());
        assert!(extra.main_hand_stack().is_none());
    }
}

#[cfg(test)]
mod fire_resistance_damage_tests {
    use super::*;


    #[test]
    fn fire_resistance_rejects_fire_before_health_and_hurt_cooldown() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        for kind in [
            DamageKind::Campfire,
            DamageKind::Fireball,
            DamageKind::HotFloor,
            DamageKind::InFire,
            DamageKind::Lava,
            DamageKind::OnFire,
            DamageKind::SulfurCubeHot,
        ] {
            let mut mob = pig();
            mob.add_effect("minecraft:fire_resistance", -1, 0);
            assert!(!mob.damage(level, 1.0, Damage::of(kind)), "{kind:?}");
            assert_eq!(mob.health, 10.0, "{kind:?}");
            assert_eq!(
                (mob.hurt_time, mob.invulnerable, mob.last_hurt),
                (0, 0, 0.0)
            );
            assert!(mob.last_damage.is_none());
            assert!(mob.outbox.is_empty());
            // A refused fire hit must not prevent the next ordinary hit.
            assert!(mob.damage(level, 1.0, Damage::of(DamageKind::Cactus)));
            assert_eq!(mob.health, 9.0);

            let mut bare = pig();
            assert!(bare.damage(level, 1.0, Damage::of(kind)));
            assert_eq!(bare.health, 9.0);
            assert!(
                !bare.damage(level, 1.0, Damage::of(kind)),
                "equal hit during cooldown"
            );
            assert!(
                bare.damage(level, 2.0, Damage::of(kind)),
                "stronger hit during cooldown"
            );
            assert_eq!(bare.health, 8.0);

            let mut resistant = pig();
            resistant.add_effect("minecraft:resistance", -1, 0);
            assert!(resistant.damage(level, 1.0, Damage::of(kind)));
            assert_eq!(resistant.health, 9.2);

            let mut invulnerable = pig();
            invulnerable.invulnerable_flag = true;
            assert!(!invulnerable.damage(level, 1.0, Damage::of(kind)));
            assert_eq!(invulnerable.health, 10.0);
        }
    }

    #[test]
    fn species_hurt_callbacks_still_run_before_the_living_fire_guard() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let mut mob = pig();
        let kind = Arc::make_mut(&mut mob.kind);
        kind.id = "minecraft:bat".into();
        kind.species = Species::Bat;
        bat::set_resting(&mut mob, true);
        mob.add_effect("minecraft:fire_resistance", -1, 0);
        assert!(!mob.damage(level, 1.0, Damage::of(DamageKind::InFire)));
        assert!(!bat::resting(&mob));
        assert_eq!(mob.health, 10.0);
        assert_eq!(mob.invulnerable, 0);
    }

    #[test]
    fn reflected_ghast_fireball_still_reports_success_when_fire_resistance_refuses_health_damage() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let mut mob = pig();
        let kind = Arc::make_mut(&mut mob.kind);
        kind.id = "minecraft:ghast".into();
        kind.species = Species::Ghast;
        kind.flags.fire_immune = true;
        mob.add_effect("minecraft:fire_resistance", -1, 0);
        let source = Damage {
            kind: DamageKind::Fireball,
            attacker: Some(Who::Player),
            from: None,
            attacker_species: None,
        };
        assert!(mob.damage(level, 1.0, source));
        assert_eq!(mob.health, 10.0);
        assert_eq!(mob.invulnerable, 0);
    }
}

#[cfg(test)]
mod magic_absorb_tests {
    use super::sensor_scan_tests::kind;
    use super::*;
    use horizon_core::geometry::Vec3;


    /// `MobEffectInstance.update` for the flags of an effect a mob has: a new instance that is
    /// not ambient takes the ambient flag off an ambient one, one that is ambient does not unless
    /// the effect itself changes, and visibility is always the new instance's.
    #[test]
    fn an_effect_added_again_updates_its_ambient_and_visible_flags_as_vanilla() {
        let mut m = witch();
        let speed = "minecraft:speed";
        // A new effect takes its flags as they are.
        assert!(m.add_effect_with("speed", 600, 0, true, false));
        assert!(
            m.extra.ambient_effects.contains(&speed) && m.extra.hidden_effects.contains(&speed)
        );
        // The same effect, no stronger and no longer, ambient again and visible: ambient stays
        // (nothing changed), the visibility follows.
        assert!(m.add_effect_with("speed", 100, 0, true, true));
        assert_eq!(
            m.extra.effects,
            vec![crate::mobs::StoredEffect::new(speed, 600, 0)]
        );
        assert!(
            m.extra.ambient_effects.contains(&speed) && !m.extra.hidden_effects.contains(&speed)
        );
        // Not ambient: it takes the flag off, though nothing else changed.
        assert!(m.add_effect_with("speed", 100, 0, false, true));
        assert!(!m.extra.ambient_effects.contains(&speed));
        // Ambient over a not ambient one that does not change: not taken; one that changes: taken.
        assert!(!m.add_effect_with("speed", 100, 0, true, true));
        assert!(!m.extra.ambient_effects.contains(&speed));
        assert!(m.add_effect_with("speed", 900, 0, true, true));
        assert!(m.extra.ambient_effects.contains(&speed));
        assert_eq!(
            m.extra.effects,
            vec![crate::mobs::StoredEffect::new(speed, 900, 0)]
        );
    }


    #[test]
    fn absorption_refills_on_equal_and_weaker_adds() {
        let mut m = witch();
        m.add_effect_with("absorption", 200, 1, false, true);
        m.absorption = 0.0;
        assert!(!m.add_effect_with("absorption", 100, 1, false, true));
        assert_eq!(m.absorption, 8.0);
        m.absorption = 0.0;
        m.add_effect("minecraft:absorption", 50, 0);
        assert_eq!(m.absorption, 4.0);
        assert_eq!(m.extra.effects[0].amplifier, 1);
        assert_eq!(m.extra.effects[0].duration, 200);
    }

    #[test]
    fn absorption_downgrade_keeps_spent_hearts() {
        // LivingEntity.onEffectUpdated -> refreshDirtyAttributes clamps
        // absorption to the new MAX_ABSORPTION (4 for amplifier 0); it never raises it.
        let mut m = witch();
        m.add_effect_with("absorption", 200, 0, false, true);
        m.add_effect_with("absorption", 90, 1, false, true);
        assert_eq!(m.absorption, 8.0);
        m.absorption = 2.0;
        for _ in 0..90 {
            ai::tick_effects(
                &mut m,
                crate::test_resources::server_loop_context(),
                &mut crate::health::TestLevel::new(0).level(),
                None,
            );
        }
        assert_eq!(
            m.extra.effects,
            vec![crate::mobs::StoredEffect::new(
                "minecraft:absorption",
                110,
                0
            )]
        );
        assert_eq!(m.absorption, 2.0);
    }

    #[test]
    fn heal_without_a_mod_adds_the_amount_up_to_the_maximum() {
        let context = crate::test_resources::server_loop_context();
        let mut m = witch();
        m.health = m.attrs.max_health - 3.0;
        m.heal(context, 2.0);
        assert_eq!(m.health, m.attrs.max_health - 1.0);
        m.heal(context, 5.0);
        assert_eq!(m.health, m.attrs.max_health);
    }

    #[test]
    fn an_effect_counted_down_leaves_when_no_mod_holds_it() {
        let mut m = witch();
        m.add_effect_with("speed", 3, 0, false, true);
        for _ in 0..4 {
            ai::tick_effects(
                &mut m,
                crate::test_resources::server_loop_context(),
                &mut crate::health::TestLevel::new(0).level(),
                None,
            );
        }
        assert!(m.extra.effects.is_empty(), "{:?}", m.extra.effects);
    }

    #[test]
    fn stronger_effect_restores_its_hidden_predecessor_after_expiry() {
        let mut m = witch();
        m.add_effect_with("speed", 200, 0, false, true);
        m.add_effect_with("speed", 90, 1, false, true);
        for _ in 0..90 {
            ai::tick_effects(
                &mut m,
                crate::test_resources::server_loop_context(),
                &mut crate::health::TestLevel::new(0).level(),
                None,
            );
        }
        assert_eq!(
            m.extra.effects,
            vec![crate::mobs::StoredEffect::new("minecraft:speed", 110, 0)]
        );
    }

    #[test]
    fn resolved_instances_preserve_mob_identity_through_hidden_promotion() {
        let mut m = witch();
        let def = horizon_client_core::potions::effect("speed").unwrap();
        let effect = "minecraft:speed";
        assert!(m.add_effect_instance(effect, crate::effects::EffectInstance::from_def(def, 2, 1)));
        assert!(
            !m.add_effect_instance(effect, crate::effects::EffectInstance::from_def(def, 5, 0))
        );
        assert_eq!(m.extra.effect_chains[0].id, effect);
        for _ in 0..2 {
            ai::tick_effects(
                &mut m,
                crate::test_resources::server_loop_context(),
                &mut crate::health::TestLevel::new(0).level(),
                None,
            );
        }
        let restored = &m.extra.effects[0];
        assert_eq!(restored.id, "minecraft:speed");
        assert_eq!((restored.duration, restored.amplifier), (3, 0));
        assert_eq!(restored.registry_index, Some(0));
        assert!(m.extra.effect_chains.is_empty());
    }

    #[test]
    fn weaker_longer_effect_returns_with_its_flags_and_removal_clears_the_chain() {
        let mut m = witch();
        assert!(m.add_effect_with("speed", 2, 1, false, false));
        assert!(!m.add_effect_with("speed", 5, 0, true, false));
        for _ in 0..2 {
            ai::tick_effects(
                &mut m,
                crate::test_resources::server_loop_context(),
                &mut crate::health::TestLevel::new(0).level(),
                None,
            );
        }
        assert_eq!(
            m.extra.effects,
            vec![crate::mobs::StoredEffect {
                show_icon: false,
                ..crate::mobs::StoredEffect::new("minecraft:speed", 3, 0)
            }]
        );
        assert!(m.extra.ambient_effects.contains(&"minecraft:speed"));
        assert!(m.extra.hidden_effects.contains(&"minecraft:speed"));
        m.add_effect("speed", 1, 2);
        assert!(m.retain_effects(|_| false));
        assert!(m.extra.effect_chains.is_empty());
        assert!(m.extra.ambient_effects.is_empty());
        assert!(m.extra.hidden_effects.is_empty());
    }


    /// `Witch.getDamageAfterMagicAbsorb`: `#witch_resistant_to` (`magic`,
    /// `indirect_magic`, `sonic_boom`, `thorns`) takes 15% of the hit, which
    /// leaves a 10 health witch on 7 of a 20 hit it would not survive whole.
    #[test]
    fn a_witch_takes_15_percent_of_a_witch_resistant_hit() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        for resistant in [
            DamageKind::Magic,
            DamageKind::IndirectMagic,
            DamageKind::SonicBoom,
            DamageKind::Thorns,
        ] {
            let mut m = witch();
            assert!(m.damage(level, 20.0, Damage::of(resistant)));
            assert_eq!(m.health, 7.0, "{resistant:?} is in #witch_resistant_to");
        }
    }

    /// A hit outside the tag (`mob_attack`) takes all of it.
    #[test]
    fn a_witch_takes_all_of_a_physical_hit() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let mut m = witch();
        m.damage(level, 4.0, Damage::of(DamageKind::MobAttack));
        assert_eq!(m.health, 6.0);
    }

    /// `Witch.getDamageAfterMagicAbsorb`: a source whose entity is the witch
    /// itself takes nothing, so its own harming potion cannot hurt it.
    #[test]
    fn a_witch_takes_nothing_from_its_own_hit() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let mut m = witch();
        let own = Damage {
            kind: DamageKind::IndirectMagic,
            attacker: Some(Who::Mob(m.id)),
            from: None,
            attacker_species: Some(Species::Witch),
        };
        m.damage(level, 20.0, own);
        assert_eq!(m.health, 10.0);
    }


    /// A harming potion's hit on a witch through `damage_by` is cut to 15%
    /// once (`getDamageAfterMagicAbsorb`), not again by the potion code.
    #[test]
    fn a_witch_takes_15_percent_of_a_potion_hit() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let mut m = witch();
        let thrown = Damage::indirect_magic(Some(Who::Player), [0.0, 1.0, 0.0]);
        assert!(m.damage_by(level, 20.0, thrown, "minecraft:splash_potion"));
        assert_eq!(m.health, 7.0);
    }
}


#[cfg(test)]
mod allay_damage_identity_tests {
    use super::*;

    const ACTUAL: [i32; 4] = [101, 102, 103, 104];
    const OTHER: [i32; 4] = [10, 20, 30, 40];


    #[test]
    fn verified_player_damage_compares_live_liked_uuid() {
        let mut test_level = crate::health::TestLevel::new(0);
        let attack = crate::health::Attack {
            attacker: 7,
            player_uuid: Some(ACTUAL),
            ..Default::default()
        };
        let mut level = test_level.level().attacked_by(&attack);
        for liked in [Some(ACTUAL), Some(OTHER), None] {
            let mut m = allay(liked);
            let accepted = m.damage(&mut level, 2.0, Damage::player([0.0; 3]));
            assert_eq!(accepted, liked != Some(ACTUAL), "liked={liked:?}");
            assert_eq!(m.health, if accepted { 18.0 } else { 20.0 });
            assert_eq!(m.hurt_time, if accepted { 10 } else { 0 });
            assert_eq!(m.extra.owner, liked);
        }
        // A UUID context alone must not turn a source without a player into its liked player.
        let mut m = allay(Some(ACTUAL));
        assert!(m.damage(&mut level, 2.0, Damage::of(DamageKind::Generic)));
        assert_eq!(m.health, 18.0);
    }

    #[test]
    fn unverified_player_identity_never_matches_the_liked_player() {
        let mut test_level = crate::health::TestLevel::new(0);
        let mut level = test_level.level();
        for liked in [Some(ACTUAL), Some(OTHER), None] {
            let mut m = allay(liked);
            assert!(m.damage(&mut level, 2.0, Damage::player([0.0; 3])));
            assert_eq!(m.health, 18.0);
            // A projectile without a verified player cannot match a liked UUID either.
            let mut m = allay(liked);
            let arrow = Damage {
                kind: DamageKind::Arrow,
                ..Damage::player([0.0; 3])
            };
            assert!(m.damage_projectile(&mut level, 2.0, arrow, None, [1.0, 0.0]));
            assert_eq!(m.health, 18.0);
            assert!(m.projectile_motion.is_none());
        }
        let mut m = allay(Some(ACTUAL));
        assert!(m.damage(&mut level, 2.0, Damage::of(DamageKind::Generic)));
    }
}

#[cfg(test)]
mod totem_tests {
    use super::*;


    #[test]
    fn death_component_instant_healing_obeys_inversion_cap_and_dead_state() {
        let mut level = crate::health::TestLevel::new(0);
        for (undead, id) in [
            (false, "minecraft:instant_health"),
            (true, "minecraft:instant_damage"),
        ] {
            let mut mob = fox();
            let mut kind = (*mob.kind).clone();
            kind.flags.undead = undead;
            mob.kind = Arc::new(kind);
            mob.health = 1.0;
            let effect = crate::items::ConsumeEffect::ApplyEffects {
                probability: 1.0,
                effects: vec![crate::items::ConsumeEffectSpec {
                    id: id.into(),
                    duration: 1,
                    amplifier: 1,
                    ambient: false,
                    show_particles: true,
                    show_icon: true,
                }],
            };
            mob.apply_consume_effect(&mut level.level(), &effect);
            assert_eq!(mob.health, 9.0);
            mob.apply_consume_effect(&mut level.level(), &effect);
            assert_eq!(mob.health, 10.0);
            mob.health = 0.0;
            mob.apply_consume_effect(&mut level.level(), &effect);
            assert_eq!(
                mob.health, 0.0,
                "instant healing does not revive a dead mob"
            );
        }
    }

    /// `LivingEntity.checkTotemDeathProtection`: a totem in the mouth takes
    /// the killing blow, the fox stays at 1 health with the totem's effects,
    /// and entity event 35 tells the clients to draw it.
    #[test]
    fn a_totem_in_the_mouth_saves_a_fox() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let mut m = fox();
        m.extra
            .set_main_hand(Some("minecraft:totem_of_undying".into()));
        assert!(m.damage(level, 1000.0, Damage::of(DamageKind::Generic)));
        assert!(m.alive(), "the totem kept it alive");
        assert_eq!(m.health, 1.0);
        assert_eq!(m.extra.main_hand_id(), None, "the totem was used up");
        // DeathProtection.TOTEM_EFFECTS.
        assert_eq!(
            m.extra.effects,
            vec![
                crate::mobs::StoredEffect::new("minecraft:regeneration", 900, 1),
                crate::mobs::StoredEffect::new("minecraft:absorption", 100, 1),
                crate::mobs::StoredEffect::new("minecraft:fire_resistance", 800, 0),
            ]
        );
        assert!(
            m.outbox
                .iter()
                .any(|e| matches!(e, MobEvent::EntityEvent { event: 35, .. }))
        );
    }

    /// `getItemInHand` asks the main hand first, then the off hand.
    #[test]
    fn a_totem_in_the_off_hand_saves_a_fox_too() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let mut m = fox();
        m.extra.set_slot_id(
            horizon_items::equipment::OFFHAND,
            Some("minecraft:totem_of_undying".into()),
        );
        assert!(m.damage(level, 1000.0, Damage::of(DamageKind::Generic)));
        assert!(m.alive());
        assert_eq!(m.health, 1.0);
        assert_eq!(m.extra.off_hand_id(), None);
    }

    /// Without one in either hand the blow kills it as before.
    #[test]
    fn without_a_totem_the_blow_kills() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let mut m = fox();
        m.extra
            .set_main_hand(Some("minecraft:sweet_berries".into()));
        assert!(m.damage(level, 1000.0, Damage::of(DamageKind::Generic)));
        assert!(!m.alive());
    }

    /// `DamageTypeTags.BYPASSES_INVULNERABILITY` (`/kill`, the void) is not
    /// something a totem answers.
    #[test]
    fn a_bypassing_killing_blow_ignores_the_totem() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let mut m = fox();
        m.extra
            .set_main_hand(Some("minecraft:totem_of_undying".into()));
        assert!(m.damage(level, 1000.0, Damage::of(DamageKind::GenericKill)));
        assert!(!m.alive());
        assert_eq!(
            m.extra.main_hand_id(),
            Some("minecraft:totem_of_undying"),
            "the totem is still in the mouth"
        );
    }
}
#[cfg(test)]
mod effect_refusal_tests {
    use super::*;

    /// `LivingEntity.addEffect` asks `canBeAffected` before it stores anything: an
    /// effect the mob's type refuses never joins the list, so the cloud on its head,
    /// the `/effect` reply and an effect tick never see it. The refused ones are the
    /// `ignores_poison_and_regen` tag (which is `#minecraft:undead`), a silverfish's
    /// infested and a slime's oozing, the classes that override the rule, and the
    /// wither, whose `addEffect` refuses every effect.
    #[test]
    fn an_effect_the_mobs_type_refuses_is_never_added() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/horizon-client")
            .join(horizon_core::VANILLA_VERSION);
        if !root.join("entity-types.json").exists() {
            return;
        }
        let types =
            MobTypes::load_for_server(&root, &MobTypes::root_catalog(&root).unwrap()).unwrap();
        let refused = |id: &str, effect: &'static str| {
            let mut m = mob(&types, id);
            m.add_effect(effect, 200, 0);
            let listed = m.has_effect(effect);
            (listed, m.extra.effects.len())
        };
        // `#minecraft:undead` is the `ignores_poison_and_regen` tag: a zombie, a
        // skeleton and a phantom turn both poison and regeneration away.
        for undead in [
            "minecraft:zombie",
            "minecraft:skeleton",
            "minecraft:phantom",
            "minecraft:zombie_horse",
        ] {
            for effect in ["minecraft:poison", "minecraft:regeneration"] {
                assert_eq!(refused(undead, effect), (false, 0), "{undead} {effect}");
            }
            // Every other effect still lands on it.
            assert_eq!(refused(undead, "minecraft:strength"), (true, 1), "{undead}");
        }
        // The tag's other two effects, each refused by the one type in it.
        assert_eq!(
            refused("minecraft:silverfish", "minecraft:infested"),
            (false, 0)
        );
        assert_eq!(
            refused("minecraft:silverfish", "minecraft:poison"),
            (true, 1)
        );
        assert_eq!(refused("minecraft:slime", "minecraft:oozing"), (false, 0));
        // A magma cube is not in `immune_to_oozing`, so oozing takes.
        assert_eq!(
            refused("minecraft:magma_cube", "minecraft:oozing"),
            (true, 1)
        );
        // Spider.canBeAffected and CaveSpider's inheritance: poison is refused, and a
        // nautilus refuses it too (AbstractNautilus.canBeAffected), the zombie one
        // through the same override, an ordinary mob not at all.
        for spider in [
            "minecraft:spider",
            "minecraft:cave_spider",
            "minecraft:nautilus",
        ] {
            assert_eq!(refused(spider, "minecraft:poison"), (false, 0), "{spider}");
        }
        assert_eq!(refused("minecraft:cow", "minecraft:poison"), (true, 1));
        // WitherBoss.addEffect returns false: the boss takes no effect at all.
        for effect in [
            "minecraft:wither",
            "minecraft:strength",
            "minecraft:speed",
            "minecraft:poison",
        ] {
            assert_eq!(
                refused("minecraft:wither", effect),
                (false, 0),
                "wither {effect}"
            );
        }
        // WitherSkeleton.canBeAffected refuses the wither effect only, which a
        // zombie takes.
        assert_eq!(
            refused("minecraft:wither_skeleton", "minecraft:wither"),
            (false, 0)
        );
        assert_eq!(
            refused("minecraft:wither_skeleton", "minecraft:strength"),
            (true, 1)
        );
        assert_eq!(refused("minecraft:zombie", "minecraft:wither"), (true, 1));
        // Parched.canBeAffected: weakness only, slowness takes.
        assert_eq!(
            refused("minecraft:parched", "minecraft:weakness"),
            (false, 0)
        );
        assert_eq!(
            refused("minecraft:parched", "minecraft:slowness"),
            (true, 1)
        );
    }

    fn check_squid_damage_result(id: &str) {
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let types =
            MobTypes::load_for_server(&root, &MobTypes::root_catalog(&root).unwrap()).unwrap();
        let mut attacker = mob(&types, "minecraft:cow");
        attacker.id = 2;
        attacker.position = Vec3::new(4.0, 64.0, 4.0);
        assert!(attacker.alive());
        let by_cow = Damage {
            kind: DamageKind::Generic,
            ..Damage::mob(attacker.id, [4.0, 64.0, 4.0]).with_species(Species::Cow)
        };
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        // An unrelated parent's generic acceptance remains true.
        let mut cow = mob(&types, "minecraft:cow");
        assert!(cow.damage(level, 1.0, Damage::of(DamageKind::Generic)));
        assert_eq!(cow.health, 9.0);
        // A parent's existing rejection never reaches the subclass aftermath.
        let mut protected = mob(&types, id);
        protected.invulnerable_flag = true;
        assert!(!protected.damage(level, 1.0, by_cow));
        assert_eq!(protected.health, 10.0);
        if protected.species() == Species::GlowSquid {
            assert_eq!(aquatic::dark_ticks(&protected), 0);
        }
        for no_ai in [false, true] {
            for attributed in [true, false] {
                let mut victim = mob(&types, id);
                victim.no_ai = no_ai;
                assert_eq!(victim.health, 10.0);
                assert!(victim.ai.last_hurt_by.is_none());
                let source = if attributed {
                    by_cow
                } else {
                    Damage::of(DamageKind::Generic)
                };
                let accepted = victim.damage(level, 1.0, source);
                // Check the parent's state first: false must not roll back damage.
                assert_eq!(victim.health, 9.0);
                assert_eq!(
                    victim.ai.last_hurt_by,
                    attributed.then_some(Who::Mob(attacker.id))
                );
                if victim.species() == Species::GlowSquid {
                    assert_eq!(
                        aquatic::dark_ticks(&victim),
                        if attributed { 100 } else { 0 }
                    );
                }
                assert_eq!(accepted, attributed, "{id} NoAI={no_ai}");
            }
        }
    }

    #[test]
    #[ignore = "daily"]
    fn glow_squid_damage_result_keeps_parent_health_side_effect() {
        check_squid_damage_result("minecraft:glow_squid");
    }

    #[test]
    #[ignore = "daily"]
    fn squid_damage_result_keeps_parent_health_side_effect() {
        check_squid_damage_result("minecraft:squid");
    }

    /// Squid.hurtServer reports a hit only when a living entity dealt it, so
    /// environmental damage leaves a glow squid glowing; a mob's blow dims it.
    #[test]
    fn only_a_mobs_blow_dims_a_glow_squid() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/horizon-client")
            .join(horizon_core::VANILLA_VERSION);
        if !root.join("entity-types.json").exists() {
            return;
        }
        let types =
            MobTypes::load_for_server(&root, &MobTypes::root_catalog(&root).unwrap()).unwrap();
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let mut burned = mob(&types, "minecraft:glow_squid");
        burned.health = 10.0;
        assert!(!burned.damage(level, 1.0, Damage::of(DamageKind::Campfire)));
        assert_eq!(burned.health, 9.0);
        assert_eq!(aquatic::dark_ticks(&burned), 0);
        let mut struck = mob(&types, "minecraft:glow_squid");
        struck.health = 10.0;
        assert!(struck.damage(level, 1.0, Damage::mob(2, [0.0, 64.0, 0.0])));
        assert_eq!(struck.health, 9.0);
        assert_eq!(aquatic::dark_ticks(&struck), 100);
    }
}
#[cfg(test)]
mod model_document_sharing_tests {
    use super::*;

    fn part_dump(part: &Part) -> String {
        format!("{part:?}")
    }

    /// The startup reads `entity-models.json` once and hands the same document
    /// to every model loader; reading it again per loader must build the same
    /// types, model for model and layer for layer.
    #[test]
    fn a_shared_document_builds_the_same_types() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/horizon-client")
            .join(horizon_core::VANILLA_VERSION);
        if !root.join("entity-models.json").exists() {
            return;
        }
        let document = EntityModelDocument::load(&root).unwrap();
        let shared = MobTypes::load_with_document(&root, &document).unwrap();
        let independent = MobTypes::load(&root).unwrap();
        assert_eq!(shared.types.len(), independent.types.len());
        for (id, independent) in &independent.types {
            let shared = shared.types.get(id).unwrap_or_else(|| panic!("{id}"));
            assert_eq!(independent.id, shared.id);
            assert_eq!(independent.species, shared.species);
            assert_eq!(
                (
                    independent.width,
                    independent.height,
                    independent.eye_height,
                ),
                (shared.width, shared.height, shared.eye_height)
            );
            assert_eq!(independent.attributes, shared.attributes);
            assert_eq!(independent.item_stack_limits, shared.item_stack_limits);
            assert_eq!(
                part_dump(&independent.model),
                part_dump(&shared.model),
                "{id} model"
            );
            let mut independent_layers: Vec<_> = independent.layers.iter().collect();
            let mut shared_layers: Vec<_> = shared.layers.iter().collect();
            independent_layers.sort_by_key(|(key, _)| key.as_str());
            shared_layers.sort_by_key(|(key, _)| key.as_str());
            assert_eq!(independent_layers.len(), shared_layers.len(), "{id} layers");
            for ((independent_key, independent_part), (shared_key, shared_part)) in
                independent_layers.into_iter().zip(shared_layers)
            {
                assert_eq!(independent_key, shared_key, "{id} layer key");
                assert_eq!(
                    part_dump(independent_part),
                    part_dump(shared_part),
                    "{id} layer {independent_key}"
                );
            }
        }
    }
}

#[cfg(test)]
mod named_damage_tests {
    use super::*;

    fn fire(tags: u32) -> Damage {
        Damage::of(DamageKind::Named(NamedDamage::new(
            "example:fan_fire",
            NamedDamage::IS_FIRE | NamedDamage::BYPASSES_ARMOR | tags,
        )))
    }
    #[test]
    fn named_fire_uses_normal_immunity_and_protection_without_becoming_on_fire() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let mut resistant = pig();
        resistant.add_effect("minecraft:fire_resistance", -1, 0);
        assert!(!resistant.damage(level, 2.0, fire(0)));
        assert_eq!((resistant.health, resistant.invulnerable), (10.0, 0));
        let mut immune = pig();
        Arc::make_mut(&mut immune.kind).flags.fire_immune = true;
        assert!(!immune.damage(level, 2.0, fire(0)));
        let mut bare = pig();
        assert!(bare.damage(level, 2.0, fire(0)));
        assert_eq!(bare.health, 8.0); // bypasses the 20 armor points
        assert!(bare.velocity.x != 0.0 || bare.velocity.z != 0.0);
        assert_eq!(bare.last_damage.unwrap().0.kind, fire(0).kind);
        let mut protected = pig();
        gear::set(
            &mut protected,
            gear::CHEST,
            vec![("minecraft:fire_protection".into(), 4)],
        );
        assert!(protected.damage(level, 2.0, fire(0)));
        assert!((protected.health - 8.64).abs() < 0.00001);
        let mut still = pig();
        assert!(still.damage(level, 2.0, fire(NamedDamage::NO_KNOCKBACK)));
        assert_eq!(still.velocity, Vec3::ZERO);
    }
    #[test]
    fn named_tags_control_resistance_and_specialist_enchantments() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let mut mob = pig();
        mob.add_effect("minecraft:resistance", -1, 0);
        assert!(mob.damage(level, 2.0, fire(0)));
        assert!((mob.health - 8.4).abs() < 0.00001);
        let mut bypass = pig();
        bypass.add_effect("minecraft:resistance", -1, 0);
        assert!(bypass.damage(level, 2.0, fire(NamedDamage::BYPASSES_RESISTANCE)));
        assert_eq!(bypass.health, 8.0);
        let mut no_effects = pig();
        no_effects.add_effect("minecraft:resistance", -1, 0);
        gear::set(
            &mut no_effects,
            gear::CHEST,
            vec![("minecraft:fire_protection".into(), 4)],
        );
        assert!(no_effects.damage(level, 2.0, fire(NamedDamage::BYPASSES_EFFECTS)));
        assert_eq!(no_effects.health, 8.0);
        // #bypasses_enchantments (#11308): a warden's sonic boom, or a named
        // type with the tag, is not reduced by Protection, as
        // getDamageAfterMagicAbsorb returns it before the enchantments.
        for kind in [
            DamageKind::SonicBoom,
            DamageKind::Named(NamedDamage::new(
                "example:boom",
                NamedDamage::BYPASSES_ARMOR | NamedDamage::BYPASSES_ENCHANTMENTS,
            )),
        ] {
            let mut plated = pig();
            gear::set(
                &mut plated,
                gear::CHEST,
                vec![("minecraft:protection".into(), 4)],
            );
            assert!(plated.damage(level, 4.0, Damage::of(kind)));
            assert_eq!(plated.health, 6.0, "{kind:?}");
        }
        let mut armor = gear::Gear::default();
        armor.slots[gear::FEET] = vec![("minecraft:feather_falling".into(), 4)];
        armor.slots[gear::CHEST] = vec![("minecraft:blast_protection".into(), 4)];
        let kind = |tags| DamageKind::Named(NamedDamage::new("example:impact", tags));
        assert_eq!(gear::protection_points(&armor, kind(0)), 0.0);
        assert_eq!(
            gear::protection_points(&armor, kind(NamedDamage::IS_FALL)),
            12.0
        );
        assert_eq!(
            gear::protection_points(&armor, kind(NamedDamage::IS_EXPLOSION)),
            8.0
        );
    }
}

#[cfg(test)]
mod silent_voice_tests {
    use super::*;
    use crate::mobs::world::with_test_world;


    /// `Mob.playAmbientSound` plays through `Entity.playSound`, which returns
    /// early for a silent entity, so a mob summoned with `Silent:1b` makes no
    /// ambient sound. The pitch is still drawn first, as Java evaluates the
    /// arguments of `playSound(event, getSoundVolume(), getVoicePitch())`
    /// before the call, so the random state is the same either way. Without
    /// the check the silent cow plays `entity.cow.ambient` like the loud one,
    /// and without drawing first its random state falls two draws behind.
    #[test]
    #[ignore = "daily"]
    fn a_silent_mob_makes_no_ambient_sound() {
        let Some((loud, loud_next)) = ambient(false) else {
            return;
        };
        let (quiet, quiet_next) = ambient(true).expect("same data");
        // Cow.getSoundVolume is 0.4 and the pitch is the voice pitch.
        assert_eq!(loud.len(), 1);
        assert_eq!(loud[0].0, "entity.cow.ambient");
        assert_eq!(loud[0].1, 0.4);
        assert!((0.8..=1.2).contains(&loud[0].2), "{}", loud[0].2);
        assert!(quiet.is_empty(), "{quiet:?}");
        assert_eq!(quiet_next, loud_next, "the pitch is drawn either way");
    }
}

#[cfg(test)]
mod visibility_percent_tests {
    use super::*;
    /// `LivingEntity.getVisibilityPercent` halves the range for the mob named
    /// on the worn head, and leaves every other mob at full range. A skull
    /// held in the hand does not count: its equippable slot is the head.
    #[test]
    fn worn_mob_heads_halve_that_mobs_detection_range() {
        let see = |head: &str, mob: Species| {
            let mut player = world::test_player(Vec3::ZERO);
            player.head = Some(Arc::from(head));
            player.visibility_percent(mob)
        };
        assert_eq!(see("minecraft:skeleton_skull", Species::Skeleton), 0.5);
        assert_eq!(see("minecraft:zombie_head", Species::Zombie), 0.5);
        assert_eq!(see("minecraft:creeper_head", Species::Creeper), 0.5);
        assert_eq!(see("minecraft:piglin_head", Species::Piglin), 0.5);
        assert_eq!(see("minecraft:piglin_head", Species::PiglinBrute), 0.5);
        assert_eq!(see("minecraft:skeleton_skull", Species::Zombie), 1.0);
        assert_eq!(see("minecraft:piglin_head", Species::ZombifiedPiglin), 1.0);
        assert_eq!(
            see("minecraft:wither_skeleton_skull", Species::WitherSkeleton),
            1.0
        );

        let mut held = world::test_player(Vec3::ZERO);
        held.main_hand = Some(Arc::from("minecraft:skeleton_skull"));
        assert_eq!(held.visibility_percent(Species::Skeleton), 1.0);

        let mut sneaking = world::test_player(Vec3::ZERO);
        sneaking.head = Some(Arc::from("minecraft:skeleton_skull"));
        sneaking.sneaking = true;
        assert_eq!(sneaking.visibility_percent(Species::Skeleton), 0.4);
    }
}

/// Added controls (H248): `LivingEntity.hurtServer` compares a hit inside
/// the hurt cooldown, and keeps `lastHurt`, by its incoming amount; the
/// reductions (`actuallyHurt`) get only the accepted difference.
#[cfg(test)]
mod hurt_cooldown_incoming_tests {
    use super::*;


    #[test]
    fn full_resistance_keeps_the_incoming_amounts_and_takes_a_stronger_hit() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let mut m = mob(Species::Pig);
        m.extra
            .effects
            .push(StoredEffect::new("minecraft:resistance", 600, 4));
        assert!(m.damage(level, 1.0, Damage::of(DamageKind::Generic)));
        assert_eq!((m.health, m.invulnerable, m.last_hurt), (40.0, 20, 1.0));
        assert!(!m.damage(level, 1.0, Damage::of(DamageKind::Generic)));
        assert!(m.damage(level, 2.0, Damage::of(DamageKind::Generic)));
        assert_eq!((m.health, m.invulnerable, m.last_hurt), (40.0, 20, 2.0));
        assert!(!m.damage(level, 1.5, Damage::of(DamageKind::Generic)));
        assert_eq!(m.last_hurt, 2.0);
    }

    #[test]
    fn the_armor_effectiveness_callback_runs_only_for_accepted_hits() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let mut m = mob(Species::Pig);
        // This damage-boundary fixture supplies effective armor directly;
        // equipment attributes are normally prepared by the equipment tick.
        Arc::make_mut(&mut m.kind).armor = 6.0;
        let mut calls = 0;
        let mut breach = |f: f32, _: &mut LegacyRandom| {
            calls += 1;
            f
        };
        assert!(m.damage_with(
            level,
            4.0,
            Damage::of(DamageKind::Cactus),
            Some(&mut breach)
        ));
        assert!(!m.damage_with(
            level,
            4.0,
            Damage::of(DamageKind::Cactus),
            Some(&mut breach)
        ));
        assert!(m.damage_with(
            level,
            8.0,
            Damage::of(DamageKind::Cactus),
            Some(&mut breach)
        ));
        assert_eq!(calls, 2);
        assert_eq!(m.last_hurt, 8.0);
    }

    #[test]
    fn absorption_soaks_the_accepted_difference() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let mut m = mob(Species::Pig);
        // Absorption I gives max_absorption 4, so the 1.5 remainder is valid.
        m.add_effect("absorption", 600, 0);
        m.absorption = 1.5;
        assert!(m.damage(level, 1.0, Damage::of(DamageKind::Generic)));
        assert_eq!((m.health, m.absorption), (40.0, 0.5));
        // 3 after 1: the difference 2, the last 0.5 absorption first.
        assert!(m.damage(level, 3.0, Damage::of(DamageKind::Generic)));
        assert_eq!((m.health, m.absorption, m.last_hurt), (38.5, 0.0, 3.0));
    }

    #[test]
    fn a_horse_armor_wears_by_the_accepted_difference() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let mut m = mob(Species::Horse);
        m.extra.set_body(Some("minecraft:iron_horse_armor".into()));
        assert!(m.damage(level, 4.0, Damage::of(DamageKind::Cactus)));
        assert!(m.damage(level, 8.0, Damage::of(DamageKind::Cactus)));
        let worn: Vec<f32> = m
            .outbox
            .iter()
            .filter_map(|e| match e {
                MobEvent::ArmorHurt { amount, .. } => Some(*amount),
                _ => None,
            })
            .collect();
        assert_eq!(worn, [4.0, 4.0]);
    }
}

#[cfg(test)]
mod hidden_speed_mob_tick_tests {
    use super::*;
    #[test]
    #[ignore = "daily"]
    fn noai_full_mob_tick_promotes_speed() {
        full_tick_promotes(true)
    }
    #[test]
    #[ignore = "daily"]
    fn ai_full_mob_tick_promotes_speed() {
        full_tick_promotes(false)
    }


}

mod projectile_damage;


pub mod control;

pub mod anim;
pub mod body_anim;

pub mod landing;
