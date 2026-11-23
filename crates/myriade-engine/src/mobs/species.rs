//! Supported vanilla mob types: model layers, default textures, animation
//! family, sounds, baby dimensions and the per-class constants
//! (`getMaxHeadXRot`, `getAmbientSoundInterval`, `getWaterSlowDown`...).
use super::{Brain, Flags};

impl super::MobType {
    /// The vanilla entity type whose class this type's mobs are: the type
    /// itself, or the template a content mod's living type copies. What a
    /// class saves and syncs of its own (`Zombie.addAdditionalSaveData`, its
    /// entity data) follows it; the saved and sent id stays [`Self::id`].
    pub fn class_id(&self) -> &str {
        if self.id.starts_with("minecraft:") {
            &self.id
        } else {
            vanilla_id(self.species)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Species {
    Tadpole,
    ZombieHorse,
    SkeletonHorse,
    Giant,
    Pig,
    Cow,
    Mooshroom,
    Sheep,
    Chicken,
    Rabbit,
    Horse,
    Donkey,
    Mule,
    Llama,
    TraderLlama,
    Goat,
    Panda,
    PolarBear,
    Fox,
    Wolf,
    Cat,
    Ocelot,
    Parrot,
    Bee,
    Turtle,
    Armadillo,
    Frog,
    Axolotl,
    Squid,
    GlowSquid,
    Cod,
    Salmon,
    TropicalFish,
    Pufferfish,
    Dolphin,
    Bat,
    Villager,
    WanderingTrader,
    IronGolem,
    SnowGolem,
    CopperGolem,
    Zombie,
    Husk,
    Drowned,
    ZombieVillager,
    Skeleton,
    Stray,
    Bogged,
    Parched,
    WitherSkeleton,
    Creeper,
    Spider,
    CaveSpider,
    Enderman,
    Endermite,
    Silverfish,
    Slime,
    MagmaCube,
    Witch,
    Phantom,
    Pillager,
    Vindicator,
    Piglin,
    PiglinBrute,
    ZombifiedPiglin,
    Hoglin,
    Zoglin,
    Ghast,
    Blaze,
    Strider,
    Guardian,
    ElderGuardian,
    Shulker,
    Evoker,
    Illusioner,
    Vex,
    Ravager,
    Wither,
    Camel,
    Allay,
    Breeze,
    Sniffer,
    HappyGhast,
    Warden,
    Creaking,
    CamelHusk,
    Nautilus,
    ZombieNautilus,
    SulfurCube,
    Mannequin,
}
/// Baby bounding boxes (each class's BABY_DIMENSIONS).
#[derive(Clone, Copy, Debug)]
pub enum Baby {
    /// Not an ageable mob.
    None,
    /// EntityDimensions.scalable(w, h).withEyeHeight(e)
    Abs(f64, f64, f64),
    /// The adult dimensions scaled, with an optional explicit eye height.
    Scale(f64, Option<f64>),
}
pub struct Def {
    pub species: Species,
    pub name: &'static str,
    pub sounds: [&'static str; 3],
    pub step_sound: Option<&'static str>,
    pub baby: Baby,
}
macro_rules! def {
    ($s:ident, $name:literal, [$a:literal, $h:literal, $d:literal], $step:expr, $baby:expr) => {
        Def {
            species: Species::$s,
            name: $name,
            sounds: [$a, $h, $d],
            step_sound: $step,
            baby: $baby,
        }
    };
}
pub const ALL: &[Def] = &[
    def!(
        Pig,
        "pig",
        ["entity.pig.ambient", "entity.pig.hurt", "entity.pig.death"],
        Some("entity.pig.step"),
        Baby::Abs(0.45, 0.45, 0.40625)
    ),
    def!(
        Cow,
        "cow",
        ["entity.cow.ambient", "entity.cow.hurt", "entity.cow.death"],
        Some("entity.cow.step"),
        Baby::Abs(0.45, 0.7, 0.69)
    ),
    def!(
        Mooshroom,
        "mooshroom",
        ["entity.cow.ambient", "entity.cow.hurt", "entity.cow.death"],
        Some("entity.cow.step"),
        Baby::Abs(0.45, 0.7, 0.69)
    ),
    def!(
        Sheep,
        "sheep",
        [
            "entity.sheep.ambient",
            "entity.sheep.hurt",
            "entity.sheep.death"
        ],
        Some("entity.sheep.step"),
        Baby::Abs(0.45, 0.65, 0.65625)
    ),
    def!(
        Chicken,
        "chicken",
        [
            "entity.chicken.ambient",
            "entity.chicken.hurt",
            "entity.chicken.death"
        ],
        Some("entity.chicken.step"),
        Baby::Abs(0.3, 0.4, 0.28125)
    ),
    def!(
        Rabbit,
        "rabbit",
        [
            "entity.rabbit.ambient",
            "entity.rabbit.hurt",
            "entity.rabbit.death"
        ],
        None,
        Baby::Abs(0.24, 0.4, 0.39)
    ),
    def!(
        ZombieHorse,
        "zombie_horse",
        [
            "entity.zombie_horse.ambient",
            "entity.zombie_horse.hurt",
            "entity.zombie_horse.death"
        ],
        Some("entity.horse.step"),
        Baby::Scale(0.7, None)
    ),
    def!(
        SkeletonHorse,
        "skeleton_horse",
        [
            "entity.skeleton_horse.ambient",
            "entity.skeleton_horse.hurt",
            "entity.skeleton_horse.death"
        ],
        Some("entity.horse.step"),
        Baby::Scale(0.7, None)
    ),
    def!(
        Giant,
        "giant",
        ["", "entity.generic.hurt", "entity.generic.death"],
        None,
        Baby::None
    ),
    def!(
        Horse,
        "horse",
        [
            "entity.horse.ambient",
            "entity.horse.hurt",
            "entity.horse.death"
        ],
        Some("entity.horse.step"),
        Baby::Scale(0.7, None)
    ),
    def!(
        Donkey,
        "donkey",
        [
            "entity.donkey.ambient",
            "entity.donkey.hurt",
            "entity.donkey.death"
        ],
        Some("entity.horse.step"),
        Baby::Scale(0.5, None)
    ),
    def!(
        Mule,
        "mule",
        [
            "entity.mule.ambient",
            "entity.mule.hurt",
            "entity.mule.death"
        ],
        Some("entity.horse.step"),
        Baby::Scale(0.5, None)
    ),
    def!(
        Llama,
        "llama",
        [
            "entity.llama.ambient",
            "entity.llama.hurt",
            "entity.llama.death"
        ],
        Some("entity.llama.step"),
        Baby::Scale(0.5, None)
    ),
    def!(
        TraderLlama,
        "trader_llama",
        [
            "entity.llama.ambient",
            "entity.llama.hurt",
            "entity.llama.death"
        ],
        Some("entity.llama.step"),
        Baby::Scale(0.5, None)
    ),
    def!(
        Goat,
        "goat",
        [
            "entity.goat.ambient",
            "entity.goat.hurt",
            "entity.goat.death"
        ],
        Some("entity.goat.step"),
        Baby::Abs(0.45, 0.65, 0.59375)
    ),
    def!(
        Panda,
        "panda",
        [
            "entity.panda.ambient",
            "entity.panda.hurt",
            "entity.panda.death"
        ],
        Some("entity.panda.step"),
        Baby::Scale(0.5, Some(0.28125))
    ),
    def!(
        PolarBear,
        "polar_bear",
        [
            "entity.polar_bear.ambient",
            "entity.polar_bear.hurt",
            "entity.polar_bear.death"
        ],
        Some("entity.polar_bear.step"),
        Baby::Abs(0.7, 0.7, 0.34375)
    ),
    def!(
        Fox,
        "fox",
        ["entity.fox.ambient", "entity.fox.hurt", "entity.fox.death"],
        None,
        Baby::Scale(0.6, Some(0.34375))
    ),
    def!(
        Wolf,
        "wolf",
        [
            "entity.wolf.ambient",
            "entity.wolf.hurt",
            "entity.wolf.death"
        ],
        Some("entity.wolf.step"),
        Baby::Abs(0.3, 0.425, 0.34375)
    ),
    def!(
        Cat,
        "cat",
        ["entity.cat.ambient", "entity.cat.hurt", "entity.cat.death"],
        None,
        Baby::Abs(0.3, 0.35, 0.34375)
    ),
    def!(
        Ocelot,
        "ocelot",
        [
            "entity.ocelot.ambient",
            "entity.ocelot.hurt",
            "entity.ocelot.death"
        ],
        None,
        Baby::Abs(0.3, 0.35, 0.34375)
    ),
    def!(
        Parrot,
        "parrot",
        [
            "entity.parrot.ambient",
            "entity.parrot.hurt",
            "entity.parrot.death"
        ],
        Some("entity.parrot.step"),
        Baby::None
    ),
    def!(
        Bee,
        "bee",
        ["", "entity.bee.hurt", "entity.bee.death"],
        None,
        Baby::Scale(0.5, None)
    ),
    def!(
        Turtle,
        "turtle",
        [
            "entity.turtle.ambient_land",
            "entity.turtle.hurt",
            "entity.turtle.death"
        ],
        Some("entity.turtle.shamble"),
        Baby::Scale(0.3, None)
    ),
    def!(
        Armadillo,
        "armadillo",
        [
            "entity.armadillo.ambient",
            "entity.armadillo.hurt",
            "entity.armadillo.death"
        ],
        Some("entity.armadillo.step"),
        Baby::Scale(0.6, Some(0.21875))
    ),
    def!(
        Frog,
        "frog",
        [
            "entity.frog.ambient",
            "entity.frog.hurt",
            "entity.frog.death"
        ],
        Some("entity.frog.step"),
        Baby::None
    ),
    def!(
        Axolotl,
        "axolotl",
        [
            "entity.axolotl.idle_water",
            "entity.axolotl.hurt",
            "entity.axolotl.death"
        ],
        None,
        Baby::Abs(0.375, 0.21, 0.09375)
    ),
    def!(
        Squid,
        "squid",
        [
            "entity.squid.ambient",
            "entity.squid.hurt",
            "entity.squid.death"
        ],
        None,
        Baby::Abs(0.5, 0.5, 0.37)
    ),
    def!(
        GlowSquid,
        "glow_squid",
        [
            "entity.glow_squid.ambient",
            "entity.glow_squid.hurt",
            "entity.glow_squid.death"
        ],
        None,
        Baby::Abs(0.5, 0.5, 0.37)
    ),
    def!(
        Tadpole,
        "tadpole",
        ["", "entity.tadpole.hurt", "entity.tadpole.death"],
        None,
        Baby::None
    ),
    def!(
        Cod,
        "cod",
        ["entity.cod.ambient", "entity.cod.hurt", "entity.cod.death"],
        None,
        Baby::None
    ),
    def!(
        Salmon,
        "salmon",
        [
            "entity.salmon.ambient",
            "entity.salmon.hurt",
            "entity.salmon.death"
        ],
        None,
        Baby::None
    ),
    def!(
        TropicalFish,
        "tropical_fish",
        [
            "entity.tropical_fish.ambient",
            "entity.tropical_fish.hurt",
            "entity.tropical_fish.death"
        ],
        None,
        Baby::None
    ),
    def!(
        Pufferfish,
        "pufferfish",
        ["", "entity.puffer_fish.hurt", "entity.puffer_fish.death"],
        None,
        Baby::None
    ),
    def!(
        Dolphin,
        "dolphin",
        [
            "entity.dolphin.ambient_water",
            "entity.dolphin.hurt",
            "entity.dolphin.death"
        ],
        None,
        Baby::Scale(0.65, Some(0.093_75))
    ),
    def!(
        Bat,
        "bat",
        ["entity.bat.ambient", "entity.bat.hurt", "entity.bat.death"],
        None,
        Baby::None
    ),
    def!(
        Villager,
        "villager",
        [
            "entity.villager.ambient",
            "entity.villager.hurt",
            "entity.villager.death"
        ],
        None,
        Baby::Abs(0.49, 0.98, 0.63)
    ),
    def!(
        WanderingTrader,
        "wandering_trader",
        [
            "entity.wandering_trader.ambient",
            "entity.wandering_trader.hurt",
            "entity.wandering_trader.death"
        ],
        None,
        Baby::None
    ),
    def!(
        IronGolem,
        "iron_golem",
        ["", "entity.iron_golem.hurt", "entity.iron_golem.death"],
        Some("entity.iron_golem.step"),
        Baby::None
    ),
    def!(
        SnowGolem,
        "snow_golem",
        [
            "entity.snow_golem.ambient",
            "entity.snow_golem.hurt",
            "entity.snow_golem.death"
        ],
        None,
        Baby::None
    ),
    def!(
        CopperGolem,
        "copper_golem",
        ["", "entity.copper_golem.hurt", "entity.copper_golem.death"],
        Some("entity.copper_golem.step"),
        Baby::None
    ),
    def!(
        Zombie,
        "zombie",
        [
            "entity.zombie.ambient",
            "entity.zombie.hurt",
            "entity.zombie.death"
        ],
        Some("entity.zombie.step"),
        Baby::Abs(0.49, 0.98, 0.775)
    ),
    def!(
        Husk,
        "husk",
        [
            "entity.husk.ambient",
            "entity.husk.hurt",
            "entity.husk.death"
        ],
        Some("entity.husk.step"),
        Baby::Abs(0.49, 0.98, 0.825)
    ),
    def!(
        Drowned,
        "drowned",
        [
            "entity.drowned.ambient",
            "entity.drowned.hurt",
            "entity.drowned.death"
        ],
        Some("entity.drowned.step"),
        Baby::Abs(0.49, 0.98, 0.775)
    ),
    def!(
        ZombieVillager,
        "zombie_villager",
        [
            "entity.zombie_villager.ambient",
            "entity.zombie_villager.hurt",
            "entity.zombie_villager.death"
        ],
        Some("entity.zombie_villager.step"),
        Baby::Abs(0.49, 0.98, 0.67)
    ),
    def!(
        Skeleton,
        "skeleton",
        [
            "entity.skeleton.ambient",
            "entity.skeleton.hurt",
            "entity.skeleton.death"
        ],
        Some("entity.skeleton.step"),
        Baby::None
    ),
    def!(
        Stray,
        "stray",
        [
            "entity.stray.ambient",
            "entity.stray.hurt",
            "entity.stray.death"
        ],
        Some("entity.stray.step"),
        Baby::None
    ),
    def!(
        Parched,
        "parched",
        [
            "entity.parched.ambient",
            "entity.parched.hurt",
            "entity.parched.death"
        ],
        Some("entity.parched.step"),
        Baby::None
    ),
    def!(
        Bogged,
        "bogged",
        [
            "entity.bogged.ambient",
            "entity.bogged.hurt",
            "entity.bogged.death"
        ],
        Some("entity.bogged.step"),
        Baby::None
    ),
    def!(
        WitherSkeleton,
        "wither_skeleton",
        [
            "entity.wither_skeleton.ambient",
            "entity.wither_skeleton.hurt",
            "entity.wither_skeleton.death"
        ],
        Some("entity.wither_skeleton.step"),
        Baby::None
    ),
    def!(
        Creeper,
        "creeper",
        ["", "entity.creeper.hurt", "entity.creeper.death"],
        None,
        Baby::None
    ),
    def!(
        Spider,
        "spider",
        [
            "entity.spider.ambient",
            "entity.spider.hurt",
            "entity.spider.death"
        ],
        Some("entity.spider.step"),
        Baby::None
    ),
    def!(
        CaveSpider,
        "cave_spider",
        [
            "entity.spider.ambient",
            "entity.spider.hurt",
            "entity.spider.death"
        ],
        Some("entity.spider.step"),
        Baby::None
    ),
    def!(
        Enderman,
        "enderman",
        [
            "entity.enderman.ambient",
            "entity.enderman.hurt",
            "entity.enderman.death"
        ],
        None,
        Baby::None
    ),
    def!(
        Endermite,
        "endermite",
        [
            "entity.endermite.ambient",
            "entity.endermite.hurt",
            "entity.endermite.death"
        ],
        Some("entity.endermite.step"),
        Baby::None
    ),
    def!(
        Silverfish,
        "silverfish",
        [
            "entity.silverfish.ambient",
            "entity.silverfish.hurt",
            "entity.silverfish.death"
        ],
        Some("entity.silverfish.step"),
        Baby::None
    ),
    def!(
        Slime,
        "slime",
        ["", "entity.slime.hurt", "entity.slime.death"],
        None,
        Baby::None
    ),
    def!(
        MagmaCube,
        "magma_cube",
        ["", "entity.magma_cube.hurt", "entity.magma_cube.death"],
        None,
        Baby::None
    ),
    def!(
        Witch,
        "witch",
        [
            "entity.witch.ambient",
            "entity.witch.hurt",
            "entity.witch.death"
        ],
        None,
        Baby::None
    ),
    def!(
        Phantom,
        "phantom",
        [
            "entity.phantom.ambient",
            "entity.phantom.hurt",
            "entity.phantom.death"
        ],
        None,
        Baby::None
    ),
    def!(
        Evoker,
        "evoker",
        [
            "entity.evoker.ambient",
            "entity.evoker.hurt",
            "entity.evoker.death"
        ],
        None,
        Baby::None
    ),
    def!(
        Illusioner,
        "illusioner",
        [
            "entity.illusioner.ambient",
            "entity.illusioner.hurt",
            "entity.illusioner.death"
        ],
        None,
        Baby::None
    ),
    def!(
        Allay,
        "allay",
        [
            "entity.allay.ambient_without_item",
            "entity.allay.hurt",
            "entity.allay.death"
        ],
        None,
        Baby::None
    ),
    def!(
        Creaking,
        "creaking",
        [
            "entity.creaking.ambient",
            "entity.generic.hurt",
            "entity.creaking.death"
        ],
        Some("entity.creaking.step"),
        Baby::None
    ),
    def!(
        Camel,
        "camel",
        [
            "entity.camel.ambient",
            "entity.camel.hurt",
            "entity.camel.death"
        ],
        Some("entity.camel.step"),
        Baby::Abs(0.95, 1.4, 1.38)
    ),
    def!(
        HappyGhast,
        "happy_ghast",
        [
            "entity.happy_ghast.ambient",
            "entity.happy_ghast.hurt",
            "entity.happy_ghast.death"
        ],
        None,
        Baby::Scale(0.2375, Some(0.46875))
    ),
    def!(
        Sniffer,
        "sniffer",
        [
            "entity.sniffer.idle",
            "entity.sniffer.hurt",
            "entity.sniffer.death"
        ],
        Some("entity.sniffer.step"),
        Baby::Scale(0.5, None)
    ),
    def!(
        Breeze,
        "breeze",
        [
            "entity.breeze.idle_ground",
            "entity.breeze.hurt",
            "entity.breeze.death"
        ],
        None,
        Baby::None
    ),
    def!(
        Wither,
        "wither",
        [
            "entity.wither.ambient",
            "entity.wither.hurt",
            "entity.wither.death"
        ],
        None,
        Baby::None
    ),
    def!(
        Ravager,
        "ravager",
        [
            "entity.ravager.ambient",
            "entity.ravager.hurt",
            "entity.ravager.death"
        ],
        Some("entity.ravager.step"),
        Baby::None
    ),
    def!(
        Vex,
        "vex",
        ["entity.vex.ambient", "entity.vex.hurt", "entity.vex.death"],
        None,
        Baby::None
    ),
    def!(
        Shulker,
        "shulker",
        [
            "entity.shulker.ambient",
            "entity.shulker.hurt",
            "entity.shulker.death"
        ],
        None,
        Baby::None
    ),
    def!(
        Guardian,
        "guardian",
        [
            "entity.guardian.ambient",
            "entity.guardian.hurt",
            "entity.guardian.death"
        ],
        None,
        Baby::None
    ),
    def!(
        ElderGuardian,
        "elder_guardian",
        [
            "entity.elder_guardian.ambient",
            "entity.elder_guardian.hurt",
            "entity.elder_guardian.death"
        ],
        None,
        Baby::None
    ),
    def!(
        Pillager,
        "pillager",
        [
            "entity.pillager.ambient",
            "entity.pillager.hurt",
            "entity.pillager.death"
        ],
        None,
        Baby::None
    ),
    def!(
        Vindicator,
        "vindicator",
        [
            "entity.vindicator.ambient",
            "entity.vindicator.hurt",
            "entity.vindicator.death"
        ],
        None,
        Baby::None
    ),
    def!(
        Piglin,
        "piglin",
        [
            "entity.piglin.ambient",
            "entity.piglin.hurt",
            "entity.piglin.death"
        ],
        Some("entity.piglin.step"),
        Baby::Abs(0.49, 0.98, 0.78)
    ),
    def!(
        PiglinBrute,
        "piglin_brute",
        [
            "entity.piglin_brute.ambient",
            "entity.piglin_brute.hurt",
            "entity.piglin_brute.death"
        ],
        Some("entity.piglin_brute.step"),
        Baby::None
    ),
    def!(
        ZombifiedPiglin,
        "zombified_piglin",
        [
            "entity.zombified_piglin.ambient",
            "entity.zombified_piglin.hurt",
            "entity.zombified_piglin.death"
        ],
        None,
        Baby::Abs(0.49, 0.98, 0.78)
    ),
    def!(
        Hoglin,
        "hoglin",
        [
            "entity.hoglin.ambient",
            "entity.hoglin.hurt",
            "entity.hoglin.death"
        ],
        Some("entity.hoglin.step"),
        Baby::Abs(0.75, 0.85, 0.625)
    ),
    def!(
        Zoglin,
        "zoglin",
        [
            "entity.zoglin.ambient",
            "entity.zoglin.hurt",
            "entity.zoglin.death"
        ],
        Some("entity.zoglin.step"),
        Baby::Abs(0.75, 0.85, 0.625)
    ),
    def!(
        Ghast,
        "ghast",
        [
            "entity.ghast.ambient",
            "entity.ghast.hurt",
            "entity.ghast.death"
        ],
        None,
        Baby::None
    ),
    def!(
        Blaze,
        "blaze",
        [
            "entity.blaze.ambient",
            "entity.blaze.hurt",
            "entity.blaze.death"
        ],
        None,
        Baby::None
    ),
    def!(
        Strider,
        "strider",
        [
            "entity.strider.ambient",
            "entity.strider.hurt",
            "entity.strider.death"
        ],
        Some("entity.strider.step"),
        Baby::Abs(0.45, 0.85, 0.4375)
    ),
    def!(
        Warden,
        "warden",
        [
            "entity.warden.ambient",
            "entity.warden.hurt",
            "entity.warden.death"
        ],
        None,
        Baby::None
    ),
    // After the camel: the camel husk reuses `camel#main`.
    def!(
        CamelHusk,
        "camel_husk",
        [
            "entity.camel_husk.ambient",
            "entity.camel_husk.hurt",
            "entity.camel_husk.death"
        ],
        Some("entity.camel_husk.step"),
        Baby::None
    ),
    def!(
        Nautilus,
        "nautilus",
        [
            "entity.nautilus.ambient",
            "entity.nautilus.hurt",
            "entity.nautilus.death"
        ],
        None,
        Baby::Scale(0.5, None)
    ),
    def!(
        ZombieNautilus,
        "zombie_nautilus",
        [
            "entity.zombie_nautilus.ambient",
            "entity.zombie_nautilus.hurt",
            "entity.zombie_nautilus.death"
        ],
        None,
        Baby::None
    ),
    // Babies are size 1 and adults size 2: the size scales the dimensions.
    def!(
        SulfurCube,
        "sulfur_cube",
        ["", "entity.sulfur_cube.hurt", "entity.sulfur_cube.death"],
        None,
        Baby::None
    ),
    // Mannequin: the player model; the DATA_PROFILE ResolvableProfile picks
    // its skin, this texture only while that has none.
    def!(
        Mannequin,
        "mannequin",
        ["", "entity.generic.hurt", "entity.generic.death"],
        None,
        Baby::None
    ),
];
pub fn def(s: Species) -> &'static Def {
    ALL.iter().find(|d| d.species == s).unwrap_or(&ALL[0])
}
/// `AgeableMob.canBeABaby`: the species `Mob::is_baby` can ever answer `true`
/// for, which is what lets `getAge() < 0` mean "a baby" rather than "an age".
///
/// `canBeABaby` is `true` across the ageable tree but for the six classes
/// that override it to `false` — `Frog`, `Parrot`, `ZombieNautilus`,
/// `CamelHusk`, `Slime` and `MagmaCube`, the last two being `AbstractCubeMob`s
/// and so `AgeableMob`s themselves — and the zombies, the piglins and the
/// zoglin answer a synced `DATA_BABY_ID` rather than an age. Everything else
/// inherits `LivingEntity.isBaby`, which is `false`: a `Giant`, a `Creeper`,
/// a `PiglinBrute` or a `Tadpole` stays an adult however negative its `Age`.
pub fn can_be_a_baby(s: Species) -> bool {
    use Species as S;
    matches!(
        s,
        S::Armadillo
            | S::Axolotl
            | S::Bee
            | S::Camel
            | S::Cat
            | S::Chicken
            | S::Cow
            | S::Dolphin
            | S::Donkey
            | S::Drowned
            | S::Fox
            | S::GlowSquid
            | S::Goat
            | S::HappyGhast
            | S::Hoglin
            | S::Horse
            | S::Husk
            | S::Llama
            | S::Mooshroom
            | S::Mule
            | S::Nautilus
            | S::Ocelot
            | S::Panda
            | S::Pig
            | S::Piglin
            | S::PolarBear
            | S::Rabbit
            | S::Sheep
            | S::SkeletonHorse
            | S::Sniffer
            | S::Squid
            | S::Strider
            | S::SulfurCube
            | S::TraderLlama
            | S::Turtle
            | S::Villager
            | S::WanderingTrader
            | S::Wolf
            | S::Zoglin
            | S::Zombie
            | S::ZombieHorse
            | S::ZombieVillager
            | S::ZombifiedPiglin
    )
}
/// The vanilla entity type of a species, `minecraft:` and its [`Def::name`].
pub fn vanilla_id(s: Species) -> &'static str {
    static IDS: std::sync::LazyLock<Vec<String>> = std::sync::LazyLock::new(|| {
        ALL.iter()
            .map(|d| format!("minecraft:{}", d.name))
            .collect()
    });
    let index = ALL.iter().position(|d| d.species == s).unwrap_or(0);
    &IDS[index]
}
impl Def {
    pub fn flags(&self, fire_immune: bool) -> Flags {
        use Species as S;
        let s = self.species;
        let undead = matches!(
            s,
            S::Zombie
                | S::Husk
                | S::Drowned
                | S::ZombieVillager
                | S::Skeleton
                | S::Stray
                | S::Bogged
                | S::Parched
                | S::WitherSkeleton
                | S::ZombieHorse
                | S::SkeletonHorse
                | S::Phantom
                | S::ZombifiedPiglin
                | S::Zoglin
                | S::Wither
                | S::CamelHusk
                | S::ZombieNautilus
        );
        let aquatic = matches!(
            s,
            S::Squid
                | S::GlowSquid
                | S::Cod
                | S::Tadpole
                | S::Salmon
                | S::TropicalFish
                | S::Pufferfish
                | S::Dolphin
        );
        Flags {
            aquatic_swimmer: aquatic,
            fall_immune: matches!(
                s,
                S::Chicken
                    | S::Bat
                    | S::Parrot
                    | S::Bee
                    | S::Ghast
                    | S::HappyGhast
                    | S::Blaze
                    | S::Phantom
                    | S::IronGolem
                    | S::Cat
                    | S::Ocelot
                    | S::Slime
                    | S::MagmaCube
                    | S::Wither
                    | S::Allay
                    | S::Breeze
            ),
            flying: matches!(s, S::Ghast | S::Phantom | S::HappyGhast),
            climber: matches!(s, S::Spider | S::CaveSpider),
            lava_walker: s == S::Strider,
            powder_snow_walkable: false,
            omnidirectional: matches!(s, S::Bee | S::Parrot),
            fire_immune: fire_immune
                || matches!(
                    s,
                    S::Ghast
                        | S::Blaze
                        | S::MagmaCube
                        | S::Strider
                        | S::WitherSkeleton
                        | S::ZombifiedPiglin
                        | S::Zoglin
                        | S::Wither
                ),
            // The `burn_in_daylight` entity type tag.
            burns_in_day: matches!(
                s,
                S::Zombie
                    | S::ZombieHorse
                    | S::Drowned
                    | S::ZombieVillager
                    | S::Skeleton
                    | S::Stray
                    | S::Bogged
                    | S::WitherSkeleton
                    | S::Phantom
                    | S::ZombieNautilus
            ),
            undead,
            // `#can_breathe_under_water`. Not the dolphin: it holds its breath
            // (4800 ticks) and surfaces for air; nor the iron golem, which keeps
            // its air through its own `decreaseAirSupply` (`ai.rs`).
            breathes_water: (aquatic && s != S::Dolphin)
                || undead
                || matches!(
                    s,
                    S::Axolotl
                        | S::Frog
                        | S::Turtle
                        | S::Guardian
                        | S::ElderGuardian
                        | S::CopperGolem
                        | S::Nautilus
                ),
            // WaterAnimal.handleAirSupply: it dries out on land; Nautilus's
            // does the same. Dolphin.handleAirSupply is empty, so a dolphin
            // takes LivingEntity.baseTick's air like a land mob.
            needs_water: (aquatic && s != S::Dolphin) || s == S::Nautilus,
            water_sensitive: matches!(s, S::Enderman | S::Blaze | S::SnowGolem | S::Strider),
            animal: !matches!(s, S::Bat | S::SulfurCube | S::Mannequin)
                && !self.hostile_class()
                && !aquatic
                && !matches!(
                    s,
                    S::Villager | S::WanderingTrader | S::IronGolem | S::SnowGolem | S::CopperGolem
                ),
        }
    }
    fn hostile_class(&self) -> bool {
        use Species as S;
        matches!(
            self.species,
            S::Zombie
                | S::Husk
                | S::Drowned
                | S::ZombieVillager
                | S::Skeleton
                | S::Stray
                | S::Bogged
                | S::Parched
                | S::WitherSkeleton
                | S::Creeper
                | S::Spider
                | S::CaveSpider
                | S::Enderman
                | S::Endermite
                | S::Silverfish
                | S::Slime
                | S::MagmaCube
                | S::Witch
                | S::Phantom
                | S::Pillager
                | S::Vindicator
                | S::Piglin
                | S::PiglinBrute
                | S::ZombifiedPiglin
                | S::Hoglin
                | S::Zoglin
                | S::Ghast
                | S::Blaze
                | S::Guardian
                | S::ElderGuardian
                | S::Shulker
                | S::Evoker
                | S::Illusioner
                | S::Vex
                | S::Ravager
                | S::Wither
                | S::Warden
                | S::Breeze
                | S::Creaking
        )
    }
    /// The item tag Animal.isFood tests.
    pub fn food_tag(&self) -> Option<&'static str> {
        use Species as S;
        Some(match self.species {
            S::Pig => "pig_food",
            S::Cow | S::Mooshroom => "cow_food",
            S::Sheep => "sheep_food",
            S::Chicken => "chicken_food",
            S::Rabbit => "rabbit_food",
            S::Horse | S::Donkey | S::Mule => "horse_food",
            S::Llama | S::TraderLlama => "llama_food",
            S::Goat => "goat_food",
            S::Sniffer => "sniffer_food",
            S::HappyGhast => "happy_ghast_food",
            S::Camel => "camel_food",
            S::CamelHusk => "camel_husk_food",
            S::Nautilus | S::ZombieNautilus => "nautilus_food",
            S::SulfurCube => "sulfur_cube_food",
            S::Panda => "panda_food",
            S::Fox => "fox_food",
            S::Wolf => "wolf_food",
            S::Cat => "cat_food",
            S::Ocelot => "ocelot_food",
            S::Parrot => "parrot_food",
            S::Bee => "bee_food",
            S::Turtle => "turtle_food",
            S::Armadillo => "armadillo_food",
            S::Frog | S::Tadpole => "frog_food",
            S::Axolotl => "axolotl_food",
            S::Hoglin => "hoglin_food",
            S::Strider => "strider_food",
            S::Piglin => "piglin_food",
            _ => return None,
        })
    }
    pub fn tempt_tag(&self) -> Option<&'static str> {
        Some(match self.species {
            Species::Horse | Species::Donkey | Species::Mule => "horse_tempt_items",
            Species::Llama | Species::TraderLlama => "llama_tempt_items",
            Species::Strider => "strider_tempt_items",
            // An adult sulfur cube follows what it can swallow.
            Species::SulfurCube => "sulfur_cube_swallowable",
            _ => return None,
        })
    }
    pub fn water_slowdown(&self) -> f32 {
        match self.species {
            Species::PolarBear => 0.98,
            // SkeletonHorse.getWaterSlowDown.
            Species::SkeletonHorse => 0.96,
            _ => 0.8,
        }
    }
    pub fn air_speed(&self) -> f32 {
        match self.species {
            Species::Phantom => 0.2,
            Species::HappyGhast => super::happy_ghast::AIR_SPEED,
            _ => 0.02,
        }
    }
    pub fn baby_scale(&self) -> f64 {
        match self.baby {
            Baby::Scale(s, _) => s,
            _ => 0.5,
        }
    }
    pub fn head_rot_speed(&self) -> f32 {
        match self.species {
            Species::Frog => 35.0,
            Species::Breeze => 25.0,
            _ => 10.0,
        }
    }
    pub fn max_head_x_rot(&self) -> f32 {
        match self.species {
            Species::Dolphin | Species::Axolotl => 1.0,
            Species::Slime | Species::MagmaCube | Species::SulfurCube => 0.0,
            Species::Guardian | Species::ElderGuardian | Species::Shulker => 180.0,
            _ => 40.0,
        }
    }
    pub fn max_head_y_rot(&self) -> f32 {
        match self.species {
            Species::Dolphin | Species::Axolotl => 1.0,
            Species::Frog => 5.0,
            Species::Breeze => 30.0,
            Species::Sniffer => 50.0,
            Species::Goat => 15.0,
            Species::Armadillo => 32.0,
            Species::Ravager => 45.0,
            Species::Camel | Species::CamelHusk => 30.0,
            Species::Shulker => 180.0,
            _ => 75.0,
        }
    }
    pub fn ambient_interval(&self) -> i32 {
        use Species as S;
        match self.species {
            S::Turtle => 200,
            // AbstractHorse.getAmbientSoundInterval, for every subclass.
            S::Horse
            | S::Donkey
            | S::Mule
            | S::ZombieHorse
            | S::SkeletonHorse
            | S::Llama
            | S::TraderLlama
            | S::Camel
            | S::CamelHusk => 400,
            S::Ocelot => 900,
            // Guardian.getAmbientSoundInterval (the elder too).
            S::Guardian | S::ElderGuardian => 160,
            // Hoglin extends Animal; Shulker extends AbstractGolem.
            S::Hoglin | S::Shulker => 120,
            // Allay is a PathfinderMob: the Mob default.
            S::Allay => 80,
            S::Squid
            | S::GlowSquid
            | S::Cod
            | S::Tadpole
            | S::Salmon
            | S::TropicalFish
            | S::Pufferfish
            | S::Dolphin
            | S::Axolotl
            | S::IronGolem
            | S::SnowGolem
            | S::CopperGolem => 120,
            _ if self.flags(false).animal => 120,
            _ => 80,
        }
    }
    pub fn brain(&self) -> Brain {
        match self.species {
            Species::Chicken => Brain::Chicken,
            Species::Zombie | Species::Husk | Species::Drowned | Species::ZombieVillager => {
                Brain::Zombie
            }
            Species::Skeleton | Species::Stray | Species::Bogged | Species::Parched => {
                Brain::Skeleton
            }
            Species::Spider | Species::CaveSpider => Brain::Spider,
            Species::Creeper => Brain::Creeper,
            _ => Brain::Passive,
        }
    }
}
/// Mobs whose `body` equipment slot Horizon keeps in `Extra::head` (a
/// happy ghast's harness, nautilus armour, a sulfur cube's swallowed block,
/// a wolf's armor).
pub fn body_slot(s: Species) -> bool {
    matches!(
        s,
        Species::HappyGhast
            | Species::Nautilus
            | Species::ZombieNautilus
            | Species::SulfurCube
            | Species::Wolf
    )
}
