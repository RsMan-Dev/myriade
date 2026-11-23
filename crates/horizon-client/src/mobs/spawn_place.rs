//! The placement rules of the natural spawning and of the spawners: the category's walk target and obstruction per species, the
//! heightmap predicates, `mob_spawn_rules`, the spawn boxes and `position_ok` / `spawn_obstruction`, the structures' spawn overrides
//! (`StructureSpawns`), and the level's read trait `NaturalLevel`. (Lot 9.)
use super::Species;
use super::spawn_rules::{self, Reason, SpawnLevel};
use crate::biome::SpawnEntry;
use crate::fast_hash::FxHashMap;
use horizon_core::{
    geometry::{Aabb, Vec3},
    random::RandomSource,
};
use mod_api::{SpawnCheck, SpawnFlags, SpawnLimits};
use serde_json::Value;
use std::{
    borrow::{Borrow, Cow},
    collections::HashMap,
    fs,
    path::Path,
};

pub use crate::biome::{SpawnCost, parse_costs, parse_entries};

/// `MobCategory`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Monster,
    Creature,
    Ambient,
    Axolotls,
    UndergroundWaterCreature,
    WaterCreature,
    WaterAmbient,
    Misc,
}
impl Category {
    pub const ALL: [Self; 8] = [
        Self::Monster,
        Self::Creature,
        Self::Ambient,
        Self::Axolotls,
        Self::UndergroundWaterCreature,
        Self::WaterCreature,
        Self::WaterAmbient,
        Self::Misc,
    ];
    /// `NaturalSpawner.SPAWNING_CATEGORIES`: all but MISC, in enum order.
    pub const SPAWNING: [Self; 7] = [
        Self::Monster,
        Self::Creature,
        Self::Ambient,
        Self::Axolotls,
        Self::UndergroundWaterCreature,
        Self::WaterCreature,
        Self::WaterAmbient,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Monster => "monster",
            Self::Creature => "creature",
            Self::Ambient => "ambient",
            Self::Axolotls => "axolotls",
            Self::UndergroundWaterCreature => "underground_water_creature",
            Self::WaterCreature => "water_creature",
            Self::WaterAmbient => "water_ambient",
            Self::Misc => "misc",
        }
    }
    pub fn from_name(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|c| c.name() == name)
            .unwrap_or(Self::Misc)
    }
    fn index(self) -> usize {
        self as usize
    }
    /// `getMaxInstancesPerChunk` under a level's `limits` (vanilla's are
    /// [`SpawnLimits::VANILLA`]); MISC's is -1, as it never spawns.
    pub fn max(self, limits: &SpawnLimits) -> i64 {
        i64::from(match self {
            Self::Monster => limits.monster,
            Self::Creature => limits.creature,
            Self::Ambient => limits.ambient,
            Self::Axolotls => limits.axolotls,
            Self::UndergroundWaterCreature => limits.underground_water_creature,
            Self::WaterCreature => limits.water_creature,
            Self::WaterAmbient => limits.water_ambient,
            Self::Misc => return -1,
        })
    }
    pub fn friendly(self) -> bool {
        self != Self::Monster
    }
    pub fn persistent(self) -> bool {
        matches!(self, Self::Creature | Self::Misc)
    }
    pub fn despawn_distance(self) -> i32 {
        if self == Self::WaterAmbient { 64 } else { 128 }
    }
}

