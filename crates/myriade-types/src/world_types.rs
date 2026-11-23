use crate::*;


/// A dimension (`minecraft:the_nether`), as the game's table numbers it: a `LevelRef`'s identity.
/// A key stays the same dimension until the game exits; `key.id(host)` gives the name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DimensionKey(pub u32);

/// A load-owned fluid identity requested through [`Registrar::fluid_key`](FluidsRegistrar::fluid_key) or resolved through
/// [`DataLoadContext::fluid_key`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FluidKey(pub u32);

/// A server explosion about to happen, for `explosion_start`
/// (`ServerLevel.explode`'s `ServerExplosion`).
#[derive(Clone, Copy, Debug)]
pub struct ExplosionStart<'a> {
    /// The level it happens in (`minecraft:overworld`).
    pub dimension: &'a str,
    /// The explosion's center in blocks.
    pub center: [f64; 3],
    /// The explosion's radius in blocks.
    pub radius: f32,
    /// It sets fires (`createFire`).
    pub fire: bool,
    /// What the explosion does to blocks.
    pub interaction: BlockInteraction,
    /// The damage type of its damage source (`minecraft:explosion`,
    /// `minecraft:player_explosion`, `minecraft:bad_respawn_point`).
    pub damage_type: &'a str,
    /// The entity that explodes (`getDirectSourceEntity`): a creeper, a wind
    /// charge, the bearer of Wind Charged, when Horizon tracks it.
    pub source: Option<u32>,
    /// `getIndirectSourceEntity`: a living source itself, a primed TNT's
    /// owner, or a projectile's living owner (a ghast for its fireball, a
    /// wither for its skull, the thrower of a wind charge).
    pub owner: Option<u32>,
}

/// One server world open or client level; it rejects a previous world's handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorldEpoch(pub u64);

impl WorldEpoch {
    /// Create an epoch for the game.
    // hidden: a game-side helper, not part of the mod surface
    #[doc(hidden)]
    pub fn new(n: u64) -> Self {
        Self(n)
    }
    /// Read the epoch number.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// A block in a dimension, vanilla's `GlobalPos`: the dimension's key
/// (`minecraft:the_nether`) and the block. The dimension is data, so it need
/// not be open; `host.level(&pos.dimension().to_string())` gives its level when it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobalPos {
    pub dimension: Identifier,
    pub pos: BlockPos,
}

impl GlobalPos {
    /// A block in the dimension with key `dimension`.
    pub fn new(dimension: Identifier, pos: BlockPos) -> Self {
        Self { dimension, pos }
    }
    /// The dimension's key (`minecraft:overworld`).
    pub fn dimension(&self) -> &Identifier {
        &self.dimension
    }
    /// The block.
    pub fn pos(&self) -> BlockPos {
        self.pos
    }
}

impl std::hash::Hash for GlobalPos {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(
            &(self.dimension.namespace(), self.dimension.path(), self.pos),
            state,
        );
    }
}

/// A level of the open world, by its dimension (`minecraft:the_nether`).
/// `Host::level` makes one, and `entity.level(host)` gives an entity's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LevelRef {
    pub epoch: WorldEpoch,
    pub dimension: DimensionKey,
    /// Which open of a runtime level this names: 0 for a level every world has, the number the
    /// host gave one open of a level a mod created at run time.
    pub generation: u32,
}

impl LevelRef {
    /// Create a handle for the game.
    // hidden: a game-side helper, not part of the mod surface
    #[doc(hidden)]
    pub fn new(epoch: WorldEpoch, dimension: DimensionKey) -> Self {
        Self {
            epoch,
            dimension,
            generation: 0,
        }
    }
    /// Create a handle for one open of a runtime level.
    // hidden: a game-side helper, not part of the mod surface
    #[doc(hidden)]
    pub fn with_generation(epoch: WorldEpoch, dimension: DimensionKey, generation: u32) -> Self {
        Self {
            epoch,
            dimension,
            generation,
        }
    }
    /// The open of a runtime level the handle names; 0 for a level every world has.
    // hidden: a game-side helper; a handle of an earlier open reads as stale
    #[doc(hidden)]
    pub fn generation(self) -> u32 {
        self.generation
    }
    /// The level's dimension. `level.dimension().id(host)` gives its name.
    pub fn dimension(self) -> DimensionKey {
        self.dimension
    }
    /// The name of the level's dimension, for the game's own tables.
    // hidden: a game-side helper; mods ask the host
    #[doc(hidden)]
    pub fn dimension_id(self) -> &'static str {
        crate::dimension_id(self.dimension)
            .expect("the game's table made the key")
    }
    /// The open of the world the handle belongs to.
    pub fn epoch(self) -> WorldEpoch {
        self.epoch
    }
}

