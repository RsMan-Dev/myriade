//! What a dying mob leaves behind: `LivingEntity.die`'s drop step. The
//! killer's `killedEntity` hook (a charged creeper's head drop), then
//! `dropAllDeathLoot`: the entity loot table (drawn from the world's random
//! sequence), `Mob.dropCustomDeathLoot` (equipment by drop chance, with the
//! entity random), the species' extra drops (an enderman's carried block) and
//! the experience orbs.
use super::{DYE_NAMES, Species, equine, gear, raider};
use crate::random_sequences::RandomSequences;
use horizon_core::random::{LegacyRandom, LevelRandom, RandomSource};
use horizon_world::{
    loot::{EnchantedLevel, EntityLootParams, LootDamage, LootData, LootEntity, LootStack},
    registry::BlockStateId,
};

/// `DropChances.DEFAULT_EQUIPMENT_DROP_CHANCE`.
pub const DEFAULT_DROP_CHANCE: f32 = 0.085;
/// `EquipmentSlot.VALUES`.
pub use horizon_world::loot::EQUIPMENT_SLOTS as SLOTS;

/// One thing a death leaves in the world, in spawn order.
#[derive(Clone, Debug, PartialEq)]
pub enum Drop {
    Item(LootStack),
    /// A whole stack a mob kept (in an equipment slot, its inventory or a
    /// horse's chest), with its damage, name and components, dropped as it is.
    Stack(crate::items::ItemStack),
    /// `Enderman.dropCustomDeathLoot`: the carried block's own drops, rolled
    /// with `enderman::carried_block_tool` (`enderman::carried_block_roll`).
    CarriedBlock(BlockStateId),
    /// The experience the mob is worth (`getExperienceReward`), before
    /// `ExperienceOrb.award` splits it into orbs.
    Experience(i32),
    /// A custom death drop with an explicit initial item-entity age.
    /// Bypasses stack-only table modifiers; joins the captured living drops.
    Aged(LootStack, i32),
}

/// How the mob died.
pub struct Death<'a> {
    pub loot: &'a LootData,
    /// The killing `DamageSource`.
    pub damage: Option<LootDamage>,
    /// `LAST_DAMAGE_PLAYER`: the player who last hurt the mob, when they are
    /// still in the world. Gone after they leave, even while the memory lasts.
    pub player: Option<LootEntity>,
    /// `LivingEntity.lastHurtByPlayerMemoryTime > 0` (`playerKilled`):
    /// experience and equipment follow this, not whether `player` is present.
    pub player_killed: bool,
    /// That player's luck.
    pub luck: f32,
    /// The entity the mob rides.
    pub vehicle: Option<LootEntity>,
    /// The `doMobLoot` game rule.
    pub mob_drops: bool,
    pub world_seed: i64,
}

/// What a death's loot rolls with: the world's random sequences, the level's
/// random and, when a mod listens, the game's answer to
/// `enchanted_entity_loot`, asked in the middle of a roll.
pub trait DeathRolls {
    fn sequences(&mut self) -> &mut RandomSequences;
    fn level_random(&mut self) -> &mut LegacyRandom;
    /// Whether [`Self::enchanted_level`] is to be asked at all.
    fn asks_enchanted_level(&self) -> bool {
        false
    }
    /// The level a loot table uses of `enchantment`, given the attacker's.
    fn enchanted_level(&mut self, _enchantment: &str, level: i32) -> i32 {
        level
    }
}
/// A death's randoms and nothing else: no mod is asked.
pub struct PlainRolls<'a> {
    pub sequences: &'a mut RandomSequences,
    pub level_random: &'a mut LegacyRandom,
}
impl DeathRolls for PlainRolls<'_> {
    fn sequences(&mut self) -> &mut RandomSequences {
        self.sequences
    }
    fn level_random(&mut self) -> &mut LegacyRandom {
        self.level_random
    }
}