/// `PathfinderMob.checkSpawnRules`: the class's `getWalkTargetValue` at the
/// spawn position must not be negative (plain `Mob`s accept any position).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WalkTarget {
    NotPathfinder,
    Zero,
    Animal,
    Mooshroom,
    Turtle,
    Monster,
    Giant,
    Guardian,
    Silverfish,
    Strider,
    Hoglin,
    Bee,
    HappyGhast,
}
fn walk_target(s: Species) -> WalkTarget {
    use Species as S;
    use WalkTarget as W;
    match s {
        S::Bat | S::Slime | S::MagmaCube | S::SulfurCube | S::Ghast | S::Phantom => {
            W::NotPathfinder
        }
        // Monster subclasses that override it with 0.
        S::Enderman | S::Creaking | S::Pillager | S::Warden => W::Zero,
        S::Giant => W::Giant,
        S::Guardian | S::ElderGuardian => W::Guardian,
        S::Silverfish => W::Silverfish,
        S::Zombie
        | S::Husk
        | S::Drowned
        | S::ZombieVillager
        | S::ZombifiedPiglin
        | S::Skeleton
        | S::Stray
        | S::Bogged
        | S::Parched
        | S::WitherSkeleton
        | S::Creeper
        | S::Spider
        | S::CaveSpider
        | S::Endermite
        | S::Witch
        | S::Vindicator
        | S::Evoker
        | S::Illusioner
        | S::Ravager
        | S::Piglin
        | S::PiglinBrute
        | S::Zoglin
        | S::Blaze
        | S::Vex
        | S::Breeze
        | S::Wither => W::Monster,
        S::Mooshroom => W::Mooshroom,
        S::Turtle => W::Turtle,
        S::Strider => W::Strider,
        S::Hoglin => W::Hoglin,
        S::Bee => W::Bee,
        S::HappyGhast => W::HappyGhast,
        // Animal subclasses with their own value.
        S::Axolotl | S::Nautilus | S::ZombieNautilus => W::Zero,
        S::Pig
        | S::Cow
        | S::Sheep
        | S::Chicken
        | S::Rabbit
        | S::Horse
        | S::Donkey
        | S::Mule
        | S::Llama
        | S::TraderLlama
        | S::SkeletonHorse
        | S::ZombieHorse
        | S::Camel
        | S::CamelHusk
        | S::Goat
        | S::Panda
        | S::PolarBear
        | S::Fox
        | S::Wolf
        | S::Cat
        | S::Ocelot
        | S::Parrot
        | S::Armadillo
        | S::Frog
        | S::Sniffer => W::Animal,
        // Other PathfinderMobs: water animals, golems, villagers, allays...
        _ => W::Zero,
    }
}
/// Each class's `checkSpawnObstruction`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Obstruction {
    /// `Mob`: no liquid in the box and no entity blocking it.
    NoLiquidUnobstructed,
    Unobstructed,
    Ocelot,
    NoLiquid,
}
fn obstruction(s: Species) -> Obstruction {
    use Species as S;
    match s {
        S::Squid
        | S::GlowSquid
        | S::Dolphin
        | S::Axolotl
        | S::Cod
        | S::Salmon
        | S::TropicalFish
        | S::Pufferfish
        | S::Tadpole
        | S::Nautilus
        | S::ZombieNautilus
        | S::Guardian
        | S::ElderGuardian
        | S::Strider
        | S::Drowned => Obstruction::Unobstructed,
        S::Ocelot => Obstruction::Ocelot,
        S::Ravager => Obstruction::NoLiquid,
        // ZombifiedPiglin's override is the Mob default's two tests; iron
        // golems and wardens never spawn naturally.
        _ => Obstruction::NoLiquidUnobstructed,
    }
}
/// `Mob.removeWhenFarAway` of a freshly created mob.
fn removed_when_far(s: Species) -> bool {
    use Species as S;
    match s {
        // Animal, golems, villagers, traders, allays and wardens never;
        // cats and ocelots only past 2400 ticks; chickens as jockeys.
        S::Axolotl
        | S::Cod
        | S::Salmon
        | S::TropicalFish
        | S::Pufferfish
        | S::Tadpole
        | S::CamelHusk
        | S::ZombieHorse
        | S::Hoglin
        | S::Nautilus
        | S::ZombieNautilus => true,
        S::Villager
        | S::WanderingTrader
        | S::IronGolem
        | S::SnowGolem
        | S::Shulker
        | S::Allay
        | S::Warden => false,
        _ => !is_animal(s),
    }
}
fn is_animal(s: Species) -> bool {
    use Species as S;
    matches!(walk_target(s), WalkTarget::Animal)
        || matches!(
            s,
            S::Axolotl
                | S::Nautilus
                | S::ZombieNautilus
                | S::Mooshroom
                | S::Turtle
                | S::Strider
                | S::Hoglin
                | S::Bee
                | S::HappyGhast
        )
}
/// `Mob.getMaxSpawnClusterSize`.
pub fn max_cluster_size(s: Species) -> u32 {
    use Species as S;
    match s {
        S::Horse
        | S::Donkey
        | S::Mule
        | S::Llama
        | S::TraderLlama
        | S::SkeletonHorse
        | S::ZombieHorse
        | S::Camel
        | S::CamelHusk => 6,
        S::Cod | S::TropicalFish | S::Pufferfish | S::Tadpole | S::Wolf => 8,
        S::Salmon => 5,
        S::Ghast | S::HappyGhast | S::Pillager => 1,
        _ => 4,
    }
}
/// Whether a group of `size` mobs has reached its limit: `parts`, the limit the new mob's
/// entity parts answered, else `Mob.getMaxSpawnClusterSize` of its species.
fn cluster_full(size: u32, parts: Option<u32>, species: Species) -> bool {
    size >= parts.unwrap_or_else(|| max_cluster_size(species))
}
/// `EntityType.canSpawnFarFromPlayer`: creatures, misc, pillagers, shulkers.
fn spawns_far_from_player(s: Species, category: Category) -> bool {
    matches!(category, Category::Creature | Category::Misc)
        || matches!(s, Species::Pillager | Species::Shulker)
}


