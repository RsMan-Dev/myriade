//! `Mob.finalizeSpawn` and its overrides, for every spawn path (natural
//! spawning, chunk generation, spawners, trial spawners, spawn eggs,
//! commands, structures, patrols, raids, jockeys, conversions...).
//!
//! Every draw vanilla takes from `level.getRandom()` comes from the random
//! the caller passes (the level's random for a running level, the region's
//! for chunk generation); the few that come from the entity's own random
//! (`Zombie.handleAttributes`, `Piglin.createSpawnWeapon`, salmon sizes) come
//! from `Mob::random`. The order of the draws follows vanilla's class chain:
//! each species runs its own part before or after its superclass as vanilla
//! does. Jockeys a mob brings (`EntityType.create` inside finalizeSpawn) are
//! made through [`FinalizeLevel::create`], finalized with the same random and
//! returned in [`Ctx::created`].
//!
//! Variant state kept in [`super::Extra`]: `variant` is the registry or enum
//! index of the species' variant (cat, wolf, frog, farm animals by climate,
//! zombie nautilus, horse, llama, fox, rabbit, parrot, axolotl, salmon,
//! tropical fish pattern, panda main gene, villager type), `variant2` the
//! second part (horse markings, llama strength, panda hidden gene, tropical
//! fish base colour), `variant3` the tropical fish pattern colour, and
//! `sound_variant` the index in the species' sound variant registry.
use super::{
    MobWorld, Species, equip, farm::Climate, gear, spawn_rules::Reason,
    spawning::LocalDifficulty,
};
use crate::{enchant::Enchantments, items::Items};
use horizon_core::equipment_slot::EquipmentSlot;
use horizon_core::{geometry::Aabb, random::RandomSource};
use serde_json::Value;
use std::{fs, path::Path, sync::Arc};

/// What a level's finalizeSpawn event decided for a mob another mob's
/// finalizeSpawn created: the rider is dropped, added without its own rolls,
/// or finalized. Vanilla fires the event for every `finalizeSpawn` call,
/// riders included.
#[derive(Debug, PartialEq)]
pub enum SpawnEvent {
    Cancelled,
    Skip,
    Finalize,
}

/// The level as finalizeSpawn reads it.
pub trait FinalizeLevel {
    /// The owning world's immutable builtin generation (vanilla for bare fixtures).
    fn mob_maps(&self) -> &crate::neoforge_mob_maps::NeoForgeMobMaps {
        crate::neoforge_mob_maps::NeoForgeMobMaps::vanilla()
    }
    /// The biome id at a block.
    fn biome(&self, pos: [i32; 3]) -> String;
    /// The biome at a block is in a biome tag (`minecraft:...`).
    fn biome_tag(&self, tag: &str, pos: [i32; 3]) -> bool;
    /// The structures with a piece at a block.
    fn structures_with_piece_at(&self, pos: [i32; 3]) -> Vec<String>;
    /// `EnvironmentAttributes.MOON_PHASE` at a block (0 full moon to 7).
    fn moon_phase(&self, pos: [i32; 3]) -> usize;
    /// `ServerLevel.isSpawningMonsters`.
    fn spawning_monsters(&self) -> bool;
    /// `CollisionGetter.noCollision(AABB)`.
    fn no_collision(&self, bounds: Aabb) -> bool;
    /// The first chicken (in entity order) whose box meets `bounds` and
    /// that is not being ridden (`Zombie.finalizeSpawn`'s chicken jockeys).
    fn free_chicken(&self, bounds: Aabb) -> Option<u32>;
    /// A chicken found by `free_chicken` carries the zombie `rider`
    /// (`setChickenJockey(true)` and `startRiding`).
    fn ride_chicken(&mut self, chicken: u32, rider: u32);
    /// `getGameTime`.
    fn game_time(&self) -> i64;
    /// `SpecialDates.isHalloween`.
    fn halloween(&self) -> bool;
    /// What finalizeSpawn needs besides the level: the variant registries,
    /// enchantments and items the level owns.
    fn spawn_data(&self) -> SpawnData<'_>;
}