/// What made a `tags_updated` notice fire (`TagsUpdatedEvent`'s two subclasses).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagsUpdateCause {
    /// `TagsUpdatedEvent.ServerDataLoad`: a server built its data tables.
    ServerDataLoad {
        /// False on a world's first open, true after `/reload`.
        reload: bool,
    },
    /// `TagsUpdatedEvent.ClientPacketReceived`: a client applied the tags its server sent.
    ClientPacketReceived {
        /// The server is the single player window's own.
        integrated_server: bool,
    },
}

impl TagsUpdateCause {
    /// `TagsUpdatedEvent.shouldUpdateStaticData`: whether to update static data, which single
    /// player's two threads share, so that there only the server side updates it.
    pub fn should_update_static_data(self) -> bool {
        !matches!(
            self,
            Self::ClientPacketReceived {
                integrated_server: true
            }
        )
    }
}

/// What made a `data_maps_updated` notice fire (`DataMapsUpdatedEvent.UpdateCause`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataMapsUpdateCause {
    /// `SERVER_RELOAD`: the server merged its data maps, on a world's first open or after
    /// `/reload`.
    ServerReload,
    /// `CLIENT_SYNC`: a client applied the data maps its server sent.
    ClientSync,
}

/// How many mobs of each spawning category a level's natural spawning allows
/// (`MobCategory.getMaxInstancesPerChunk`), read and set through
/// `LevelRef::spawn_limits` and `set_spawn_limits`. Each limit counts twice, as
/// vanilla's own does: the level spawns a category only while its count of
/// that category's mobs stays under `limit * spawnable chunks / 289` (one
/// player's 17 by 17 chunks hold the vanilla value), and only in a chunk with
/// a player near it who has fewer than `limit` of them around. A limit of 0
/// stops the category's natural spawning. `MobCategory::Misc` never spawns
/// naturally, so it has no limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SpawnLimits {
    /// `MONSTER`: 70 in vanilla.
    pub monster: u32,
    /// `CREATURE`: 10 in vanilla.
    pub creature: u32,
    /// `AMBIENT`: 15 in vanilla.
    pub ambient: u32,
    /// `AXOLOTLS`: 5 in vanilla.
    pub axolotls: u32,
    /// `UNDERGROUND_WATER_CREATURE`: 5 in vanilla.
    pub underground_water_creature: u32,
    /// `WATER_CREATURE`: 5 in vanilla.
    pub water_creature: u32,
    /// `WATER_AMBIENT`: 20 in vanilla.
    pub water_ambient: u32,
}

impl SpawnLimits {
    /// Vanilla's limits, which every level starts with.
    pub const VANILLA: Self = Self {
        monster: 70,
        creature: 10,
        ambient: 15,
        axolotls: 5,
        underground_water_creature: 5,
        water_creature: 5,
        water_ambient: 20,
    };
}

impl Default for SpawnLimits {
    fn default() -> Self {
        Self::VANILLA
    }
}

/// Which categories a level's natural spawning (`NaturalSpawner.spawnForChunk`)
/// may spawn at all, read and set through `LevelRef::spawn_flags` and
/// `set_spawn_flags`. A flag only takes away, for its level alone: monsters
/// still need the `spawn_monsters` game rule, and nothing spawns naturally with
/// `spawn_mobs` off. Spawners, patrols, sieges, phantoms, a new chunk's
/// creatures and other custom spawners don't read the flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SpawnFlags {
    /// Whether `MobCategory::Monster` spawns: off acts as the `spawn_monsters`
    /// rule off does (`getFilteredSpawningCategories`' `spawnEnemies`).
    pub monsters: bool,
    /// Whether every other spawning category spawns: creatures, ambient and
    /// water mobs, axolotls. Horizon's own; vanilla has no such switch.
    pub friendlies: bool,
}