/// What natural spawning reads from a level beyond the spawn rules.
/// An entity type's species, width, height and category.
pub type MobType = (Species, f32, f32, Category);

pub trait NaturalLevel: SpawnLevel {
    /// `MobSpawnEvent.PositionCheck`: what server mods answer for the new mob
    /// of type `entity` at `at`, described by `pending`; `Default` leaves the
    /// checks to vanilla.
    fn position_check(
        &self,
        _entity: &str,
        _reason: Reason,
        _at: [f64; 3],
        _pending: Pending,
    ) -> SpawnCheck {
        SpawnCheck::Default
    }
    /// The position check just asked refused the mob: what the level made
    /// for it is dropped.
    fn position_refused(&self) {}
    /// Whether server mods hear `potential_spawns`.
    fn spawn_hooks(&self) -> bool {
        false
    }
    /// `LevelEvent.PotentialSpawns`: server mods change the possible spawns
    /// at a position.
    fn potential_spawns(
        &self,
        _category: Category,
        _pos: [i32; 3],
        _entries: &mut Vec<SpawnEntry>,
    ) {
    }
    fn min_y(&self) -> i32;
    /// `getMaxY()`: the highest buildable block.
    fn max_y(&self) -> i32;
    /// `DimensionType.ambientLight`.
    fn ambient_light(&self) -> f32;
    fn has_ceiling(&self) -> bool;
    /// The entity type's species, width, height and category.
    fn mob_type(&self, entity: &str) -> Option<MobType>;
    /// The `natural_mob_spawns` lists by category at a position.
    fn biome_spawns(&self, x: i32, y: i32, z: i32) -> Option<&HashMap<String, Vec<SpawnEntry>>>;
    /// The `natural_mob_spawns` spawn cost of a type at a position.
    fn spawn_cost(&self, entity: &str, x: i32, y: i32, z: i32) -> Option<SpawnCost>;
    /// The `creature_world_gen_spawn_probability` attribute at a position.
    fn creature_probability(&self, x: i32, y: i32, z: i32) -> f32;
    /// Every structure whose references reach the position's chunk and
    /// whose id `wanted` takes, in the chunk's reference order.
    fn structures_at(
        &self,
        x: i32,
        y: i32,
        z: i32,
        wanted: &dyn Fn(&str) -> bool,
    ) -> Vec<StructureAt>;
    fn structure_spawns(&self) -> &StructureSpawns;
    /// Where the players `getNearestPlayer(x, y, z, -1, false)` measures to
    /// are: every one that is not a spectator.
    fn nearest_player_positions(&self) -> &[[f64; 3]];
    /// Those of [`Self::nearest_player_positions`] within `extent` of
    /// `(x, z)` on both axes, and possibly others: the caller tests each. A
    /// level with many players indexes them ([`PlayerSquares`]).
    fn player_positions_near(&self, x: f64, z: f64, extent: f64, out: &mut Vec<[f64; 3]>) {
        let _ = (x, z, extent);
        out.extend_from_slice(self.nearest_player_positions());
    }
    /// The world spawn point, when it is in this dimension.
    fn respawn(&self) -> Option<[i32; 3]>;
    /// `ServerLevel.canSpawnEntitiesInChunk`.
    fn can_spawn_in_chunk(&self, cx: i32, cz: i32) -> bool;
    /// `noCollision(null, box)`: blocks under an empty collision context and
    /// entities that can be collided with.
    fn no_collision(&self, bounds: Aabb) -> bool;
    /// `isUnobstructed(entity)`: no entity blocking building overlaps.
    fn unobstructed(&self, bounds: Aabb) -> bool;
    /// The `spawn_mobs` game rule.
    fn spawn_mobs(&self) -> bool;
    /// `getHeight(heightmap, x, z)`.
    fn heightmap(&self, kind: Heightmap, x: i32, z: i32) -> i32;
    /// `isPathfindable(PathComputationType.LAND)` of the block.
    fn pathfindable_land(&self, x: i32, y: i32, z: i32) -> bool;
}