/// `AgeableMob.AgeableMobGroupData`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ageable {
    pub size: i32,
    pub should_spawn_baby: bool,
    pub baby_chance: f32,
}
impl Ageable {
    pub fn new(should_spawn_baby: bool, baby_chance: f32) -> Self {
        Self {
            size: 0,
            should_spawn_baby,
            baby_chance,
        }
    }
    /// `AgeableMobGroupData(boolean)`.
    pub fn babies(should_spawn_baby: bool) -> Self {
        Self::new(should_spawn_baby, 0.05)
    }
    /// `AgeableMobGroupData(float)`.
    pub fn chance(baby_chance: f32) -> Self {
        Self::new(true, baby_chance)
    }
}
/// `SpawnGroupData`: what the mobs of a pack share.
#[derive(Clone, Debug, PartialEq)]
pub enum Group {
    Ageable(Ageable),
    /// `Zombie.ZombieGroupData`, and `Husk.HuskGroupData` with `tried_camel`.
    Zombie {
        baby: bool,
        jockey: bool,
        husk: Option<bool>,
    },
    /// `Spider.SpiderEffectsGroupData`.
    Spider(Option<&'static str>),
    Horse(Ageable, u8),
    Llama(Ageable, u8),
    Fox(Ageable, u8),
    Rabbit(Ageable, u8),
    Axolotl(Ageable, [u8; 2]),
    Wolf(Ageable, u8),
    /// `AbstractSchoolingFish.SchoolSpawnGroupData` (the leader's id), and
    /// `TropicalFishGroupData` with its variant.
    School(u32, Option<(u8, u8, u8)>),
}
impl Group {
    fn ageable(&mut self) -> Option<&mut Ageable> {
        match self {
            Self::Ageable(a)
            | Self::Horse(a, _)
            | Self::Llama(a, _)
            | Self::Fox(a, _)
            | Self::Rabbit(a, _)
            | Self::Axolotl(a, _)
            | Self::Wolf(a, _) => Some(a),
            _ => None,
        }
    }
}

/// A variant registry entry's spawn conditions (`SpawnPrioritySelectors`).
#[derive(Clone, Debug)]
enum Condition {
    Biome(Vec<String>),
    Structure(Vec<String>),
    Moon(Option<f64>, Option<f64>),
}
#[derive(Clone, Debug)]
pub struct VariantRegistry {
    /// Entry names in registry order (sorted by id).
    pub names: Vec<String>,
    selectors: Vec<(usize, Option<Condition>, i32)>,
}
impl VariantRegistry {
    fn load(dir: &Path, structure_tags: &dyn Fn(&str) -> Vec<String>) -> Self {
        let mut names: Vec<String> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                e.file_name()
                    .to_str()
                    .and_then(|n| n.strip_suffix(".json"))
                    .map(str::to_owned)
            })
            .collect();
        names.sort();
        let mut selectors = Vec::new();
        for (i, name) in names.iter().enumerate() {
            let Ok(bytes) = fs::read(dir.join(format!("{name}.json"))) else {
                continue;
            };
            let Ok(v) = serde_json::from_slice::<Value>(&bytes) else {
                continue;
            };
            for s in v["spawn_conditions"].as_array().into_iter().flatten() {
                let priority = s["priority"].as_i64().unwrap_or(0) as i32;
                let c = &s["condition"];
                let list = |v: &Value| -> Vec<String> {
                    match v {
                        Value::String(s) => vec![s.clone()],
                        Value::Array(a) => a
                            .iter()
                            .filter_map(|x| x.as_str().map(str::to_owned))
                            .collect(),
                        _ => Vec::new(),
                    }
                };
                let condition = match c["type"].as_str() {
                    Some("minecraft:biome") => Some(Condition::Biome(list(&c["biomes"]))),
                    Some("minecraft:structure") => Some(Condition::Structure(
                        list(&c["structures"])
                            .into_iter()
                            .flat_map(|s| match s.strip_prefix('#') {
                                Some(tag) => structure_tags(tag),
                                None => vec![s],
                            })
                            .collect(),
                    )),
                    Some("minecraft:moon_brightness") => {
                        let r = &c["range"];
                        Some(match r.as_f64() {
                            Some(x) => Condition::Moon(Some(x), Some(x)),
                            None => Condition::Moon(r["min"].as_f64(), r["max"].as_f64()),
                        })
                    }
                    _ => None,
                };
                selectors.push((i, condition, priority));
            }
        }
        Self { names, selectors }
    }
    pub fn index(&self, name: &str) -> Option<usize> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        self.names.iter().position(|n| n == name)
    }
    /// `VariantUtils.selectVariantToSpawn`: the entries of the highest
    /// priority whose condition holds, one of them at random.
    pub fn select<L: FinalizeLevel + ?Sized>(
        &self,
        level: &L,
        pos: [i32; 3],
        random: &mut impl RandomSource,
    ) -> Option<usize> {
        let mut unpacked: Vec<&(usize, Option<Condition>, i32)> = self.selectors.iter().collect();
        unpacked.sort_by_key(|s| std::cmp::Reverse(s.2));
        let mut highest = i32::MIN;
        let mut selected = Vec::new();
        for (entry, condition, priority) in unpacked {
            if *priority < highest {
                continue;
            }
            let ok = match condition {
                None => true,
                Some(Condition::Biome(list)) => list.iter().any(|b| match b.strip_prefix('#') {
                    Some(tag) => level.biome_tag(tag, pos),
                    None => level.biome(pos) == *b,
                }),
                Some(Condition::Structure(list)) => {
                    let here = level.structures_with_piece_at(pos);
                    here.iter().any(|s| list.contains(s))
                }
                Some(Condition::Moon(min, max)) => {
                    let b = f64::from(super::spawning::MOON_BRIGHTNESS[level.moon_phase(pos) & 7]);
                    min.is_none_or(|m| b >= m) && max.is_none_or(|m| b <= m)
                }
            };
            if ok {
                highest = *priority;
                selected.push(*entry);
            }
        }
        if selected.is_empty() {
            return None;
        }
        Some(selected[random.next_int_bound(selected.len() as i32).unwrap_or(0) as usize])
    }
    /// `Registry.getRandom`: any entry.
    pub fn random(&self, random: &mut impl RandomSource) -> usize {
        if self.names.is_empty() {
            return 0;
        }
        random.next_int_bound(self.names.len() as i32).unwrap_or(0) as usize
    }
}

