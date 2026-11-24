//! The table of species and the data of a type.
//! A cow as data components and batch systems (palier 1): the stroll goal (a geometric wait with the law of
//! `nextInt(60) == 0` each tick, then `LandRandomPos` 10 x 7), the move control, `LivingEntity.travel` on the
//! ground with Horizon's real block collision sweep (`SceneCollisions::resolve_in_border_with`), and sleeping.
//! Species data (size, step, speed, stroll range) are component values, not branches.
#[allow(unused_imports)]
use crate::IFloor as _;
#[allow(unused_imports)]
use bevy_ecs::{
    entity_disabling::Disabled,
    prelude::*,
    query::Allow,
    schedule::{Schedule, ScheduleLabel, SingleThreadedExecutor},
};
#[allow(unused_imports)]
use myriade_engine_lib::mobs::{
    MobWorld,
    nav::Navigation,
    path::{Evaluator, Finder, Path, PathMob, PATH_TYPES},
};
#[allow(unused_imports)]
use myriade_level::{chunk::Chunk, collisions::SceneCollisions};
#[allow(unused_imports)]
use horizon_core::geometry::{Aabb, Vec3, trig};
#[allow(unused_imports)]
use std::sync::Arc;
use super::*;

#[derive(Resource, Clone, Copy)]
pub struct Config {
    pub sleep: bool,
    pub parallel: bool,
    /// The pushes of the animals that overlap another run in the `Mob`'s order (`ordered_pass`): see `PushMode`.
    pub ordered_push: bool,
    /// Batch mode: an animal that overlaps another stays in `push` every tick even if it does not move (`vanilla` runs
    /// `pushEntities` for every entity, at rest or not).
    pub keep_touching: bool,
    /// The most path searches `pathfind` runs in a tick (0: no limit). The others keep `NeedsPath` and are served the next tick, in the table order
    /// (first asked, first served, in the main); a guard against the bursts (suivi/ecs-echelle.md, H): a zombie that waits does not move.
    pub path_budget: usize,
    /// EXPERIMENT (calibration of the batch push): the form (0: all impulses x k; 1: only the impulse from a lower index x k)
    /// and the factor k, from the environment (PUSH_FORM, PUSH_K).
    pub push_form: u8,
    pub push_k: f64,
}


/// The species' navigation data, read once from a real `Mob` of the type.
/// The species of an entity: an index into the `SpeciesTable`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(on_add = census_added, on_remove = census_removed)]
pub struct SpeciesId(pub u8);

/// How many entities of each species are alive (by `SpeciesId`): kept by the hooks, read by the goals that scan for a species.
/// It lives in `Carriers` (a resource of its own would be one more entity, and shift the bench's picks).
#[derive(Default)]
pub struct Census(pub Vec<u32>, pub NameIndex);