/// The heightmap types `SpawnPlacements` registers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heightmap {
    WorldSurface,
    OceanFloor,
    MotionBlocking,
    MotionBlockingNoLeaves,
}
impl Heightmap {
    pub fn from_name(name: &str) -> Self {
        match name {
            "WORLD_SURFACE" => Self::WorldSurface,
            "OCEAN_FLOOR" => Self::OceanFloor,
            "MOTION_BLOCKING" => Self::MotionBlocking,
            _ => Self::MotionBlockingNoLeaves,
        }
    }
}

/// `Heightmap.Types`' predicates: WORLD_SURFACE (not air), OCEAN_FLOOR
/// (`#blocks_motion_in_heightmap`), MOTION_BLOCKING (that or a fluid) and
/// MOTION_BLOCKING_NO_LEAVES (`#blocks_motion_in_heightmap_no_leaves` or a fluid).
pub fn heightmap_blocks<L: SpawnLevel>(l: &L, kind: Heightmap, x: i32, y: i32, z: i32) -> bool {
    let state = l.state(x, y, z);
    let f = l.facts().get(state);
    let fluid = f.fluid != crate::blocks::Fluid::None;
    match kind {
        Heightmap::WorldSurface => !l.facts().is_air(state),
        Heightmap::OceanFloor => l.block_tag("minecraft:blocks_motion_in_heightmap", &f.name),
        Heightmap::MotionBlocking => {
            fluid || l.block_tag("minecraft:blocks_motion_in_heightmap", &f.name)
        }
        Heightmap::MotionBlockingNoLeaves => {
            fluid || l.block_tag("minecraft:blocks_motion_in_heightmap_no_leaves", &f.name)
        }
    }
}

/// `LevelReader.getLightLevelDependentMagicValue`.
fn magic_light<L: NaturalLevel>(l: &L, x: i32, y: i32, z: i32) -> f32 {
    let raw = l
        .block_light(x, y, z)
        .max(l.sky_light(x, y, z) - l.sky_darken());
    let v = raw as f32 / 15.0;
    let curved = v / (4.0 - 3.0 * v);
    curved + l.ambient_light() * (1.0 - curved)
}
fn cost_from_light<L: NaturalLevel>(l: &L, x: i32, y: i32, z: i32) -> f32 {
    magic_light(l, x, y, z) - 0.5
}
fn name_at<L: NaturalLevel>(l: &L, x: i32, y: i32, z: i32) -> &str {
    &l.facts().get(l.state(x, y, z)).name
}
fn fluid_at<L: NaturalLevel>(l: &L, x: i32, y: i32, z: i32) -> crate::blocks::Fluid {
    l.facts().get(l.state(x, y, z)).fluid
}
/// `checkSpawnRules(level, NATURAL)` of a freshly created mob at a block.
pub(crate) fn mob_spawn_rules<L: NaturalLevel>(s: Species, l: &L, x: i32, y: i32, z: i32) -> bool {
    use crate::blocks::Fluid;
    let value = match walk_target(s) {
        WalkTarget::NotPathfinder | WalkTarget::Zero => 0.0,
        WalkTarget::Animal => {
            if name_at(l, x, y - 1, z) == "minecraft:grass_block" {
                10.0
            } else {
                cost_from_light(l, x, y, z)
            }
        }
        WalkTarget::Mooshroom => {
            if name_at(l, x, y - 1, z) == "minecraft:mycelium" {
                10.0
            } else {
                cost_from_light(l, x, y, z)
            }
        }
        WalkTarget::Turtle => {
            if fluid_at(l, x, y, z) == Fluid::Water
                || l.block_tag("minecraft:sand", name_at(l, x, y - 1, z))
            {
                10.0
            } else {
                cost_from_light(l, x, y, z)
            }
        }
        WalkTarget::Monster => -cost_from_light(l, x, y, z),
        WalkTarget::Giant => cost_from_light(l, x, y, z),
        WalkTarget::Guardian => {
            if fluid_at(l, x, y, z) == Fluid::Water {
                10.0 + cost_from_light(l, x, y, z)
            } else {
                -cost_from_light(l, x, y, z)
            }
        }
        WalkTarget::Silverfish => {
            if infested_host(name_at(l, x, y - 1, z)) {
                10.0
            } else {
                -cost_from_light(l, x, y, z)
            }
        }
        // A new strider is not in lava yet.
        WalkTarget::Strider => {
            if fluid_at(l, x, y, z) == Fluid::Lava {
                10.0
            } else {
                0.0
            }
        }
        // A new hoglin remembers no repellent.
        WalkTarget::Hoglin => {
            if name_at(l, x, y - 1, z) == "minecraft:crimson_nylium" {
                10.0
            } else {
                0.0
            }
        }
        WalkTarget::Bee => {
            if l.facts().is_air(l.state(x, y, z)) {
                10.0
            } else {
                0.0
            }
        }
        WalkTarget::HappyGhast => {
            let air = |dy| l.facts().is_air(l.state(x, y + dy, z));
            if !air(0) {
                0.0
            } else if air(-1) && !air(-2) {
                10.0
            } else {
                5.0
            }
        }
    };
    value >= 0.0
}