/// The registries finalizeSpawn reads.
pub struct SpawnRegistries {
    pub pig: VariantRegistry,
    pub cow: VariantRegistry,
    pub chicken: VariantRegistry,
    pub cat: VariantRegistry,
    pub wolf: VariantRegistry,
    pub frog: VariantRegistry,
    pub zombie_nautilus: VariantRegistry,
    pub pig_sound: VariantRegistry,
    pub cow_sound: VariantRegistry,
    pub chicken_sound: VariantRegistry,
    pub cat_sound: VariantRegistry,
    pub wolf_sound: VariantRegistry,
}
impl SpawnRegistries {
    /// Loads the variant registries from a resources directory
    /// (`data/minecraft/<registry>/`).
    pub fn load(root: &Path) -> Self {
        let data = root.join("data/minecraft");
        let structure_tag = |tag: &str| -> Vec<String> {
            let path = tag.strip_prefix("minecraft:").unwrap_or(tag);
            let file = data.join(format!("tags/worldgen/structure/{path}.json"));
            let Ok(bytes) = fs::read(file) else {
                return Vec::new();
            };
            serde_json::from_slice::<Value>(&bytes)
                .ok()
                .and_then(|v| v["values"].as_array().cloned())
                .into_iter()
                .flatten()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        };
        let r = |name: &str| VariantRegistry::load(&data.join(name), &structure_tag);
        Self {
            pig: r("pig_variant"),
            cow: r("cow_variant"),
            chicken: r("chicken_variant"),
            cat: r("cat_variant"),
            wolf: r("wolf_variant"),
            frog: r("frog_variant"),
            zombie_nautilus: r("zombie_nautilus_variant"),
            pig_sound: r("pig_sound_variant"),
            cow_sound: r("cow_sound_variant"),
            chicken_sound: r("chicken_sound_variant"),
            cat_sound: r("cat_sound_variant"),
            wolf_sound: r("wolf_sound_variant"),
        }
    }
}

/// What finalizeSpawn needs besides the level: registries, enchantments.
pub struct SpawnData<'a> {
    pub registries: &'a SpawnRegistries,
    pub enchantments: &'a Enchantments,
    pub items: &'a Items,
    /// `EntityTypes.CAMEL_HUSK` dimensions (width, height).
    pub camel_husk_size: (f32, f32),
}

/// A finalizeSpawn call and the entities it made.
pub struct Ctx<'a, L: FinalizeLevel + ?Sized, R: RandomSource> {
    pub level: &'a mut L,
    pub random: &'a mut R,
    pub difficulty: LocalDifficulty,}

// ---- attributes -----------------------------------------------------------