/// `instanceof Monster` (Monster.shouldDropLoot/shouldDropExperience).
pub fn is_monster(s: Species) -> bool {
    use Species as S;
    matches!(
        s,
        S::Blaze
            | S::Bogged
            | S::Breeze
            | S::CaveSpider
            | S::Creaking
            | S::Creeper
            | S::Drowned
            | S::ElderGuardian
            | S::Enderman
            | S::Endermite
            | S::Evoker
            | S::Giant
            | S::Guardian
            | S::Husk
            | S::Illusioner
            | S::Parched
            | S::Piglin
            | S::PiglinBrute
            | S::Pillager
            | S::Ravager
            | S::Silverfish
            | S::Skeleton
            | S::Spider
            | S::Stray
            | S::Vex
            | S::Vindicator
            | S::Warden
            | S::Witch
            | S::Wither
            | S::WitherSkeleton
            | S::Zoglin
            | S::Zombie
            | S::ZombieVillager
            | S::ZombifiedPiglin
    )
}
/// The mobs that extend `Animal`.
pub fn is_animal(s: Species) -> bool {
    use Species as S;
    matches!(
        s,
        S::Armadillo
            | S::Axolotl
            | S::Bee
            | S::Camel
            | S::CamelHusk
            | S::Cat
            | S::Chicken
            | S::Cow
            | S::Donkey
            | S::Fox
            | S::Frog
            | S::HappyGhast
            | S::Goat
            | S::Hoglin
            | S::Horse
            | S::Llama
            | S::Mooshroom
            | S::Mule
            | S::Nautilus
            | S::Ocelot
            | S::Panda
            | S::Parrot
            | S::Pig
            | S::PolarBear
            | S::Rabbit
            | S::Sheep
            | S::SkeletonHorse
            | S::Sniffer
            | S::Strider
            | S::TraderLlama
            | S::Turtle
            | S::Wolf
            | S::ZombieHorse
            | S::ZombieNautilus
    )
}
/// AgeableWaterCreature and WaterAnimal.
fn is_water_creature(s: Species) -> bool {
    use Species as S;
    matches!(
        s,
        S::Dolphin
            | S::GlowSquid
            | S::Squid
            | S::Cod
            | S::Salmon
            | S::Pufferfish
            | S::TropicalFish
            | S::Tadpole
    )
}
fn zombie_family(s: Species) -> bool {
    use Species as S;
    matches!(
        s,
        S::Zombie | S::Husk | S::Drowned | S::ZombieVillager | S::ZombifiedPiglin
    )
}
fn is_raider(s: Species) -> bool {
    use Species as S;
    matches!(
        s,
        S::Pillager | S::Vindicator | S::Evoker | S::Illusioner | S::Witch | S::Ravager
    )
}