/// `InfestedBlock.isCompatibleHostBlock`.
pub fn infested_host(block: &str) -> bool {
    matches!(
        block,
        "minecraft:stone"
            | "minecraft:cobblestone"
            | "minecraft:stone_bricks"
            | "minecraft:mossy_stone_bricks"
            | "minecraft:cracked_stone_bricks"
            | "minecraft:chiseled_stone_bricks"
            | "minecraft:deepslate"
    )
}
/// `LevelReader.containsAnyLiquid`: the blocks from the box's minimum corner
/// to the block holding its maximum corner.
pub fn contains_any_liquid<L: SpawnLevel>(l: &L, b: Aabb) -> bool {
    let (lo, hi) = (b.min_position(), b.max_position());
    let (x0, y0, z0) = (
        lo.x.floor() as i32,
        lo.y.floor() as i32,
        lo.z.floor() as i32,
    );
    let (x1, y1, z1) = (
        hi.x.floor() as i32,
        hi.y.floor() as i32,
        hi.z.floor() as i32,
    );
    (x0..=x1).any(|x| {
        (y0..=y1).any(|y| {
            (z0..=z1).any(|z| l.facts().get(l.state(x, y, z)).fluid != crate::blocks::Fluid::None)
        })
    })
}
/// `EntityDimensions.makeBoundingBox`: the half width is a float.
pub fn spawn_box(x: f64, y: f64, z: f64, width: f32, height: f32) -> Aabb {
    let half = f64::from(width / 2.0);
    Aabb::new(
        Vec3::new(x - half, y, z - half),
        Vec3::new(x + half, y + f64::from(height), z + half),
    )
}
/// `EntityType.spawnDimensionsScale`: cube mobs are checked for room at
/// their largest spawn size.
fn spawn_dimensions_scale(s: Species) -> f32 {
    match s {
        Species::Slime | Species::MagmaCube => 4.0,
        Species::SulfurCube => 2.0,
        _ => 1.0,
    }
}
/// `EntityType.getSpawnAABB`.
pub fn spawn_aabb(s: Species, x: f64, y: f64, z: f64, width: f32, height: f32) -> Aabb {
    let scale = spawn_dimensions_scale(s);
    let half = f64::from(scale * width / 2.0);
    Aabb::new(
        Vec3::new(x - half, y, z - half),
        Vec3::new(x + half, y + f64::from(scale * height), z + half),
    )
}
/// The mob a position check is about, which the site has not created yet
/// (vanilla creates it before the check).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pending {
    /// The yaw the site creates it with, drawn before the check.
    pub yaw: f32,
    /// Whether the site creates it fresh from its type (`EntityType.create`):
    /// natural spawning, and a spawner whose spawn data is the type alone. A
    /// spawner with entity data loads it, and chunk generation's packs come
    /// from the generator.
    pub fresh: bool,
}
/// `Mob.checkSpawnRules` and `checkSpawnObstruction` of a freshly created
/// mob, unless a mod forces the answer (`MobSpawnEvent.PositionCheck`); then
/// `vanilla` does not run.
pub fn position_ok<L: NaturalLevel>(
    l: &L,
    entity: &str,
    reason: Reason,
    at: [f64; 3],
    pending: Pending,
    vanilla: impl FnOnce() -> bool,
) -> bool {
    let ok = match l.position_check(entity, reason, at, pending) {
        SpawnCheck::Succeed => true,
        SpawnCheck::Fail => false,
        _ => vanilla(),
    };
    if !ok {
        l.position_refused();
    }
    ok
}
/// `checkSpawnObstruction` of a freshly created mob with the given box, with
/// the mobs already `spawned` in the way; None in a generation region, which
/// has no entities (`WorldGenRegion.getEntities`).
pub fn spawn_obstruction<L: NaturalLevel>(
    s: Species,
    l: &L,
    bounds: Aabb,
    spawned: Option<&[Aabb]>,
    x: i32,
    y: i32,
    z: i32,
) -> bool {
    let unobstructed = |l: &L| {
        spawned.is_none_or(|s| l.unobstructed(bounds) && !s.iter().any(|b| b.intersects(bounds)))
    };
    match obstruction(s) {
        Obstruction::NoLiquidUnobstructed => !contains_any_liquid(l, bounds) && unobstructed(l),
        Obstruction::Unobstructed => unobstructed(l),
        Obstruction::NoLiquid => !contains_any_liquid(l, bounds),
        Obstruction::Ocelot => {
            if unobstructed(l) && !contains_any_liquid(l, bounds) {
                if y < l.sea_level() {
                    return false;
                }
                let below = name_at(l, x, y - 1, z);
                if below == "minecraft:grass_block" || l.block_tag("minecraft:leaves", below) {
                    return true;
                }
            }
            false
        }
    }
}