/// The attributes finalizeSpawn changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attr {
    MaxHealth,
    MovementSpeed,
    FollowRange,
    AttackDamage,
    KnockbackResistance,
    SpawnReinforcements,
    JumpStrength,
    Armor,
    BurningTime,
    /// `Attributes.FLYING_SPEED`: what `FlyingMoveControl` flies at.
    FlyingSpeed,
}
impl Attr {
    pub const ALL: [Self; 10] = [
        Self::MaxHealth,
        Self::MovementSpeed,
        Self::FollowRange,
        Self::AttackDamage,
        Self::KnockbackResistance,
        Self::SpawnReinforcements,
        Self::JumpStrength,
        Self::Armor,
        Self::BurningTime,
        Self::FlyingSpeed,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::MaxHealth => "max_health",
            Self::MovementSpeed => "movement_speed",
            Self::FollowRange => "follow_range",
            Self::AttackDamage => "attack_damage",
            Self::KnockbackResistance => "knockback_resistance",
            Self::SpawnReinforcements => "spawn_reinforcements",
            Self::JumpStrength => "jump_strength",
            Self::Armor => "armor",
            Self::BurningTime => "burning_time",
            Self::FlyingSpeed => "flying_speed",
        }
    }
    /// The attribute's registry id (`minecraft:` and its name), without
    /// building it: mob ticks and entity tracking ask for every mob.
    pub fn id(self) -> &'static str {
        match self {
            Self::MaxHealth => "minecraft:max_health",
            Self::MovementSpeed => "minecraft:movement_speed",
            Self::FollowRange => "minecraft:follow_range",
            Self::AttackDamage => "minecraft:attack_damage",
            Self::KnockbackResistance => "minecraft:knockback_resistance",
            Self::SpawnReinforcements => "minecraft:spawn_reinforcements",
            Self::JumpStrength => "minecraft:jump_strength",
            Self::Armor => "minecraft:armor",
            Self::BurningTime => "minecraft:burning_time",
            Self::FlyingSpeed => "minecraft:flying_speed",
        }
    }
}
/// The definition of each [`Attr`] and of `max_absorption`
/// (`RangedAttribute`'s default and bounds), read once from the attribute
/// catalog when the mob types load: what a mob's own attribute record
/// sanitizes its values with, as `AttributeInstance` does. A type built
/// without a catalog (a test fixture, the player's render type) has none,
/// and its values are not clamped.
#[derive(Clone, Copy, Debug, Default)]
pub struct AttrBounds {
    attrs: [Option<horizon_world::attributes::Definition>; Attr::ALL.len()],
    /// `Attributes.MAX_ABSORPTION`'s, for a mob with no attribute map yet.
    max_absorption: Option<horizon_world::attributes::Definition>,
}
impl AttrBounds {
    /// Each attribute's definition in `catalog`.
    pub fn of(catalog: &horizon_world::attributes::entity::Catalog) -> Self {
        Self {
            attrs: Attr::ALL.map(|a| catalog.definition(a.id())),
            max_absorption: catalog.definition("minecraft:max_absorption"),
        }
    }
    /// `RangedAttribute.sanitizeValue` of `a`: NaN to the minimum, then into
    /// the bounds.
    pub fn sanitize(&self, a: Attr, value: f64) -> f64 {
        sanitize(self.attrs[a as usize], value)
    }
    /// The saved base `base` of the attribute `id` as a load takes it
    /// ([`crate::living_attributes::loaded_base`]): clamped into an [`Attr`]'s
    /// bounds when a mod that widened them is gone.
    pub fn loaded_base(&self, id: &str, base: f64) -> f64 {
        Attr::ALL
            .into_iter()
            .find(|a| a.id() == id)
            .and_then(|a| self.attrs[a as usize])
            .map_or(base, |definition| {
                crate::living_attributes::loaded_base(id, definition, base)
            })
    }
    /// `RangedAttribute.sanitizeValue` of `max_absorption`.
    pub fn sanitize_max_absorption(&self, value: f64) -> f64 {
        sanitize(self.max_absorption, value)
    }
}
fn sanitize(definition: Option<horizon_world::attributes::Definition>, value: f64) -> f64 {
    definition.map_or(value, |definition| definition.sanitize(value))
}
/// `AttributeModifier.Operation`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    AddValue,
    AddMultipliedBase,
    AddMultipliedTotal,
}
/// A permanent attribute modifier (`AttributeModifier`).
#[derive(Clone, Debug, PartialEq)]
pub struct Modifier {
    pub attr: Attr,
    pub id: std::borrow::Cow<'static, str>,
    pub amount: f64,
    pub op: Op,
}
/// Attribute base values that differ from the type's and the permanent
/// modifiers finalizeSpawn added, in the order vanilla adds them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Attributes {
    pub bases: Vec<(Attr, f64)>,
    pub modifiers: Vec<Modifier>,
    /// The transient modifiers mods added: in the value and synced, never
    /// saved.
    pub transient: Vec<Modifier>,
    /// `NAME_TAG_DISTANCE` from a server attribute update: the client reads
    /// it for nameplates and a save never writes it.
    pub name_tag_distance: Option<f64>,
    /// `BELOW_NAME_DISTANCE` from a server attribute update.
    pub below_name_distance: Option<f64>,
    /// The transient minecraft:baby speed modifier was taken away
    /// (`/attribute ... modifier remove`) until the next setBaby. Not
    /// saved: a load runs setBaby again.
    pub baby_speed_removed: bool,
    /// The maximum health changed since the mob's last tick
    /// (`AttributeMap.getAttributesToUpdate`). Not saved.
    pub max_health_dirty: bool,
}
/// The attribute `/attribute` names (`follow_range`, not
/// `minecraft:follow_range`) when gameplay reads it from this store. Armor
/// has its own map (`armor_attributes`).
pub fn command_attr(short: &str) -> Option<Attr> {
    Attr::ALL
        .into_iter()
        .find(|a| *a != Attr::Armor && a.name() == short)
}
/// `Goat` and `Hoglin.ageBoundaryReached`: the attack damage of a baby or an adult.
fn age_attack_damage(s: Species, baby: bool) -> Option<f64> {
    match s {
        Species::Goat => Some(if baby { 1.0 } else { 2.0 }),
        Species::Hoglin => Some(if baby { 0.5 } else { 6.0 }),
        _ => None,
    }
}
/// The attribute modifier one status effect grants (`MobEffect.addAttributeModifier`),
/// scaled by the amplifier plus one; `None` when the effect does not touch `a`.
fn effect_modifier(effect: &str, amplifier: i32, a: Attr) -> Option<(f64, Op)> {
    let level = f64::from(amplifier + 1);
    match (effect.trim_start_matches("minecraft:"), a) {
        ("strength", Attr::AttackDamage) => Some((3.0 * level, Op::AddValue)),
        ("weakness", Attr::AttackDamage) => Some((-4.0 * level, Op::AddValue)),
        ("speed", Attr::MovementSpeed) => Some((f64::from(0.2f32) * level, Op::AddMultipliedTotal)),
        ("slowness", Attr::MovementSpeed) => {
            Some((f64::from(-0.15f32) * level, Op::AddMultipliedTotal))
        }
        ("health_boost", Attr::MaxHealth) => Some((4.0 * level, Op::AddValue)),
        _ => None,
    }
}
/// `EquipmentSlot.getSerializedName`.
fn slot_name(slot: EquipmentSlot) -> &'static str {
    match slot {
        EquipmentSlot::MainHand => "mainhand",
        EquipmentSlot::OffHand => "offhand",
        EquipmentSlot::Feet => "feet",
        EquipmentSlot::Legs => "legs",
        EquipmentSlot::Chest => "chest",
        EquipmentSlot::Head => "head",
        EquipmentSlot::Body => "body",
        EquipmentSlot::Saddle => "saddle",
    }
}