/// The `finalize` slot of a drop slot's name, for the slots a stack is kept in.
fn slot_index(slot: &str) -> Option<usize> {
    use super::finalize as f;
    Some(match slot {
        "mainhand" => f::MAINHAND,
        "offhand" => f::OFFHAND,
        "head" => f::HEAD,
        "chest" => f::CHEST,
        "legs" => f::LEGS,
        "feet" => f::FEET,
        _ => return None,
    })
}
/// `EnchantmentHelper.has(stack, EnchantmentEffectComponents.PREVENT_EQUIPMENT_DROP)`
/// on a whole stack: whether its enchantments keep it from dropping.
fn stack_prevents_drop(loot: &LootData, stack: &crate::items::ItemStack) -> bool {
    let mut probe = loot_stack(&stack.item, stack.count as i32);
    probe.enchantments = stack
        .enchantments()
        .iter()
        .map(|(e, l)| (e.to_string(), *l as i32))
        .collect();
    loot.prevents_equipment_drop(&probe)
}
fn loot_stack(item: &str, count: i32) -> LootStack {
    LootStack {
        item: item.to_owned(),
        count,
        damage: 0,
        enchantments: Vec::new(),
        potion: None,
        map: false,
        exploration: None,
        ominous_amplifier: None,
        stew_effects: Vec::new(),
        names: Vec::new(),
        components: Vec::new(),
    }
}
/// What `roll_death` collected, for `finish_death`.
pub struct Rolled {
    out: Vec<Drop>,
    skulls: bool,
    this: LootEntity,
    allay_load: Option<(std::sync::Arc<str>, i32)>,
    table_rolled: bool,
}
/// What a mod may change about the chance of one equipment slot
/// (`GetEquipmentDropChanceEvent`), asked before its roll.
pub struct EquipmentChoice<'a> {
    pub slot: usize,
    /// The stack that would drop (the mob's whole stack, damage as it is).
    pub stack: &'a Drop,
    /// `DropChances.byEquipment`, before the enchantments.
    pub original: f32,
    /// Whether the slot is preserved: it drops without a player kill.
    pub preserved: bool,
    /// The chance after the enchantments of the killer and the victim.
    pub chance: f32,
}
/// The loot parameters of the damage that killed `m`, as the equipment drop
/// chance enchantments read them (no luck, no killing player).
fn damage_context(this: LootEntity, d: &Death) -> EntityLootParams {
    EntityLootParams {
        this,
        damage: d.damage.clone(),
        player: None,
        luck: 0.0,
    }
}
/// `LootTable.getRandomItems(params, seed, ...)`: a seed's own random, else
/// the table's random sequence, else the level's random.
fn roll(
    d: &Death,
    table: &str,
    params: &EntityLootParams,
    rolls: &mut dyn DeathRolls,
    seed: i64,
) -> Vec<LootStack> {
    if seed != 0 {
        let random = &mut LegacyRandom::new(seed);
        return if rolls.asks_enchanted_level() {
            let ask = &mut |_: &mut LegacyRandom, id: &str, level| rolls.enchanted_level(id, level);
            d.loot.roll_death_entity(table, random, params, ask)
        } else {
            d.loot.roll_entity(table, random, params)
        };
    }
    match d.loot.random_sequence(table) {
        Some(id) => roll_live(d, table, params, rolls, |r| {
            r.sequences().get(id, d.world_seed)
        }),
        None => roll_live(d, table, params, rolls, |r| r.level_random()),
    }
}
/// A roll on a random the game keeps (`home` finds it), with `roll` given the
/// question to ask at each level it reads. A listener asked in the middle of
/// the roll can reach that random, so it is back home while the listener runs,
/// and the roll carries on from wherever the listener left it.
pub fn roll_asked<R: Clone, T>(
    rolls: &mut dyn DeathRolls,
    home: impl for<'r> Fn(&'r mut dyn DeathRolls) -> &'r mut R,
    roll: impl FnOnce(&mut R, &mut EnchantedLevel<'_, R>) -> T,
) -> T {
    let mut random = home(&mut *rolls).clone();
    let rolled =
        roll(&mut random, &mut |random: &mut R,
                                enchantment: &str,
                                level| {
            *home(&mut *rolls) = random.clone();
            let level = rolls.enchanted_level(enchantment, level);
            *random = home(&mut *rolls).clone();
            level
        });
    *home(rolls) = random;
    rolled
}
/// A death's roll on a random the game keeps: asked when a mod listens.
fn roll_live<R: RandomSource + Clone>(
    d: &Death,
    table: &str,
    params: &EntityLootParams,
    rolls: &mut dyn DeathRolls,
    home: impl for<'r> Fn(&'r mut dyn DeathRolls) -> &'r mut R,
) -> Vec<LootStack> {
    if !rolls.asks_enchanted_level() {
        return d.loot.roll_entity(table, home(rolls), params);
    }
    roll_asked(rolls, home, |random, ask| {
        d.loot.roll_death_entity(table, random, params, ask)
    })
}

#[cfg(test)]
mod death_loot_table_tests {
    use super::*;
    use crate::mobs::world::level_random;
    use horizon_world::loot::{LootData, PackOverlay};