/// The live entities of each `TypeName`, kept by the hooks of `TypeName` (spawn, despawn, replacement): `select` by species reads its set
/// and never scans the world. Unordered; the readers sort what they return.
#[derive(Default)]
pub struct NameIndex(pub std::collections::HashMap<&'static str, bevy_ecs::entity::EntityHashSet>);

impl NameIndex {
    pub fn of(&self, name: &str) -> Option<&bevy_ecs::entity::EntityHashSet> {
        self.0.get(name)
    }
}

pub(crate) fn census_added(mut w: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let id = w.get::<SpeciesId>(ctx.entity).unwrap().0 as usize;
    let first = w.get_resource_mut::<crate::goals::Carriers>().is_some_and(|mut c| {
        let c = &mut c.1;
        if c.0.len() <= id {
            c.0.resize(id + 1, 0);
        }
        c.0[id] += 1;
        c.0[id] == 1
    });
    if first {
        w.commands().queue(move |w: &mut World| crate::behaviours::nearest_target::unpark(w, id as u8));
    }
}

pub(crate) fn census_removed(mut w: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let id = w.get::<SpeciesId>(ctx.entity).unwrap().0 as usize;
    if let Some(mut c) = w.get_resource_mut::<crate::goals::Carriers>() {
        let c = &mut c.1;
        if let Some(n) = c.0.get_mut(id) {
            *n = n.saturating_sub(1);
        }
    }
}

/// A set of species by `SpeciesId`: 256 bits, one per value of the id (no species is left out of a set).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpeciesSet(pub(crate) [u64; 4]);

impl SpeciesSet {
    pub const EMPTY: Self = Self([0; 4]);
    pub fn with(mut self, id: usize) -> Self {
        self.0[id >> 6 & 3] |= 1 << (id & 63);
        self
    }
    pub fn has(&self, id: u8) -> bool {
        self.0[usize::from(id) >> 6] >> (id & 63) & 1 == 1
    }
    pub fn is_empty(&self) -> bool {
        self.0 == [0; 4]
    }
}

/// The data of every species of the world, by `SpeciesId`: what a system needs of a species it reads here, per entity.
#[derive(Resource, Clone)]
pub struct SpeciesTable(pub Vec<Species>);

#[derive(Clone, Copy)]
pub struct Species {
    pub id: u8,
    pub malus: [Option<f32>; PATH_TYPES],
    pub can_float: bool,
    pub can_open_doors: bool,
    pub can_pass_doors: bool,
    pub can_walk_over_fences: bool,
    pub follow_range: f32,
    pub max_health: f32,
    pub water_slowdown: f32,
    pub lava_walker: bool,
    pub water_efficiency: f32,
    pub eye_height: f64,
    /// `Entity.getSoundSource` of the type (`Def::sound_source`).
    pub sound_source: crate::species::SoundCategory,
    /// The type (`minecraft:cow`...), whose size, speed and goals below are data of the species.
    pub name: &'static str,
    /// Whether the type is saved with its world (`Def::saved`): an entity of an unsaved species has no NBT.
    pub saved: bool,
    pub width: f32,
    pub height: f32,
    pub step_height: f32,
    /// The gravity of the type (`Gravity`), absent when the type is not subject to it.
    pub gravity: Option<f64>,
    pub speed: f64,
    /// `Attributes.FLYING_SPEED` and the `air_speed` of the type (`LivingEntity.getFlyingSpeed`).
    pub flying_speed: f64,
    pub air_speed: f32,
    /// `EntityType` flag: the vertical drag is the horizontal one (0.91) rather than 0.98 (a flier of the air).
    pub omnidirectional: bool,
    pub baby: (f32, f32),
    pub goals: crate::species::Config,
    /// The farm behaviours of the species (`farm.rs`).
    pub rules: &'static [crate::rules::RuleEntry],
    /// `Attributes.KNOCKBACK_RESISTANCE`.
    pub knockback_resistance: f64,
    /// `Attributes.ATTACK_KNOCKBACK` (`Mob.doHurtTarget`'s extra push, half of it).
    pub attack_knockback: f64,
    /// The base armor and toughness of the type (`Attributes.ARMOR`, `ARMOR_TOUGHNESS`): worn pieces add to it.
    pub armor: (f32, f32),
    /// `Attributes.ATTACK_DAMAGE` (0 for a species that does not attack).
    pub attack_damage: f32,
    /// `EntityType` flag: the undead that burn in daylight.
    /// What the species prefers to walk to (`getWalkTargetValue`).
    pub walk: crate::spots::WalkValue,
    /// The capabilities of the configuration (its sections): the entity is made of these only.
    pub caps: &'static [crate::behaviours::Cap],
    /// The brain of the species, resolved (None: it has none).
    pub brain: Option<&'static crate::brain::Plan>,
    /// The modes of locomotion (`locomotion.rs`).
    pub modes: &'static [crate::locomotion::ModeEntry],
    /// The class bits of the type (`tag::*`): what its save writes at the game's default (`nbt_fields::common::TAGGED_DEFAULTS`).
    pub tags: u32,
    /// `Entity.getMaxAirSupply` of the type (300; the axolotl and the dolphin more).
    pub max_air: i32,
    /// The type is persistent from its constructor (`setPersistenceRequired`: the elder guardian, the copper golem).
    pub persistent: bool,
    /// The chance of a screaming goat at birth (`Def::screaming`): a draw only when it is there.
    pub screaming: Option<f64>,
    /// The instances the species gets at load (`attr_mod::apply_load`): none for most.
    pub load_attrs: &'static [crate::attr_mod::LoadAttr],
    /// The chance of a horn lost at a natural spawn (`Def::lost_horn`).
    pub lost_horn: Option<f64>,
    /// `Def::max_scale`: the cap `model_scale` applies (`LivingEntity.sanitizeScale`).
    pub max_scale: f32,
    /// `Def::llama_strength`: the strength is drawn at the pose (`spawn_declared`).
    pub llama_strength: bool,
    /// `Def::random_max_health`: the max health is drawn at the birth (`spawn_declared`).
    pub random_max_health: bool,
    /// `Def::horse_gen`: the speed and jump strength drawn at the birth (`spawn_declared`).
    pub horse_gen: crate::species::HorseGen,
    /// `Def::body_wear`: the body piece wears when hurt (`BodyWear`, posed at the birth).
    pub body_wear: bool,
    /// `Def::not_in_peaceful`.
    pub not_in_peaceful: bool,
}

/// The class bits of a type, as data of its row (no per-entity byte): the groups of species whose class saves a state at its default.
pub mod tag {
    /// `Animal`: saves `InLove`.
    pub const ANIMAL: u32 = 1;
    /// `AgeableMob` that can be a baby (and the tadpole): saves `Age` and `AgeLocked`.
    pub const AGEING: u32 = 1 << 1;
    /// `AgeableMob` but the tadpole: saves `ForcedAge`.
    pub const FORCED_AGE: u32 = 1 << 2;
    /// `AbstractHorse` and the camels: save `Tame`, `Bred`, `EatingHaystack`.
    pub const EQUINE: u32 = 1 << 3;
    /// `AbstractChestedHorse` and llamas: save `ChestedHorse`.
    pub const CHESTED: u32 = 1 << 4;
    /// `Zombie` and its subclasses: save `CanBreakDoors` and the conversion tracker.
    pub const ZOMBIE: u32 = 1 << 5;
    /// `Raider` (`EntityTypeTags.RAIDERS`): saves `CanJoinRaid`, `PatrolLeader`, `Patrolling`, `Wave`.
    pub const RAIDER: u32 = 1 << 6;
    /// A zombie that can be a baby (saves `IsBaby`).
    pub const BABY_FLAG: u32 = 1 << 7;
    /// `Bucketable` fish and the axolotl: save `FromBucket`.
    pub const BUCKETABLE: u32 = 1 << 8;
    /// The camels (`Camel.isTamed` is always true): their `Tame` default is 1.
    pub const CAMEL: u32 = 1 << 9;
    /// The tadpole (`fromBucket` is always true).
    pub const TADPOLE: u32 = 1 << 10;
    /// Its constructor sets `canPickUpLoot` (the fox, the panda, the dolphin, the villager, the piglins).
    pub const PICKS_UP: u32 = 1 << 11;
    /// `NeutralMob` whose `persistentAngerEndTime` is a plain field (default 0): saves `anger_end_time`.
    pub const ANGER_ZERO: u32 = 1 << 12;
    /// `NeutralMob` whose end time is synched data (the bee, the wolf): default -1.
    pub const ANGER_UNSET: u32 = 1 << 13;
    /// `Goat`: saves `IsScreamingGoat` (`false` unless the screaming marker is there).
    pub const SCREAMER: u32 = 1 << 14;
}


/// `getWalkTargetValue` of the template mob, from its type (the species the prototype ports; the others read zero).
pub(crate) fn walk_value_of(kind: &myriade_engine_lib::mobs::MobType) -> crate::spots::WalkValue {
    use crate::spots::WalkValue as W;
    use myriade_engine_lib::mobs::Species as S;
    match kind.species {
        S::Mooshroom => W::Mooshroom,
        S::Bee => W::Bee,
        S::Turtle => W::Turtle,
        S::Guardian | S::ElderGuardian => W::Guardian,
        S::HappyGhast => W::HappyGhast,
        S::Hoglin => W::Hoglin,
        S::Strider => W::Strider,
        S::Silverfish => W::Silverfish,
        S::Enderman | S::Pillager | S::Warden | S::Axolotl | S::Nautilus | S::ZombieNautilus | S::Creaking => W::Zero,
        S::Slime | S::MagmaCube | S::Ghast | S::Phantom => W::Zero,
        _ if kind.category == "monster" => W::Monster,
        _ if kind.flags.animal => W::Animal,
        _ => W::Zero,
    }
}

/// The path maluses of a fresh mob of the type: `constructor_maluses` (the vanilla base classes), then the species' own
/// `register`, in that order. The writes that depend on a live mob (a state, a position) are left out.
pub(crate) fn malus_of(kind: &myriade_engine_lib::mobs::MobType) -> [Option<f32>; myriade_engine_lib::mobs::path::PATH_TYPES] {
    use myriade_engine_lib::mobs::{path::PathType as P, Species as S};
    let s = kind.species;
    let animal = matches!(
        s,
        S::Armadillo | S::Axolotl | S::Bee | S::Camel | S::CamelHusk | S::Cat | S::Chicken | S::Cow | S::Donkey | S::Fox | S::Frog
            | S::Goat | S::HappyGhast | S::Hoglin | S::Horse | S::Llama | S::Mule | S::Mooshroom | S::Nautilus | S::Ocelot
            | S::Panda | S::Parrot | S::Pig | S::PolarBear | S::Rabbit | S::Sheep | S::SkeletonHorse | S::Sniffer | S::Strider
            | S::TraderLlama | S::Turtle | S::Wolf | S::ZombieHorse | S::ZombieNautilus
    );
    let mut w: Vec<(P, f32)> = Vec::new();
    if animal || matches!(s, S::Villager | S::WanderingTrader | S::Piglin | S::PiglinBrute | S::CopperGolem) {
        w.extend([(P::FireInNeighbor, 16.0), (P::Fire, -1.0)]);
    }
    w.extend(match s {
        S::Fox => vec![(P::DamagingInNeighbor, 0.0), (P::Damaging, 0.0)],
        S::Parrot => vec![(P::FireInNeighbor, -1.0), (P::Fire, -1.0), (P::Cocoa, -1.0)],
        S::Frog => vec![(P::Water, 4.0), (P::Trapdoor, -1.0)],
        S::Goat | S::Wolf => vec![(P::PowderSnow, -1.0), (P::OnTopOfPowderSnow, -1.0)],
        S::CopperGolem => vec![(P::DamagingInNeighbor, 16.0)],
        S::Blaze => vec![(P::Water, -1.0), (P::Lava, 8.0), (P::FireInNeighbor, 0.0), (P::Fire, 0.0)],
        S::ZombifiedPiglin | S::WitherSkeleton => vec![(P::Lava, 8.0)],
        // The species' own `register`: water costs nothing to the swimmers, and the others' fixed writes.
        S::Cod | S::Salmon | S::TropicalFish | S::Pufferfish | S::Tadpole | S::Squid | S::GlowSquid | S::Dolphin | S::Axolotl
        | S::Chicken | S::Guardian | S::ElderGuardian | S::Nautilus | S::ZombieNautilus | S::Turtle | S::Drowned => {
            vec![(P::Water, 0.0)]
        }
        S::Enderman => vec![(P::Water, -1.0)],
        S::Bee => vec![(P::Fire, -1.0), (P::Water, -1.0), (P::WaterBorder, 16.0), (P::Cocoa, -1.0), (P::Fence, -1.0)],
        S::Breeze => vec![(P::OnTopOfTrapdoor, -1.0), (P::Fire, -1.0)],
        S::Ravager => vec![(P::Leaves, 0.0)],
        S::Sniffer => vec![(P::Water, -1.0), (P::OnTopOfPowderSnow, -1.0), (P::DamageCautious, -1.0)],
        S::Strider => vec![(P::Water, -1.0), (P::Lava, 0.0), (P::FireInNeighbor, 0.0), (P::Fire, 0.0)],
        S::Warden => vec![
            (P::UnpassableRail, 0.0),
            (P::Damaging, 8.0),
            (P::PowderSnow, 8.0),
            (P::Lava, 8.0),
            (P::Fire, 0.0),
            (P::FireInNeighbor, 0.0),
        ],
        _ => vec![],
    });
    if matches!(s, S::Turtle) {
        w.extend([(P::DoorIronClosed, -1.0), (P::DoorWoodClosed, -1.0), (P::DoorOpen, -1.0)]);
    }
    let mut m = [None; myriade_engine_lib::mobs::path::PATH_TYPES];
    for (t, v) in w {
        m[t as usize] = Some(v);
    }
    m
}

/// Whether a fresh mob of the type can float (`nav.can_float`): the `FloatGoal` species and the final list of `ai::register`.
pub(crate) fn can_float_of(kind: &myriade_engine_lib::mobs::MobType) -> bool {
    use myriade_engine_lib::mobs::Species as S;
    matches!(
        kind.species,
        S::Slime
            | S::MagmaCube
            | S::SulfurCube
            | S::Bee
            | S::Creaking
            | S::HappyGhast
            | S::Rabbit
            | S::Sniffer
            | S::Villager
            | S::Warden
            | S::Parrot
            | S::Allay
            | S::Armadillo
            | S::Camel
            | S::Goat
            | S::Wither
    )
}

/// Whether a fresh mob of the type opens doors (`nav.can_open_doors`): the villager and piglin families (the zombies' is state).
pub(crate) fn can_open_doors_of(kind: &myriade_engine_lib::mobs::MobType) -> bool {
    use myriade_engine_lib::mobs::Species as S;
    matches!(kind.species, S::CopperGolem | S::Villager | S::Piglin | S::PiglinBrute)
}

/// The flying speed of a fresh mob of the type: its attribute default, else the registry's 0.4 (`finalize::value`).
pub(crate) fn flying_speed_of(kind: &myriade_engine_lib::mobs::MobType) -> f64 {
    kind.attributes.get("minecraft:flying_speed").copied().unwrap_or(0.4)
}

/// The air a mob of the type holds at most (`Mob::max_air_supply`): the axolotl's and the dolphin's, else 300.
pub(crate) fn max_air_of(kind: &myriade_engine_lib::mobs::MobType) -> i32 {
    use myriade_engine_lib::mobs::Species as S;
    match kind.species {
        S::Axolotl => myriade_engine_lib::mobs::axolotl::MAX_AIR,
        S::Dolphin => 4800,
        _ => 300,
    }
}

/// Whether a fresh mob of the type picks up loot (`Mob::can_pick_up_loot`, no item in hand, so the allay's is false).
pub(crate) fn can_pick_up_of(kind: &myriade_engine_lib::mobs::MobType) -> bool {
    use myriade_engine_lib::mobs::Species as S;
    matches!(kind.species, S::SulfurCube | S::Villager | S::Piglin | S::PiglinBrute | S::Dolphin | S::Fox | S::Panda)
}

/// The class bits of a species, from its type (the fresh mob's pickup and anger are the type's).
pub(crate) fn tags_of(kind: &myriade_engine_lib::mobs::MobType) -> u32 {
    use myriade_engine_lib::mobs::{finalize, species::can_be_a_baby, Species as S};
    let s = kind.species;
    let mut t = 0;
    // `flags.animal` also marks the allay and the giant, which do not extend `Animal`.
    if (kind.flags.animal && !matches!(s, S::Allay | S::Giant)) || s == S::Hoglin {
        t |= tag::ANIMAL;
    }
    if s == S::Tadpole || (finalize::ageable(s) && can_be_a_baby(s)) {
        t |= tag::AGEING;
        if s != S::Tadpole {
            t |= tag::FORCED_AGE;
        }
    }
    if matches!(s, S::Horse | S::Donkey | S::Mule | S::SkeletonHorse | S::ZombieHorse | S::Camel | S::CamelHusk | S::Llama | S::TraderLlama) {
        t |= tag::EQUINE;
    }
    if can_pick_up_of(kind) {
        t |= tag::PICKS_UP;
    }
    if matches!(s, S::Bee | S::Wolf) {
        t |= tag::ANGER_UNSET;
    } else if myriade_engine_lib::mobs::neutral_anger::persistent_anger(s) {
        t |= tag::ANGER_ZERO;
    }
    if s == S::Tadpole {
        t |= tag::TADPOLE;
    }
    if matches!(s, S::Camel | S::CamelHusk) {
        t |= tag::CAMEL;
    }
    if matches!(s, S::Donkey | S::Mule | S::Llama | S::TraderLlama) {
        t |= tag::CHESTED;
    }
    if matches!(s, S::Zombie | S::Husk | S::Drowned | S::ZombieVillager | S::ZombifiedPiglin) {
        t |= tag::ZOMBIE;
    }
    if matches!(s, S::Pillager | S::Vindicator | S::Evoker | S::Illusioner | S::Witch | S::Ravager) {
        t |= tag::RAIDER;
    }
    if matches!(s, S::Zombie | S::Husk | S::Drowned | S::ZombieVillager | S::ZombifiedPiglin | S::Piglin | S::Zoglin) {
        t |= tag::BABY_FLAG;
    }
    if matches!(s, S::Cod | S::Salmon | S::Pufferfish | S::TropicalFish | S::Axolotl) {
        t |= tag::BUCKETABLE;
    }
    t
}

/// The class bits of the species of an entity (0 when no level was built).
pub fn tags_of_id(id: SpeciesId) -> u32 {
    TABLE.get().and_then(|t| t.get(usize::from(id.0))).map_or(0, |s| s.tags)
}

/// The type of the entity (`minecraft:cow`...), for the data keyed by type of the host (breathing).
/// Every entity has one when it is made, so its join is taken there, unless it has one already or hangs (`hanging::JoinedAt` is taken then).
#[derive(Component, Clone, Copy, Debug)]
#[component(on_add = joined, on_remove = unnamed)]
pub struct TypeName(pub &'static str);

fn unnamed(mut w: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let name = w.get::<TypeName>(ctx.entity).map(|t| t.0);
    if let (Some(name), Some(mut c)) = (name, w.get_resource_mut::<crate::goals::Carriers>())
        && let Some(set) = c.1.1.0.get_mut(name)
    {
        set.remove(&ctx.entity);
        if set.is_empty() {
            c.1.1.0.remove(name);
        }
    }
}

fn joined(mut w: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let name = w.get::<TypeName>(ctx.entity).map(|t| t.0);
    if let (Some(name), Some(mut c)) = (name, w.get_resource_mut::<crate::goals::Carriers>()) {
        c.1.1.0.entry(name).or_default().insert(ctx.entity);
    }
    if w.get::<crate::hanging::JoinedAt>(ctx.entity).is_none() && w.get::<crate::hanging::Attached>(ctx.entity).is_none() {
        let at = crate::hanging::JoinedAt(myriade_engine_lib::join_seq::JoinSeq::next());
        w.commands().entity(ctx.entity).insert(at);
    }
}

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct CowTick;

/// The prepared vanilla types of the process, for the species the tests compare with a `Mob`.
pub(crate) static TYPES: std::sync::OnceLock<&'static myriade_engine_lib::mobs::MobTypes> = std::sync::OnceLock::new();

pub fn mob_kind(name: &str) -> Arc<myriade_engine_lib::mobs::MobType> {
    Arc::new(TYPES.get().expect("a level was built").types.get(name).unwrap_or_else(|| panic!("no MobType {name}")).clone())
}

/// Whether the identifier has a `MobType` (a creature Horizon simulates as a `Mob`).
pub fn has_mob_kind(name: &str) -> bool {
    TYPES.get().is_some_and(|t| t.types.contains_key(name))
}

/// Whether the species is a monster (`MobType.category`); an entity that is not a `Mob` has no type and is not one.
pub fn is_monster(name: &str) -> bool {
    TYPES.get().expect("a level was built").types.get(name).is_some_and(|k| k.category == "monster")
}

/// Every species of the process (see `build_level`).
pub(crate) static TABLE: std::sync::OnceLock<Vec<Species>> = std::sync::OnceLock::new();

pub fn species_table() -> SpeciesTable {
    SpeciesTable(TABLE.get().expect("a level was built").clone())
}

/// `HORIZON_SPECIES`, the one place the identifier enters the process (default `PROCESS_DEFAULT`). `HORIZON_SPECIES_PROBE=<file>` appends a
/// line to the file at every read (`pass.sh --classify` tells the tests that depend on the identifier from those that do not);
/// `HORIZON_SPECIES_FORBID=1` makes a read panic (the run of the common tests: one that reads the identifier turns red, it is never run unseen).
pub fn species_env() -> String {
    species_var().unwrap_or_else(|_| crate::species::PROCESS_DEFAULT.into())
}

/// `HORIZON_SPECIES` as `std::env::var` gives it (absent = `Err`): EVERY read of the identifier goes through here, the harness's and the tests'.
pub fn species_var() -> Result<String, std::env::VarError> {
    if std::env::var_os("HORIZON_SPECIES_FORBID").is_some() {
        panic!("a test of the common group read HORIZON_SPECIES: it depends on the identifier (pass.sh --classify, then rerun)");
    }
    if let Some(path) = std::env::var_os("HORIZON_SPECIES_PROBE") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all(b"read\n");
        }
    }
    std::env::var("HORIZON_SPECIES")
}