// ---- species facts --------------------------------------------------------

/// `AgeableMob` subclasses.
pub fn ageable(s: Species) -> bool {
    use Species as S;
    matches!(
        s,
        S::Pig
            | S::Cow
            | S::Mooshroom
            | S::Sheep
            | S::Chicken
            | S::Rabbit
            | S::Horse
            | S::Donkey
            | S::Mule
            | S::Llama
            | S::TraderLlama
            | S::Goat
            | S::Panda
            | S::PolarBear
            | S::Fox
            | S::Wolf
            | S::Cat
            | S::Ocelot
            | S::Bee
            | S::Turtle
            | S::Armadillo
            | S::Frog
            | S::Axolotl
            | S::Squid
            | S::GlowSquid
            | S::Dolphin
            | S::Villager
            | S::WanderingTrader
            | S::Slime
            | S::MagmaCube
            | S::SulfurCube
            | S::Hoglin
            | S::Strider
            | S::Camel
            | S::CamelHusk
            | S::Sniffer
            | S::HappyGhast
            | S::Nautilus
            | S::ZombieNautilus
            | S::ZombieHorse
            | S::SkeletonHorse
    )
}
fn zombie(s: Species) -> bool {
    matches!(
        s,
        Species::Zombie
            | Species::Husk
            | Species::Drowned
            | Species::ZombieVillager
            | Species::ZombifiedPiglin
    )
}
fn skeleton(s: Species) -> bool {
    matches!(
        s,
        Species::Skeleton
            | Species::Stray
            | Species::Bogged
            | Species::Parched
            | Species::WitherSkeleton
    )
}
fn raider(s: Species) -> bool {
    super::raider::is_raider(s)
}
/// `EntityType.isAllowedInPeaceful`: the types built with `notInPeaceful`.
pub fn allowed_in_peaceful(kind: &str) -> bool {
    !matches!(
        kind.strip_prefix("minecraft:").unwrap_or(kind),
        "blaze"
            | "bogged"
            | "breeze"
            | "cave_spider"
            | "creaking"
            | "creeper"
            | "drowned"
            | "elder_guardian"
            | "enderman"
            | "endermite"
            | "evoker"
            | "ghast"
            | "giant"
            | "guardian"
            | "hoglin"
            | "husk"
            | "illusioner"
            | "magma_cube"
            | "parched"
            | "phantom"
            | "piglin_brute"
            | "pillager"
            | "ravager"
            | "silverfish"
            | "skeleton"
            | "slime"
            | "spider"
            | "stray"
            | "vex"
            | "vindicator"
            | "warden"
            | "witch"
            | "wither"
            | "wither_skeleton"
            | "zoglin"
            | "zombie"
            | "zombie_villager"
            | "zombified_piglin"
    )
}
/// `AgeableMob.getBabyStartAge` (a sniffer's is -48000).
pub fn baby_start_age(s: Species) -> i32 {
    if s == Species::Sniffer {
        super::sniffer::BABY_START_AGE
    } else {
        -24000
    }
}
/// The age a non-ageable baby (zombies, piglins, zoglins) keeps.
pub const NON_AGEABLE_BABY: i32 = i32::MIN / 2;

/// `SpawnEggItem.spawnsEntity`: the spawn egg is for the mob's own kind, so
/// it is used on it (`Mob.checkAndHandleImportantInteractions`).
pub fn spawn_egg_is_for(egg: &str, kind: &str) -> bool {
    egg.strip_suffix("_spawn_egg") == Some(kind)
}


// ---- equipment ------------------------------------------------------------

/// `EquipmentSlot` names (the `drops::SLOTS` order).
pub const SLOT_NAMES: [&str; 8] = super::drops::SLOTS;
pub use horizon_items::equipment::{CHEST, FEET, HEAD, LEGS, MAINHAND, OFFHAND};
const SADDLE: usize = horizon_items::equipment::SADDLE;
/// The `gear` slot of an `EquipmentSlot`.
pub fn gear_slot(slot: usize) -> usize {
    match slot {
        MAINHAND => gear::MAIN,
        OFFHAND => gear::OFF,
        FEET => gear::FEET,
        LEGS => gear::LEGS,
        CHEST => gear::CHEST,
        _ => gear::HEAD,
    }
}