/// A structure's spawn override for one category.
#[derive(Clone, Debug)]
pub struct Override {
    /// `bounding_box: piece` (else the whole structure's box).
    pub piece: bool,
    pub spawns: Vec<SpawnEntry>,
}
/// Every structure's `spawn_overrides`, by structure id then category.
#[derive(Clone, Debug, Default)]
pub struct StructureSpawns {
    /// Fx-hashed: natural spawning asks for every structure reaching every
    /// position it tries.
    pub overrides: FxHashMap<String, FxHashMap<String, Override>>,
}
impl StructureSpawns {
    /// Read a fresh generation, including mod namespaces and nested structure ids.
    pub fn load(root: &Path) -> Self {
        fn insert(
            overrides: &mut FxHashMap<String, FxHashMap<String, Override>>,
            id: String,
            v: &Value,
        ) {
            let map: FxHashMap<String, Override> = v["spawn_overrides"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(category, o)| {
                    (
                        category.clone(),
                        Override {
                            piece: o["bounding_box"].as_str() == Some("piece"),
                            spawns: parse_entries(&o["spawns"]),
                        },
                    )
                })
                .collect();
            overrides.insert(id, map);
        }
        let mut overrides = FxHashMap::default();
        if let Some(Value::Object(entries)) = fs::read(root.join("worldgen/structures.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        {
            for (id, value) in entries {
                if let Some(value) = value
                    .as_str()
                    .and_then(|text| serde_json::from_str::<Value>(text).ok())
                {
                    insert(&mut overrides, id, &value);
                }
            }
        }
        for ns in fs::read_dir(root.join("data"))
            .into_iter()
            .flatten()
            .flatten()
        {
            let dir = ns.path().join("worldgen/structure");
            let Ok(files) = horizon_world::json_folder::list(&dir) else {
                continue;
            };
            for (name, path) in files {
                let Some(v) = fs::read(path)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                else {
                    continue;
                };
                insert(
                    &mut overrides,
                    format!("{}:{}", ns.file_name().to_string_lossy(), name),
                    &v,
                );
            }
        }
        Self { overrides }
    }
    pub fn get(&self, structure: &str, category: &str) -> Option<&Override> {
        self.overrides.get(structure)?.get(category)
    }
}

/// A structure whose references reach a position's chunk.
#[derive(Clone, Debug)]
pub struct StructureAt {
    pub id: String,
    /// A piece of one of its starts holds the position.
    pub in_piece: bool,
    /// One of its starts' bounding boxes holds the position.
    pub in_bounds: bool,
}

/// `NetherFortressStructure.FORTRESS_ENEMIES`.
pub fn fortress_enemies() -> &'static [SpawnEntry] {
    static ENEMIES: std::sync::LazyLock<Vec<SpawnEntry>> = std::sync::LazyLock::new(|| {
        let e = |entity: &str, weight, min, max, constant| SpawnEntry {
            entity: entity.to_owned(),
            weight,
            min,
            max,
            constant,
        };
        vec![
            e("minecraft:blaze", 10, 2, 3, false),
            e("minecraft:zombified_piglin", 5, 4, 4, true),
            e("minecraft:wither_skeleton", 8, 5, 5, true),
            e("minecraft:skeleton", 2, 5, 5, true),
            e("minecraft:magma_cube", 3, 4, 4, true),
        ]
    });
    &ENEMIES
}