impl Default for SpawnFlags {
    /// Both on, as every level starts.
    fn default() -> Self {
        Self {
            monsters: true,
            friendlies: true,
        }
    }
}

/// Why a mob spawns (`EntitySpawnReason`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnReason {
    /// `NATURAL`: the level's own spawning each tick.
    Natural,
    /// `CHUNK_GENERATION`: a new chunk's creature packs.
    ChunkGeneration,
    /// `SPAWNER`: a mob spawner block.
    Spawner,
    /// `TRIAL_SPAWNER`: a trial spawner.
    TrialSpawner,
    /// `REINFORCEMENT`: a zombie calling for help.
    Reinforcement,
    /// `PATROL`: a pillager patrol.
    Patrol,
    /// `STRUCTURE`: a structure's own mobs.
    Structure,
    /// `EVENT`: a world event, such as a village siege.
    Event,
    /// `COMMAND`: `/summon`.
    Command,
    /// `SPAWN_ITEM_USE`: a spawn egg.
    SpawnItemUse,
    /// `MOB_SUMMONED`: a mob that summons another (an evoker's vexes).
    MobSummoned,
    /// `CONVERSION`: a mob that became another.
    Conversion,
    /// `BREEDING`: a baby.
    Breeding,
    /// `TRIGGERED`: a block or item set it off (a sculk shrieker's warden).
    Triggered,
    /// `JOCKEY`: a rider spawned on a mount.
    Jockey,
    /// `DISPENSER`: a dispenser fired a spawn egg.
    Dispenser,
    /// `BUCKET`: a bucketed mob released.
    Bucket,
    /// `LOAD`: loaded from saved data.
    Load,
    /// `DIMENSION_TRAVEL`: a mob that crossed a portal.
    DimensionTravel,
}

/// The local difficulty at a spawn (`DifficultyInstance`): what `finalize_spawn` reads and
/// may change before the mob is finalized.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Difficulty {
    /// `Difficulty.getId`: 0 peaceful, 1 easy, 2 normal, 3 hard. A larger value is taken as 3.
    pub base: u8,
    /// `DifficultyInstance.getEffectiveDifficulty`: the base scaled by world age, the chunk's
    /// inhabited time and the moon.
    pub effective: f32,
}

impl Difficulty {
    /// A difficulty of `base` and `effective`.
    pub fn new(base: u8, effective: f32) -> Self {
        Self { base, effective }
    }
    /// `DifficultyInstance.getSpecialMultiplier`: 0 below an effective difficulty of 2, 1
    /// above 4, and rising in a straight line between them. Mobs read it for their chance to
    /// pick up loot, a zombie's reinforcements and a skeleton's enchanted bow.
    pub fn special_multiplier(&self) -> f32 {
        if self.effective < 2.0 {
            0.0
        } else if self.effective > 4.0 {
            1.0
        } else {
            (self.effective - 2.0) / 2.0
        }
    }
}

/// What spawned a mob, where `EntitySpawnReason` alone does not say (`FinalizeSpawnEvent.getSpawner`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnSource {
    /// A spawner or trial spawner block at this position.
    Block(BlockPos),
}

/// The spawner block that asked for a spawn check (`MobSpawnEvent.PositionCheck.getSpawner`):
/// what `mob_spawn_placement_check` and `mob_spawn_position_check` get when a block spawns the
/// mob. Natural spawning, chunk generation, patrols, sieges, reinforcements and a spawner
/// minecart have none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spawner {
    /// A monster spawner block (`SpawnerBlockEntity`).
    Block {
        /// Where the block is.
        pos: BlockPos,
    },
    /// A trial spawner block (`TrialSpawnerBlockEntity`).
    Trial {
        /// Where the block is.
        pos: BlockPos,
    },
}

/// What a mod answers a spawn check (`MobSpawnEvent.PositionCheck.Result` and
/// `MobSpawnEvent.SpawnPlacementCheck.Result`, one type for both).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpawnCheck {
    /// `SUCCEED`: the check passes.
    Succeed,
    /// `DEFAULT`: vanilla's own answer stands.
    #[default]
    Default,
    /// `FAIL`: the check fails.
    Fail,
}