/// `SpecialDates.isHalloween`: October 31.
pub fn halloween_date(month: u32, day: u32) -> bool {
    (month, day) == (10, 31)
}

/// `SpecialDates.isHalloween` from the local calendar.
pub fn is_halloween() -> bool {
    use chrono::Datelike;
    let today = chrono::Local::now();
    halloween_date(today.month(), today.day())
}


impl<L: FinalizeLevel + ?Sized, R: RandomSource> Ctx<'_, L, R> {
    fn float(&mut self) -> f32 {
        self.random.next_float()
    }
    fn int(&mut self, bound: i32) -> i32 {
        self.random.next_int_bound(bound).unwrap_or(0)
    }
    fn special(&self) -> f32 {
        self.difficulty.special()
    }
    /// `UniformInt.sample`.
    fn uniform(&mut self, min: i32, max: i32) -> i32 {
        self.random
            .next_int_between_inclusive(min, max)
            .unwrap_or(min)
    }

    // ---- Mob ----------------------------------------------------------


    // ---- jockeys ------------------------------------------------------


    // ---- the classes --------------------------------------------------

    fn horse_health(&mut self) -> f32 {
        15.0 + self.int(8) as f32 + self.int(9) as f32
    }
    fn horse_speed(&mut self) -> f64 {
        (f64::from(0.45f32)
            + self.random.next_double() * 0.3
            + self.random.next_double() * 0.3
            + self.random.next_double() * 0.3)
            * 0.25
    }
    fn horse_jump(&mut self) -> f64 {
        f64::from(0.4f32)
            + self.random.next_double() * 0.2
            + self.random.next_double() * 0.2
            + self.random.next_double() * 0.2
    }

}