fn total_weight(list: &[SpawnEntry]) -> i64 {
    list.iter().map(|e| i64::from(e.weight)).sum()
}

fn has_weight(list: &[SpawnEntry]) -> bool {
    total_weight(list) > 0
}

/// `WeightedList.getRandom`.
pub fn weighted_pick<'a>(
    list: &'a [SpawnEntry],
    random: &mut impl RandomSource,
) -> Option<&'a SpawnEntry> {
    let total = total_weight(list);
    if total <= 0 {
        return None;
    }
    // Keep vanilla's exact draw for representable totals. Larger modded lists use
    // one unsigned long modulo the total, a bounded, slightly biased fallback.
    let mut selection = if total <= i64::from(i32::MAX) {
        i64::from(random.next_int_bound(total as i32).ok()?)
    } else {
        ((random.next_long() as u64) % total as u64) as i64
    };
    for e in list {
        selection -= i64::from(e.weight);
        if selection < 0 {
            return Some(e);
        }
    }
    None
}

/// `SpawnerData.count().sample(random)`: a constant draws nothing.
pub fn sample_count(entry: &SpawnEntry, random: &mut impl RandomSource) -> u32 {
    if entry.constant {
        entry.min
    } else {
        let span = entry.max.saturating_sub(entry.min) as i32 + 1;
        entry.min + random.next_int_bound(span).unwrap_or(0) as u32
    }
}
/// `SpawnerData.equals`: the type and the count provider (the weight is not
/// part of it; a constant and a one-value uniform differ).
fn same_spawner(a: &SpawnEntry, b: &SpawnEntry) -> bool {
    a.entity == b.entity && a.constant == b.constant && a.min == b.min && a.max == b.max
}

/// An entity counted by `NaturalSpawner.createState`.

/// Player positions by the 256-block square they stand in, for
/// [`NaturalLevel::player_positions_near`]: every spawn start asks for the
/// players near it, and a server's players are spread over the map.
pub struct PlayerSquares {
    squares: FxHashMap<(i64, i64), Vec<[f64; 3]>>,
    /// Positions too far out (or not numbers) for a square: always handed over.
    unsquared: Vec<[f64; 3]>,
}
impl PlayerSquares {
    const SIDE: f64 = 256.0;
    /// Further out than any world border.
    const FAR: f64 = 1.0e9;
    pub fn new(positions: &[[f64; 3]]) -> Self {
        let mut squares = FxHashMap::<(i64, i64), Vec<[f64; 3]>>::default();
        let mut unsquared = Vec::new();
        for &p in positions {
            if p[0].abs() < Self::FAR && p[2].abs() < Self::FAR {
                squares.entry(Self::square(p[0], p[2])).or_default().push(p);
            } else {
                unsquared.push(p);
            }
        }
        Self { squares, unsquared }
    }
    fn square(x: f64, z: f64) -> (i64, i64) {
        (
            (x / Self::SIDE).floor() as i64,
            (z / Self::SIDE).floor() as i64,
        )
    }
    /// Every position within `extent` (at most a square's side) of `(x, z)`
    /// on both axes, and some others.
    pub fn near(&self, x: f64, z: f64, extent: f64, out: &mut Vec<[f64; 3]>) {
        out.extend_from_slice(&self.unsquared);
        if !(x.abs() < Self::FAR && z.abs() < Self::FAR && extent <= Self::SIDE) {
            out.extend(self.squares.values().flatten().copied());
            return;
        }
        let (sx, sz) = Self::square(x, z);
        for dx in -1..=1 {
            for dz in -1..=1 {
                if let Some(square) = self.squares.get(&(sx + dx, sz + dz)) {
                    out.extend_from_slice(square);
                }
            }
        }
    }
}