/// Where a mod entity type stands to spawn (`SpawnPlacementType`):
/// `SpawnPlacements.isSpawnPositionOk`'s first check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnPlacementType {
    /// `ON_GROUND`: a block that is valid to spawn on below, free room at the feet and the head.
    OnGround,
    /// `IN_WATER`: a water block, with no redstone conductor above.
    InWater,
    /// `IN_LAVA`: a lava block.
    InLava,
    /// `NO_RESTRICTIONS`: anywhere.
    NoRestrictions,
}

/// How bright a spot a mod entity type needs to spawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnLight {
    /// Any light level.
    Any,
    /// Dark enough for a monster (`Monster.isDarkEnoughToSpawn`: sky light, block light
    /// and the dimension's monster light test, which draw from the level's random).
    Dark,
    /// Bright enough for an animal (`Animal.isBrightEnoughToSpawn`: light above 8).
    Bright,
}

/// A mod entity type's spawn rule, declared with
/// [`Registrar::declare_spawn_placement`](SpawnPlacementsRegistrar::declare_spawn_placement):
/// the arguments of NeoForge's `RegisterSpawnPlacementsEvent.register` as data, so one
/// rule serves native and wasm mods. Every field must hold for a spawn to pass; a trial
/// spawner ignores `light`, as for vanilla types.
#[derive(Clone, Debug, PartialEq)]
pub struct SpawnPlacementRule {
    /// Where the type stands (`SpawnPlacementType`).
    pub placement: SpawnPlacementType,
    /// The heightmap natural spawning and chunk generation find the surface with.
    pub heightmap: Heightmap,
    /// A block tag (`minecraft:animals_spawnable_on`, no `#`) the block below must be in.
    pub spawnable_on: Option<String>,
    /// How bright the spot must be.
    pub light: SpawnLight,
    /// A biome tag (no `#`) the spot's biome must be in.
    pub biome_tag: Option<String>,
}

/// The player responsible for a game rule change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameRuleActor<'a> {
    /// Player UUID.
    pub uuid: u128,
    /// Server entity ID.
    pub entity_id: i32,
    /// Player name.
    pub name: &'a str,
}

/// A block state's sounds (`BlockState.getSoundType()`, vanilla's `SoundType`): the sound
/// events, as `LevelRef::play_sound` names them, and the volume and pitch they play at.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockSoundType {
    /// `SoundType.getVolume`.
    pub volume: f32,
    /// `SoundType.getPitch`.
    pub pitch: f32,
    /// `getBreakSound`: the block broken.
    pub break_sound: std::sync::Arc<str>,
    /// `getStepSound`: walked on.
    pub step: std::sync::Arc<str>,
    /// `getPlaceSound`: placed.
    pub place: std::sync::Arc<str>,
    /// `getHitSound`: hit while being mined.
    pub hit: std::sync::Arc<str>,
    /// `getFallSound`: landed on.
    pub fall: std::sync::Arc<str>,
}

/// Why a block's sound type is read (`BlockPart::sound_type`): which of its sounds the game is
/// about to play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockSoundCause {
    /// An entity steps on it (`Entity.playStepSound`): the step sound at volume × 0.15.
    Step,
    /// A living entity lands on it hard enough to be hurt (`LivingEntity.playBlockFallSound`):
    /// the fall sound at volume × 0.5, pitch × 0.75.
    Fall,
    /// It is placed (`BlockItem.place`): the place sound at `(volume + 1) / 2`, pitch × 0.8.
    Place,
    /// It is broken (level event 2001): the break sound at `(volume + 1) / 2`, pitch × 0.8.
    Break,
    /// A sniffer digs it (`Sniffer.emitDiggingParticles`): the hit sound at a fixed volume 0.5
    /// and pitch 0.5, whatever the answer's volume and pitch.
    Dig,
}

/// NeoForge's `TriState`: force a part of an action off or on, or leave it
/// to vanilla.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TriState {
    /// Deny.
    False,
    #[default]
    /// Leave the game's own answer.
    Default,
    /// Allow.
    True,
}

impl TriState {
    /// Whether the part runs, `vanilla` deciding for `Default`.
    pub fn or(self, vanilla: bool) -> bool {
        match self {
            Self::False => false,
            Self::Default => vanilla,
            Self::True => true,
        }
    }
}