/// `Panda.Gene.getRandom` (enum order: normal, lazy, worried, playful,
/// brown, weak, aggressive).
pub fn panda_gene(roll: i32) -> u8 {
    match roll {
        0 => 1,
        1 => 2,
        2 => 3,
        4 => 6,
        r if r < 9 => 5,
        r if r < 11 => 4,
        _ => 0,
    }
}
/// `Panda.Gene.getVariantFromGenes`: a recessive gene shows when both match.
pub fn panda_variant(main: u8, hidden: u8) -> u8 {
    let recessive = matches!(main, 4 | 5);
    if recessive && main != hidden { 0 } else { main }
}
/// The index the game keeps for a variant registry entry.
pub fn variant_index(s: Species, name: &str) -> u8 {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    let i = match s {
        Species::Cat => super::tame::CAT_VARIANTS.iter().position(|v| *v == name),
        Species::Wolf => super::tame::WOLF_VARIANTS.iter().position(|v| v.0 == name),
        Species::ZombieNautilus => super::nautilus::VARIANTS.iter().position(|v| *v == name),
        // MushroomCow.Variant: red is the codec's default, brown the other.
        Species::Mooshroom => ["red", "brown"].iter().position(|v| *v == name),
        // Fox.Variant, read from a `minecraft:fox/variant` component; the save
        // keeps the same two names under `Type`.
        Species::Fox => ["red", "snow"].iter().position(|v| *v == name),
        // Horse.Variant and Llama.Variant, the coat colours their
        // `.../variant` components carry; a trader llama inherits the llama's.
        Species::Horse => super::equine::VARIANT_NAMES.iter().position(|v| *v == name),
        Species::Llama | Species::TraderLlama => {
            super::llama::VARIANTS.iter().position(|v| *v == name)
        }
        _ => ["temperate", "warm", "cold"]
            .iter()
            .position(|v| *v == name),
    };
    i.unwrap_or(0) as u8
}
/// [`variant_component`] of a species with the game's variant index.
pub fn species_variant_component(s: Species, variant: u8) -> Option<(&'static str, String)> {
    let component = match s {
        Species::Wolf => "minecraft:wolf/variant",
        Species::Cat => "minecraft:cat/variant",
        Species::Frog => "minecraft:frog/variant",
        Species::Pig => "minecraft:pig/variant",
        Species::Cow => "minecraft:cow/variant",
        Species::Mooshroom => "minecraft:mooshroom/variant",
        Species::Chicken => "minecraft:chicken/variant",
        Species::ZombieNautilus => "minecraft:zombie_nautilus/variant",
        Species::Horse => "minecraft:horse/variant",
        Species::Llama | Species::TraderLlama => "minecraft:llama/variant",
        _ => return None,
    };
    Some((component, format!("minecraft:{}", variant_name(s, variant))))
}
/// Whether the mob's save keeps its variant under `variant` — the registry
/// id `VariantUtils.writeVariant` stores — rather than under its own key (a
/// fox's and a mooshroom's `Type`) or a packed int (a horse's and a llama's
/// `Variant`). Only these seven classes call `writeVariant`, so reporting the
/// component does not mean the save carries the name.
pub fn saves_variant_name(s: Species) -> bool {
    matches!(
        s,
        Species::Wolf
            | Species::Cat
            | Species::Frog
            | Species::Pig
            | Species::Cow
            | Species::Chicken
            | Species::ZombieNautilus
    )
}
/// The `*_sound_variant` registry of a species, in registry order.
pub fn sound_variants(s: Species) -> &'static [&'static str] {
    match s {
        Species::Pig => &["big", "classic", "mini"],
        Species::Cow => &["classic", "moody"],
        Species::Chicken => &["classic", "picky"],
        Species::Cat => &["classic", "royal"],
        Species::Wolf => &["angry", "big", "classic", "cute", "grumpy", "puglin", "sad"],
        _ => &[],
    }
}
/// The registry name of the game's variant index.
pub fn variant_name(s: Species, index: u8) -> &'static str {
    let i = index as usize;
    match s {
        Species::Cat => super::tame::CAT_VARIANTS.get(i).copied().unwrap_or("black"),
        Species::Wolf => super::tame::WOLF_VARIANTS.get(i).map_or("pale", |v| v.0),
        Species::ZombieNautilus => super::nautilus::VARIANTS
            .get(i)
            .copied()
            .unwrap_or("temperate"),
        Species::Mooshroom => ["red", "brown"].get(i).copied().unwrap_or("red"),
        Species::Horse => super::equine::VARIANT_NAMES
            .get(i)
            .copied()
            .unwrap_or("white"),
        Species::Llama | Species::TraderLlama => {
            super::llama::VARIANTS.get(i).copied().unwrap_or("creamy")
        }
        _ => ["temperate", "warm", "cold"]
            .get(i)
            .copied()
            .unwrap_or("temperate"),
    }
}
/// `VillagerType.BY_BIOME` (index into `render::VILLAGER_TYPES`: plains,
/// desert, jungle, savanna, snow, swamp, taiga); other biomes are plains.
pub const BIOME_VILLAGER_TYPES: &[(&str, u8)] = &[
    ("badlands", 1),
    ("desert", 1),
    ("eroded_badlands", 1),
    ("wooded_badlands", 1),
    ("bamboo_jungle", 2),
    ("jungle", 2),
    ("sparse_jungle", 2),
    ("savanna_plateau", 3),
    ("savanna", 3),
    ("windswept_savanna", 3),
    ("deep_frozen_ocean", 4),
    ("frozen_ocean", 4),
    ("frozen_river", 4),
    ("ice_spikes", 4),
    ("snowy_beach", 4),
    ("snowy_taiga", 4),
    ("snowy_plains", 4),
    ("grove", 4),
    ("snowy_slopes", 4),
    ("frozen_peaks", 4),
    ("jagged_peaks", 4),
    ("swamp", 5),
    ("mangrove_swamp", 5),
    ("old_growth_spruce_taiga", 6),
    ("old_growth_pine_taiga", 6),
    ("windswept_gravelly_hills", 6),
    ("windswept_hills", 6),
    ("taiga", 6),
    ("windswept_forest", 6),
];
/// `VillagerType.byBiome` over vanilla's table.
pub fn villager_type(biome: &str) -> u8 {
    let biome = biome.strip_prefix("minecraft:").unwrap_or(biome);
    BIOME_VILLAGER_TYPES
        .iter()
        .find(|(b, _)| *b == biome)
        .map_or(0, |(_, t)| *t)
}

#[cfg(test)]
mod tests {
    use super::*;