/// The name of the species of the process (`HORIZON_SPECIES`), readable before any level is built (`species()` is not).
pub fn process_species_name() -> String {
    let name = species_env();
    // A name that matches no registered species must stop the harness: skipping every test of an unknown species would be a false green.
    if crate::species::registry().by_name(&name).is_err() {
        panic!("HORIZON_SPECIES={name:?} matches no registered species (names carry the namespace, e.g. minecraft:frog): {:?}", crate::species::names());
    }
    name
}

pub(crate) fn species_size(name: &str) -> (f32, f32) {
    // The size of the type, read once from the real data of the process.
    static TYPES_FOR_SIZE: std::sync::OnceLock<Option<myriade_engine_lib::mobs::MobTypes>> = std::sync::OnceLock::new();
    let t = TYPES_FOR_SIZE.get_or_init(|| myriade_engine_lib::mobs::MobTypes::load(std::path::Path::new(&std::env::var("HORIZON_TEST_RESOURCES").unwrap_or_default())).ok());
    t.as_ref().and_then(|t| t.types.get(name).map(|k| (k.width as f32, k.height as f32))).unwrap_or((0.9, 1.4))
}

/// The species of the process (the first world built sets it: `HORIZON_SPECIES`, `minecraft:cow` by default), for
/// the bundles that are made without a world at hand.
pub(crate) static SPECIES: std::sync::OnceLock<Species> = std::sync::OnceLock::new();

pub fn species() -> &'static Species {
    // Asked for only by the tests that need the species of the process: the level does not read it (`build_level_neutral_with`).
    SPECIES.get_or_init(|| {
        let name = process_species_name();
        *TABLE.get().expect("a level was built").iter().find(|s| s.name == name).expect("a known species")
    })
}