    /// A listener of `enchanted_entity_loot` draws from the level's random in
    /// the middle of a roll on that random: the roll carries on after its
    /// draw, as vanilla's one shared random does.
    #[test]
    fn a_draw_made_while_asked_keeps_its_place_in_the_roll() {
        struct Drawing<'a>(PlainRolls<'a>, bool);
        impl DeathRolls for Drawing<'_> {
            fn sequences(&mut self) -> &mut RandomSequences {
                self.0.sequences()
            }
            fn level_random(&mut self) -> &mut LegacyRandom {
                self.0.level_random()
            }
            fn asks_enchanted_level(&self) -> bool {
                true
            }
            fn enchanted_level(&mut self, _: &str, level: i32) -> i32 {
                if self.1 {
                    self.0.level_random.next_int();
                }
                level
            }
        }
        let root = std::env::temp_dir().join(format!("loot-drops-{}", std::process::id()));
        let dir = root.join("data/minecraft/loot_table");
        std::fs::create_dir_all(&dir).unwrap();
        let table = r#"{"pools":[{"rolls":1,"entries":[{"type":"minecraft:item",
            "name":"minecraft:stone","modifier":[
            {"type":"minecraft:enchanted_count_increase","enchantment":"minecraft:looting",
             "count":0.0},
            {"type":"minecraft:set_count",
             "count":{"type":"minecraft:uniform","min":1.0,"max":60.0}}]}]}]}"#;
        std::fs::write(dir.join("test.json"), table).unwrap();
        std::fs::write(root.join("items.json"), "{}").unwrap();
        let defaults = horizon_world::item_defaults::read(&root).unwrap();
        let loot = LootData::load(&root, &PackOverlay::NONE, &defaults).unwrap();
        let _ = std::fs::remove_dir_all(&root);
        let death = Death {
            loot: &loot,
            damage: None,
            player: None,
            player_killed: false,
            luck: 0.0,
            vehicle: None,
            mob_drops: true,
            world_seed: 0,
        };
        let run = |mut level_random: LegacyRandom, draws: bool| {
            let mut sequences = RandomSequences::default();
            let mut rolls = Drawing(
                PlainRolls {
                    sequences: &mut sequences,
                    level_random: &mut level_random,
                },
                draws,
            );
            let params = EntityLootParams::default();
            let items = roll(&death, "minecraft:test", &params, &mut rolls, 0);
            (items[0].count, level_random.next_long())
        };
        // The draw comes before the roll's only one, as if made just before it.
        let mut ahead = LegacyRandom::new(7);
        ahead.next_int();
        assert_eq!(run(LegacyRandom::new(7), true), run(ahead, false));
    }

}
#[cfg(test)]
mod slot_tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;
    use crate::mobs::world::level_random;
    use horizon_core::geometry::Vec3;
    use horizon_world::loot::PackOverlay;
    use std::sync::Arc;


    /// A wolf's armor is the body slot, with the durability it lost; a
    /// helmet beside it reads from the head slot.
    #[test]
    fn a_wolfs_slots_read_apart() {
        let mut wolf = mob("minecraft:wolf", Species::Wolf);
        wolf.extra.set_body(Some("minecraft:wolf_armor".into()));
        wolf.extra.set_body_damage(9);
        wolf.extra.set_slot_id(
            horizon_items::equipment::HEAD,
            Some("minecraft:iron_helmet".into()),
        );
        let body = slot_item(&wolf, "body").unwrap();
        let head = slot_item(&wolf, "head").unwrap();
        assert_eq!(
            (body.item.as_str(), body.damage),
            ("minecraft:wolf_armor", 9)
        );
        assert_eq!(head.item, "minecraft:iron_helmet");
        // The armor was put on by `Equippable`, so it always drops.
        assert_eq!(drop_chance(&wolf, "body"), 2.0);
    }

    /// A body item nothing equipped (a `/item replace ... armor.body` on a
    /// zombie) drops by the default chance, as vanilla's unequipped slot.
    #[test]
    fn a_zombies_body_item_drops_by_the_default_chance() {
        let mut zombie = mob("minecraft:zombie", Species::Zombie);
        zombie.extra.set_body(Some("minecraft:iron_helmet".into()));
        assert_eq!(drop_chance(&zombie, "body"), DEFAULT_DROP_CHANCE);
        let body = slot_item(&zombie, "body").unwrap();
        assert_eq!(body.item, "minecraft:iron_helmet");
        assert!(slot_item(&zombie, "head").is_none());
    }

    /// `LivingEntity.dropExperience` and `Mob.dropCustomDeathLoot`: the
    /// memory timer (`playerKilled`) awards experience and equipment even
    /// when `LAST_DAMAGE_PLAYER` is gone (the player left).
    #[test]
    fn experience_and_equipment_follow_the_hurt_memory_not_the_player_entity() {
        let root =
            std::env::temp_dir().join(format!("horizon-guest-left-xp-{}", std::process::id()));
        std::fs::create_dir_all(root.join("data/minecraft/loot_table")).unwrap();
        std::fs::write(root.join("items.json"), "{}").unwrap();
        let defaults = horizon_world::item_defaults::read(&root).unwrap();
        let loot = LootData::load(&root, &PackOverlay::default(), &defaults).unwrap();
        let death = |player_killed| Death {
            loot: &loot,
            damage: None,
            player: None,
            player_killed,
            luck: 0.0,
            vehicle: None,
            mob_drops: true,
            world_seed: 0,
        };
        let zombie = |helmet: bool| {
            let mut m = mob("minecraft:zombie", Species::Zombie);
            if helmet {
                m.extra.set_slot_id(
                    horizon_items::equipment::HEAD,
                    Some("minecraft:iron_helmet".into()),
                );
                m.extra.drop_chances = Some(Box::new([1.0; 8]));
            }
            m
        };
        let orbs = |out: &[Drop]| {
            out.iter()
                .filter(|d| matches!(d, Drop::Experience(_)))
                .count()
        };
        let helmet = |out: &[Drop]| {
            out.iter().any(|d| match d {
                Drop::Item(s) => s.item == "minecraft:iron_helmet",
                Drop::Stack(s) => &*s.item == "minecraft:iron_helmet",
                _ => false,
            })
        };

        let mut m = zombie(false);
        let (out, _) = die(
            &mut m,
            &death(true),
            &mut RandomSequences::default(),
            &mut level_random(7),
        );
        assert!(orbs(&out) > 0, "memory timer awards experience: {out:?}");

        let mut m = zombie(false);
        let (out, _) = die(
            &mut m,
            &death(false),
            &mut RandomSequences::default(),
            &mut level_random(7),
        );
        assert_eq!(orbs(&out), 0, "no timer, no experience: {out:?}");

        let mut m = zombie(true);
        let (out, _) = die(
            &mut m,
            &death(true),
            &mut RandomSequences::default(),
            &mut level_random(7),
        );
        assert!(helmet(&out), "memory timer drops equipment: {out:?}");

        let mut m = zombie(true);
        let (out, _) = die(
            &mut m,
            &death(false),
            &mut RandomSequences::default(),
            &mut level_random(7),
        );
        assert!(!helmet(&out), "no timer, no equipment: {out:?}");
    }

    #[test]
    fn preserved_equipment_drops_when_chance_exceeds_one() {
        let mut m = mob("minecraft:zombie", Species::Zombie);
        m.extra.set_slot_stack(
            horizon_items::equipment::HEAD,
            Some(crate::items::ItemStack::new("minecraft:iron_helmet", 1)),
        );
        m.extra.drop_chances = Some(Box::new([2.0; 8]));
        let drops = drop_preserved_equipment(&mut m);
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].item.as_ref(), "minecraft:iron_helmet");
        assert_eq!(m.extra.slot_stack(horizon_items::equipment::HEAD), None);
    }
}