    #[test]
    fn panda_genes_follow_the_vanilla_table() {
        assert_eq!(panda_gene(0), 1);
        assert_eq!(panda_gene(3), 5);
        assert_eq!(panda_gene(4), 6);
        assert_eq!(panda_gene(9), 4);
        assert_eq!(panda_gene(15), 0);
        assert_eq!(panda_variant(4, 0), 0);
        assert_eq!(panda_variant(4, 4), 4);
        assert_eq!(panda_variant(6, 0), 6);
    }
    /// `MushroomCow.get(DataComponentGetter)`: a mooshroom reports its colour
    /// as the variant component, and the name maps back to the index.
    #[test]
    fn a_mooshroom_reports_its_variant_component() {
        for (index, name) in [(0, "minecraft:red"), (1, "minecraft:brown")] {
            assert_eq!(
                species_variant_component(Species::Mooshroom, index),
                Some(("minecraft:mooshroom/variant", name.to_owned()))
            );
            assert_eq!(variant_index(Species::Mooshroom, name), index);
            assert_eq!(
                format!("minecraft:{}", variant_name(Species::Mooshroom, index)),
                name
            );
        }
        // Variant.DEFAULT is red: an unknown name, or an index past brown.
        assert_eq!(variant_index(Species::Mooshroom, "minecraft:temperate"), 0);
        assert_eq!(variant_name(Species::Mooshroom, 2), "red");
    }
    /// `Horse.get(DataComponents.HORSE_VARIANT)` and
    /// `Llama.get(DataComponents.LLAMA_VARIANT)`: the coat colour a horse or a
    /// llama reports, and the names a `.../variant` component carries.
    #[test]
    fn a_horse_and_a_llama_report_their_variant_component() {
        for (species, component, names) in [
            (
                Species::Horse,
                "minecraft:horse/variant",
                &crate::mobs::equine::VARIANT_NAMES[..],
            ),
            (
                Species::Llama,
                "minecraft:llama/variant",
                &crate::mobs::llama::VARIANTS[..],
            ),
            (
                Species::TraderLlama,
                "minecraft:llama/variant",
                &crate::mobs::llama::VARIANTS[..],
            ),
        ] {
            for (index, name) in names.iter().enumerate() {
                let index = index as u8;
                let name = format!("minecraft:{name}");
                assert_eq!(
                    species_variant_component(species, index),
                    Some((component, name.clone())),
                    "{species:?} {index}"
                );
                assert_eq!(variant_index(species, &name), index, "{name}");
                assert_eq!(format!("minecraft:{}", variant_name(species, index)), name);
            }
        }
        // Horse.Variant.WHITE and Llama.Variant.CREAMY are the defaults.
        assert_eq!(variant_index(Species::Horse, "minecraft:temperate"), 0);
        assert_eq!(variant_index(Species::Llama, "minecraft:temperate"), 0);
        assert_eq!(variant_name(Species::Horse, 7), "white");
        assert_eq!(variant_name(Species::TraderLlama, 4), "creamy");
        assert_eq!(variant_name(Species::Cat, 20), "black");
        // Only `VariantUtils.writeVariant` species save the name; a horse and
        // a llama save a packed `Variant` int, a fox and a mooshroom `Type`.
        for species in [Species::Wolf, Species::Cat, Species::Chicken, Species::Pig] {
            assert!(saves_variant_name(species), "{species:?}");
        }
        for species in [
            Species::Horse,
            Species::Llama,
            Species::TraderLlama,
            Species::Fox,
            Species::Mooshroom,
        ] {
            assert!(!saves_variant_name(species), "{species:?}");
        }
    }
    /// #3887: Weakness and Slowness (and Health Boost) carry attribute
    /// modifiers in vanilla (`MobEffects`), and every effect's amount is
    /// scaled by the amplifier plus one (`MobEffect.addAttributeModifier`);
    /// before this only strength and speed were mapped.
    #[test]
    fn weakness_slowness_and_health_boost_carry_attribute_modifiers() {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        let modifier = |effect: &str, amplifier: i32, attr: Attr| {
            effect_modifier(effect, amplifier, attr).expect("the effect should carry a modifier")
        };
        // MobEffects.WEAKNESS: attack damage -4 per level, ADD_VALUE.
        let (amount, op) = modifier("minecraft:weakness", 0, Attr::AttackDamage);
        assert!(close(amount, -4.0));
        assert_eq!(op, Op::AddValue);
        let (amount, _) = modifier("weakness", 1, Attr::AttackDamage);
        assert!(close(amount, -8.0));
        // MobEffects.SLOWNESS: movement speed -0.15F per level, ADD_MULTIPLIED_TOTAL.
        let (amount, op) = modifier("minecraft:slowness", 0, Attr::MovementSpeed);
        assert!(close(amount, f64::from(-0.15f32)));
        assert_eq!(op, Op::AddMultipliedTotal);
        let (amount, _) = modifier("slowness", 1, Attr::MovementSpeed);
        assert!(close(amount, f64::from(-0.15f32) * 2.0));
        // MobEffects.HEALTH_BOOST: max health +4 per level, ADD_VALUE.
        let (amount, op) = modifier("minecraft:health_boost", 0, Attr::MaxHealth);
        assert!(close(amount, 4.0));
        assert_eq!(op, Op::AddValue);
        // The effects that already worked keep their vanilla amounts.
        let (amount, _) = modifier("strength", 0, Attr::AttackDamage);
        assert!(close(amount, 3.0));
        let (amount, _) = modifier("speed", 0, Attr::MovementSpeed);
        assert!(close(amount, f64::from(0.2f32)));
        // An effect does not touch attributes it does not modify.
        assert!(effect_modifier("weakness", 0, Attr::MovementSpeed).is_none());
        assert!(effect_modifier("poison", 0, Attr::AttackDamage).is_none());
    }
    #[test]
    fn villager_types_by_biome() {
        assert_eq!(villager_type("minecraft:badlands"), 1);
        assert_eq!(villager_type("minecraft:grove"), 4);
        assert_eq!(villager_type("minecraft:plains"), 0);
    }
    #[test]
    fn october_31_is_halloween() {
        assert!(halloween_date(10, 31));
        assert!(!halloween_date(10, 30));
        assert!(!halloween_date(11, 1));
        assert!(!halloween_date(1, 1));
    }
}

