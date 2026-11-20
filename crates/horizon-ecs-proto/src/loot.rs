//! What a death leaves on the ground: `LivingEntity.dropAllDeathLoot` for an entity of the frame. The loot table is Horizon's (`LootData`,
//! rolled with the entity's own seed when it has one, as `Mob`'s `death_table_seed`, else with the level's random); each stack becomes an
//! item on the ground at the place of the death (`spawn_at_location`), and what the entity held in its hand comes out first. The experience
//! orbs are another kind of entity and are not made.
use crate::cow::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::random::{LegacyRandom, RandomSource};
use horizon_world::loot::{EntityLootParams, LootData, LootEntity, PackOverlay};

/// The loot tables of the world's resources (loaded once, from `HORIZON_TEST_RESOURCES` in the benches and tests).
#[derive(Resource)]
pub struct LootTables(pub LootData);
/// The level's random, which a table without a seed of its own is rolled with.
#[derive(Resource)]
pub struct LevelRandom(pub LegacyRandom);
/// What a death is worth in experience (`Mob.getBaseExperienceReward`: `1 + nextInt(3)` for an animal, 5 for a monster).
#[derive(Component, Clone, Copy, Debug)]
pub struct XpReward {
    pub min: i32,
    pub max: i32,
}
/// The rule that gives a species an experience reward.
pub static XP: crate::rules::Rule = crate::rules::Rule {
    name: "xp",
    equip: |w, e, p| {
        let p = *crate::rules::params::<XpReward>(p);
        w.entity_mut(e).insert(p);
    },
    attrs: crate::rules::no_attrs,
    interact: crate::rules::no_interact,
};
/// The seed an entity's death table is rolled with (`Mob.extra.death_table_seed`); 0 or absent: the level's random.
#[derive(Component, Clone, Copy, Debug)]
pub struct DeathSeed(pub i64);

/// Reads the loot tables now (what the first death of a world would do: some 276 million instructions, a hitch of 90 ms): a bench
/// world calls it at its creation so that the first death of a fish on dry land is not counted as a cost of every tick.
pub fn preload(world: &mut World) {
    tables(world);
}
fn tables(world: &mut World) -> bool {
    if world.get_resource::<LootTables>().is_some() {
        return true;
    }
    let Some(root) = std::env::var_os("HORIZON_TEST_RESOURCES").map(std::path::PathBuf::from) else { return false };
    let Ok(defaults) = horizon_world::item_defaults::read(&root) else { return false };
    let Ok(data) = LootData::load(&root, &PackOverlay::default(), &defaults) else { return false };
    world.insert_resource(LootTables(data));
    if world.get_resource::<LevelRandom>().is_none() {
        world.insert_resource(LevelRandom(LegacyRandom::new(0x5EED)));
    }
    true
}
/// The stacks the death of an entity of type `kind` rolls: `table` as `Mob.getLootTable` names it (`entities/<name>`), with the facts the
/// predicates read (on fire, baby).
pub fn roll(world: &mut World, kind: &str, on_fire: bool, baby: bool, seed: i64) -> Vec<ItemStack> {
    if !tables(world) {
        return Vec::new();
    }
    let (ns, path) = kind.split_once(':').unwrap_or(("minecraft", kind));
    let table = format!("{ns}:entities/{path}");
    let mut this = LootEntity::new(kind);
    this.on_fire = on_fire;
    this.baby = baby;
    let params = EntityLootParams { this, damage: None, player: None, luck: 0.0 };
    // `Mob.dropFromLootTable` for a baby: no table unless the type is a monster (only the animals of the frame are asked here).
    if baby {
        return Vec::new();
    }
    let stacks = world.resource_scope(|world: &mut World, t: Mut<LootTables>| {
        if seed != 0 {
            t.0.roll_entity(&table, &mut LegacyRandom::new(seed), &params)
        } else {
            let mut r = world.resource_mut::<LevelRandom>();
            t.0.roll_entity(&table, &mut r.0, &params)
        }
    });
    stacks.into_iter().filter(|s| !s.is_empty()).map(|s| ItemStack::from_loot(&s)).collect()
}
/// The stacks a table that is not an entity's rolls (a gameplay table such as the dig of a seed: `minecraft:gameplay/...`), from `seed`.
pub fn roll_table(world: &mut World, table: &str, seed: i64) -> Vec<ItemStack> {
    if !tables(world) {
        return Vec::new();
    }
    // No entity rolls this table: a placeholder type that is no species.
    let params = EntityLootParams { this: LootEntity::new(crate::species::lists::MARKER), damage: None, player: None, luck: 0.0 };
    let stacks = world.resource_scope(|_, t: Mut<LootTables>| t.0.roll_entity(table, &mut LegacyRandom::new(seed), &params));
    stacks.into_iter().filter(|s| !s.is_empty()).map(|s| ItemStack::from_loot(&s)).collect()
}
/// The death of `e`: its hand, then its table, on the ground where it died. Run before the entity goes.
pub fn on_death(world: &mut World, e: Entity) {
    let Some(at) = world.get::<Pos>(e).map(|p| p.0) else { return };
    // A bench world drops nothing (`orb::NoXp`): a bench player never picks anything up, and the stacks and orbs left among the herd cost
    // each tick for thousands of ticks (items and orbs that no player collects), where a game collects them within seconds.
    if world.get_resource::<crate::orb::NoXp>().is_some() {
        crate::size::split_on_death(world, e);
        return;
    }
    let Some(kind) = world.get::<TypeName>(e).map(|t| t.0) else { return };
    let now = now_of(world, e);
    let on_fire = world.get::<Burning>(e).is_some_and(|b| b.until > now);
    let baby = match (world.get::<Sizes>(e), world.get::<Body>(e)) {
        (Some(s), Some(b)) => b.width < s.adult.0,
        _ => false,
    };
    let seed = world.get::<DeathSeed>(e).map_or(0, |s| s.0);
    let mut out = Vec::new();
    // `Fox.dropAllDeathLoot`: what it holds in its mouth comes out first.
    if let Some(h) = world.get::<crate::goals::HandStack>(e).and_then(|h| h.0.clone()) {
        out.push(h);
    }
    // `Mob.dropCustomDeathLoot`: what it picked up and wears (drop chance 2,0) always comes out, the pieces of armor too.
    if let Some(looted) = world.get::<crate::behaviours::pick_up_gear::Looted>(e).map(|l| l.0) {
        use crate::goals::{equipped, Slot};
        for slot in [Slot::Head, Slot::Chest, Slot::Legs, Slot::Feet, Slot::Off] {
            let id = equipped(world, e, slot);
            if id != 0 && looted & crate::behaviours::pick_up_gear::slot_bit(slot) != 0 && let Some(n) = crate::items::item_name(id) {
                out.push(horizon_client_lib::items::ItemStack::new(n, 1));
            }
        }
    }
    out.extend(roll(world, kind, on_fire, baby, seed));
    let mut r = LegacyRandom::new(seed ^ 0x2545_F491 ^ i64::from(world.get::<crate::goals::Uid>(e).map_or(0, |u| u.0 as i32)));
    for stack in out {
        crate::item::spawn_at_location(world, stack, at, &mut r);
    }
    // What a body of a size leaves: the smaller ones.
    crate::size::split_on_death(world, e);
    // The experience: not for a baby (`LivingEntity.shouldDropExperience`).
    if let Some(x) = world.get::<XpReward>(e).copied().filter(|_| !baby) {
        let amount = x.min + r.next_int_bound(x.max - x.min + 1).unwrap_or(0);
        crate::orb::award(world, at, amount.max(0) as u32, &mut r);
    }
}
