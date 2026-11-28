//! What a death leaves on the ground: `LivingEntity.dropAllDeathLoot` for an entity of the frame. The loot table is Horizon's (`LootData`,
//! rolled with the entity's own seed when it has one, as `Mob`'s `death_table_seed`, else with the level's random); each stack becomes an
//! item on the ground at the place of the death (`spawn_at_location`), and what the entity held in its hand comes out first. The experience
//! orbs are another kind of entity and are not made.
use crate::cow::*;
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;
use crate::rules::loot::{death_seed, entity_table, xp_amount};
use horizon_core::random::{LegacyRandom, RandomSource};
use horizon_world::loot::{EntityLootParams, LootData, LootEntity, PackOverlay};

/// The loot tables of the world's resources (loaded once, from `HORIZON_TEST_RESOURCES` in the benches and tests).
#[derive(Resource)]
pub struct LootTables(pub std::sync::Arc<LootData>);
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
/// What a baby of the species still leaves. `loot`: its table and equipment (`Monster.shouldDropLoot` ignores the age; `Hoglin`, an animal
/// that is an enemy, keeps `LivingEntity`'s: no loot for a baby). Any `BabyDrops` keeps the experience (`shouldDropExperience` true), scaled by
/// `xp_scale` (`Zombie.getBaseExperienceReward`: 2.5 for a baby). Without it a baby leaves nothing (`Animal`, `LivingEntity`).
#[derive(Component, Clone, Copy, Debug)]
pub struct BabyDrops {
    pub loot: bool,
    pub xp_scale: f32,
}
impl BabyDrops {
    /// A monster's baby: loot, experience as is (`Piglin`, `Zoglin`).
    pub const MONSTER: BabyDrops = BabyDrops { loot: true, xp_scale: 1.0 };
    /// `Zombie`'s baby: loot, experience times 2.5.
    pub const ZOMBIE: BabyDrops = BabyDrops { loot: true, xp_scale: 2.5 };
    /// `Hoglin`'s baby: experience but no loot.
    pub const HOGLIN: BabyDrops = BabyDrops { loot: false, xp_scale: 1.0 };
}
/// The reward is fixed by the species and no equipment adds to it: `Hoglin` and `Piglin` return `xpReward` as is, where `Mob`'s adds
/// `1 + nextInt(3)` per slot that holds a piece (a ranged reward, `Animal`'s `1 + nextInt(3)`, adds nothing either).
#[derive(Component, Clone, Copy, Debug)]
pub struct FlatXp;
/// The rule that makes the reward flat (`FlatXp`; no params).
pub static FLAT_XP: crate::rules::Rule = crate::rules::Rule {
    name: "flat_xp",
    equip: |w, e, _| {
        w.entity_mut(e).insert(FlatXp);
    },
    attrs: crate::rules::no_attrs,
    interact: crate::rules::no_interact,
};
/// A reward that replaces the species' own, flat (`Chicken.getBaseExperienceReward`: a chicken jockey is worth 10).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct RewardOverride(pub i32);
/// A chicken jockey's reward.
pub const JOCKEY_XP: i32 = 10;
/// The species leaves no experience, whatever its reward (`Tadpole.shouldDropExperience` is false).
#[derive(Component, Clone, Copy, Debug)]
pub struct NoXpDrop;
/// The rule that makes the species leave no experience (`NoXpDrop`; no params).
pub static NO_XP_DROP: crate::rules::Rule = crate::rules::Rule {
    name: "no_xp_drop",
    equip: |w, e, _| {
        w.entity_mut(e).insert(NoXpDrop);
    },
    attrs: crate::rules::no_attrs,
    interact: crate::rules::no_interact,
};
/// The rule that makes a baby of the species drop (`BabyDrops`).
pub static BABY_DROPS: crate::rules::Rule = crate::rules::Rule {
    name: "baby_drops",
    equip: |w, e, p| {
        let p = *crate::rules::params::<BabyDrops>(p);
        w.entity_mut(e).insert(p);
    },
    attrs: crate::rules::no_attrs,
    interact: crate::rules::no_interact,
};
/// What a death reads of the player that hit the entity last (`dropFromLootTable`'s killer: its luck and the weapon in its main hand,
/// whose Looting counts). The host tells it with the hit; the loot stack is made at the death only.
#[derive(Component, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct KillerKit {
    pub player: u32,
    pub luck: f32,
    pub weapon: Option<ItemStack>,
}
/// Puts the kit of the player `player` on `e`, over the one a former hit left.
pub fn note_killer(world: &mut World, e: Entity, player: u32, luck: f32, weapon: Option<ItemStack>) {
    if let Ok(mut x) = world.get_entity_mut(e) {
        x.insert(KillerKit { player, luck, weapon });
    }
}
/// The seed an entity's death table is rolled with (`Mob.extra.death_table_seed`); 0 or absent: the level's random.
#[derive(Component, Clone, Copy, Debug)]
pub struct DeathSeed(pub i64);

/// Reads the loot tables now (what the first death of a world would do: some 276 million instructions, a hitch of 90 ms): a bench
/// world calls it at its creation so that the first death of a fish on dry land is not counted as a cost of every tick.
/// The host's tables (its data packs included), given again whenever it reloads them; the same tables twice change nothing.
pub fn install(world: &mut World, data: std::sync::Arc<LootData>) {
    if world.get_resource::<LootTables>().is_some_and(|t| std::sync::Arc::ptr_eq(&t.0, &data)) {
        return;
    }
    world.insert_resource(LootTables(data));
    if world.get_resource::<LevelRandom>().is_none() {
        world.insert_resource(LevelRandom(LegacyRandom::new(0x5EED)));
    }
}
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
    world.insert_resource(LootTables(std::sync::Arc::new(data)));
    if world.get_resource::<LevelRandom>().is_none() {
        world.insert_resource(LevelRandom(LegacyRandom::new(0x5EED)));
    }
    true
}
/// The stacks the death of an entity of type `kind` rolls: `table` as `Mob.getLootTable` names it (`entities/<name>`), with the facts the
/// predicates read (on fire, baby).
pub fn roll(world: &mut World, kind: &str, on_fire: bool, baby: bool, seed: i64) -> Vec<ItemStack> {
    let mut this = LootEntity::new(kind);
    this.on_fire = on_fire;
    this.baby = baby;
    roll_with(world, this, None, seed)
}
/// The same with every fact the predicates read (`entity_for_loot`) and the player that killed it (`LAST_DAMAGE_PLAYER`, None: no player).
pub fn roll_with(world: &mut World, this: LootEntity, killer: Option<LootEntity>, seed: i64) -> Vec<ItemStack> {
    roll_params(world, EntityLootParams { this, damage: None, player: killer, luck: 0.0 }, seed)
}
/// The stacks of `params.this`'s table, rolled with the killer's luck and weapon and the damage source the params carry.
pub fn roll_params(world: &mut World, params: EntityLootParams, seed: i64) -> Vec<ItemStack> {
    if !tables(world) {
        return Vec::new();
    }
    // `Mob.dropFromLootTable` for a baby: no table unless the type is a monster (only the animals of the frame are asked here).
    if params.this.baby {
        return Vec::new();
    }
    let table = entity_table(&params.this.kind);
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
    roll_table_for(world, None, table, seed)
}
/// `roll_table` for a table that reads the entity `this` (a gift: the chicken's variant picks its egg); no entity: a placeholder type.
pub fn roll_table_for(world: &mut World, this: Option<Entity>, table: &str, seed: i64) -> Vec<ItemStack> {
    if !tables(world) {
        return Vec::new();
    }
    let this = this.and_then(|e| entity_for_loot(world, e)).unwrap_or_else(|| LootEntity::new(crate::species::lists::MARKER));
    let params = EntityLootParams { this, damage: None, player: None, luck: 0.0 };
    let stacks = world.resource_scope(|_, t: Mut<LootTables>| t.0.roll_entity(table, &mut LegacyRandom::new(seed), &params));
    stacks.into_iter().filter(|s| !s.is_empty()).map(|s| ItemStack::from_loot(&s)).collect()
}
/// The facts a loot predicate reads about the live entity `e` (`/loot ... kill`, a death): its type, fire and age, then what its components
/// say: a sheep's dye and wool (`Wool`), a cube's size (`Size`), a raider's banner (`Raider`, `Patrol`), a charged creeper (`Charged`), what it
/// wears (`equip_stack`, else the item ids of `Equipment`) and what it rides. A frog's and a chicken's variant come from the `Variant` table of the species (saved key `variant`).
pub fn entity_for_loot(world: &World, e: Entity) -> Option<LootEntity> {
    let mut this = facts(world, e)?;
    if let Some(m) = world.get::<crate::mount::Riding>(e).and_then(|r| facts(world, r.0)) {
        this.vehicle = Some(Box::new(m));
    }
    Some(this)
}
/// An item of the frame as a loot stack (id, count, wear, enchantments, custom data as its predicates read it).
pub fn loot_stack(stack: &ItemStack) -> horizon_world::loot::LootStack {
    let components = stack.custom_data().map(|d| ("minecraft:custom_data".to_owned(), Some(horizon_core::nbt::Tag::Compound(d.clone())))).into_iter().collect();
    horizon_world::loot::LootStack {
        item: stack.item.to_string(),
        count: stack.count as i32,
        damage: stack.damage as i32,
        enchantments: stack.all_enchantments().iter().map(|(n, l)| (n.to_string(), *l as i32)).collect(),
        potion: None,
        map: false,
        exploration: None,
        ominous_amplifier: None,
        stew_effects: Vec::new(),
        names: Vec::new(),
        components,
    }
}
/// The `LootEntity.equipment` slot names, in the order of `equip_stack::SLOTS`.
const SLOT_NAMES: [&str; crate::equip_stack::SLOTS] = ["mainhand", "offhand", "head", "chest", "legs", "feet", "body", "saddle"];
fn facts(world: &World, e: Entity) -> Option<LootEntity> {
    use crate::goals::{equipped, Slot};
    let mut this = LootEntity::new(world.get::<TypeName>(e)?.0);
    let now = now_of(world, e);
    this.on_fire = world.get::<Burning>(e).is_some_and(|b| b.until > now as i64);
    this.baby = match (world.get::<Sizes>(e), world.get::<Body>(e)) {
        (Some(s), Some(b)) => b.width < s.adult.0,
        _ => false,
    };
    this.sheep = world.get::<crate::farm::Wool>(e).map(|w| (crate::species::cushion::DYES[usize::from(w.color & 15)].to_owned(), w.sheared));
    // A creature never given a variant (no spawn draw) has the registry's default: the first (temperate).
    let drawn = world.get::<crate::behaviours::variant::Variant>(e).map_or(0, |v| v.0);
    if let Some(t) = crate::behaviours::variant::table_of(world, e)
        && t.key == "variant"
        && let Some(name) = t.ids.iter().position(|x| *x == drawn).and_then(|i| t.names.get(i))
    {
        let id = if name.contains(':') { (*name).to_owned() } else { format!("minecraft:{name}") };
        (this.frog_variant, this.chicken_variant) = (Some(id.clone()), Some(id));
    }
    let component = match this.kind.as_str() {
        "minecraft:mooshroom" => Some(&crate::species::variants::MUSHROOM),
        "minecraft:horse" => Some(&crate::species::variants::HORSE),
        "minecraft:llama" | "minecraft:trader_llama" => Some(&crate::species::variants::LLAMA),
        _ => None,
    };
    if let Some(name) = component.and_then(|t| crate::rules::variant::component_name(t, drawn)) {
        let id = Some(format!("minecraft:{name}"));
        match this.kind.as_str() {
            "minecraft:mooshroom" => this.mooshroom_variant = id,
            "minecraft:horse" => this.horse_variant = id,
            _ => this.llama_variant = id,
        }
    }
    this.cube_size = world.get::<crate::size::Size>(e).map(|s| s.0);
    this.powered = world.get::<crate::explosion::Charged>(e).is_some();
    // `Raider.isCaptain`: the ominous banner on its head.
    if world.get::<crate::raids::Raider>(e).is_some() || world.get::<crate::raids::Patrol>(e).is_some() {
        this.captain = Some(crate::equip_stack::get(world, e, 2).is_some_and(|s| s.data().is_some_and(|d| d.ominous_banner)));
    }
    let by_id = [Slot::Main, Slot::Off, Slot::Head, Slot::Chest, Slot::Legs, Slot::Feet, Slot::Body, Slot::Saddle];
    for (i, name) in SLOT_NAMES.into_iter().enumerate() {
        let stack = crate::equip_stack::get(world, e, i).or_else(|| match equipped(world, e, by_id[i]) {
            0 => None,
            id => crate::items::item_name(id).map(|n| ItemStack::new(n, 1)),
        });
        if let Some(s) = stack {
            this.equipment.push((name.to_owned(), loot_stack(&s)));
        }
    }
    Some(this)
}
/// The player `id` that hurt `victim` as a loot entity (its weapon in the main hand) and its luck, as the host told them with the hit.
fn player_entity(world: &World, victim: Entity, id: u32) -> (LootEntity, f32) {
    let mut p = LootEntity::new("minecraft:player");
    let kit = world.get::<KillerKit>(victim).filter(|k| k.player == id);
    if let Some(w) = kit.and_then(|k| k.weapon.as_ref()) {
        p.equipment.push(("mainhand".to_owned(), loot_stack(w)));
    }
    (p, kit.map_or(0.0, |k| k.luck))
}
/// The damage source of the last blow of `e`'s `CombatLog` (`DamageSource`: type, causing entity, direct entity).
pub fn damage_of(world: &World, e: Entity) -> Option<horizon_world::loot::LootDamage> {
    use crate::targeting::Who;
    let c = *world.get::<CombatLog>(e)?.entries.last()?;
    let source = match c.by {
        Some(Who::Player(id)) => Some(player_entity(world, e, id).0),
        Some(Who::Entity(a)) => entity_for_loot(world, a),
        None => None,
    };
    // `getDirectEntity`: the attacker itself for its own blow, else the projectile that carried it (known by its type).
    let direct = if c.direct { source.clone() } else { c.projectile.map(LootEntity::new) };
    Some(horizon_world::loot::LootDamage { kind: format!("minecraft:{}", c.damage_type), direct, source })
}
/// The `equip_stack::SLOTS` index of a slot.
pub(crate) fn stack_slot(slot: crate::goals::Slot) -> usize {
    use crate::goals::Slot::*;
    match slot {
        Main => 0,
        Off => 1,
        Head => 2,
        Chest => 3,
        Legs => 4,
        Feet => 5,
        Body => 6,
        Saddle => 7,
    }
}
/// `Mob.dropCustomDeathLoot`'s equipment: what each slot leaves (chance, Looting of a living killer, wear), taken off the slot. A piece the
/// mob picked up (`Looted`) has the stored chance 2,0 and so always falls.
fn equipment_stacks(world: &mut World, e: Entity, this: LootEntity, damage: Option<horizon_world::loot::LootDamage>, r: &mut LegacyRandom) -> Vec<ItemStack> {
    use crate::equipment_drop::{death_drops, index, DropChances, DEFAULT};
    use crate::goals::Slot;
    if let Some(l) = world.get::<crate::behaviours::pick_up_gear::Looted>(e).map(|l| l.0) {
        let mut c = world.get::<DropChances>(e).map_or([DEFAULT; 8], |c| c.0);
        for slot in [Slot::Main, Slot::Off, Slot::Feet, Slot::Legs, Slot::Chest, Slot::Head] {
            if l & crate::behaviours::pick_up_gear::slot_bit(slot) != 0 {
                c[index(slot)] = 2.0;
            }
        }
        world.entity_mut(e).insert(DropChances(c));
    }
    let killed = crate::credit::killed_by_player(world, e);
    let living = damage.as_ref().is_some_and(|d| d.source.is_some());
    let dropped = if tables(world) {
        let prevented: Vec<u32> = {
            let t = &world.resource::<LootTables>().0;
            (0..crate::equip_stack::SLOTS)
                .filter_map(|i| crate::equip_stack::get(world, e, i))
                .filter(|s| t.prevents_equipment_drop(&loot_stack(s)))
                .map(|s| crate::items::item_id(&s.item))
                .collect()
        };
        let params = EntityLootParams { this, damage, player: None, luck: 0.0 };
        world.resource_scope(|w: &mut World, t: Mut<LootTables>| {
            let looting = |c: f32| if living { t.0.equipment_drop_chance(&params, c) } else { c };
            death_drops(w, e, true, killed, looting, |id| prevented.contains(&id), r)
        })
    } else {
        death_drops(world, e, true, killed, |c| c, |_| false, r)
    };
    let mut out = Vec::new();
    for (slot, id, wear) in dropped {
        let i = stack_slot(slot);
        let held = crate::equip_stack::get(world, e, i);
        crate::equip_stack::set(world, e, i, None);
        let Some(mut stack) = held.or_else(|| crate::items::item_name(id).map(|n| ItemStack::new(n, 1))) else { continue };
        if wear > 0 {
            stack.damage = wear as _;
        }
        out.push(stack);
    }
    out
}
/// `EnchantmentHelper.processMobExperience`: the `mob_experience` effects of the killer's equipment (the weapon `KillerKit` holds) applied
/// to `amount`. The killer is the causing entity of the last blow (`DamageSource.getEntity`).
fn processed_by_killer(world: &mut World, e: Entity, amount: u32) -> u32 {
    if !tables(world) {
        return amount;
    }
    let Some(killer) = damage_of(world, e).and_then(|d| d.source) else { return amount };
    let Some(mob) = entity_for_loot(world, e) else { return amount };
    let t = &world.resource::<LootTables>().0;
    t.mob_experience(&killer, mob, amount as i32).max(0) as u32
}
/// The death of `e`: its hand, then its table, on the ground where it died. Run before the entity goes.
/// The item names it left on the ground, in order (empty when nothing drops).
pub fn on_death(world: &mut World, e: Entity) -> Vec<&'static str> {
    let Some(at) = world.get::<Pos>(e).map(|p| p.0) else { return Vec::new() };
    // A bench world drops nothing (`orb::NoXp`): a bench player never picks anything up, and the stacks and orbs left among the herd cost
    // each tick for thousands of ticks (items and orbs that no player collects), where a game collects them within seconds.
    if world.get_resource::<crate::orb::NoXp>().is_some() {
        crate::size::split_on_death(world, e);
        return Vec::new();
    }
    let Some(kind) = world.get::<TypeName>(e).map(|t| t.0) else { return Vec::new() };
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
    let table = tables(world).then(|| world.resource::<LootTables>().0.clone());
    // `dropEquipment`: a stack the table prevents from dropping (a vanishing curse) stays with the mount.
    out.extend(crate::mount_inventory::death_stacks(world, e).into_iter().filter(|s| !table.as_ref().is_some_and(|t| t.prevents_equipment_drop(&loot_stack(s)))));
    let uid = world.get::<crate::goals::Uid>(e).map_or(0, |u| u.0 as i32);
    let mut r = LegacyRandom::new(death_seed(seed, uid));
    // `shouldDropLoot`: the table and `dropCustomDeathLoot` (the equipment) need an adult (or a species that keeps its babies' loot) and
    // the `mob_drops` rule; `shouldDropExperience` is separate (a baby of the species that has `BabyDrops`).
    let mob_drops = crate::blocks::world_of(world).mob_drops;
    let kid = world.get::<BabyDrops>(e).copied();
    let drops = !baby || kid.is_some_and(|k| k.loot);
    let xp_drops = (!baby || kid.is_some()) && world.get::<NoXpDrop>(e).is_none();
    // `Mob.getBaseExperienceReward`: each slot but the saddle (the body armour counts) that holds a stack of drop chance 1 or less adds
    // `1 + nextInt(3)`; counted before the equipment is dropped.
    let chances = world.get::<crate::equipment_drop::DropChances>(e).map_or([crate::equipment_drop::DEFAULT; 8], |c| c.0);
    let held = [crate::goals::Slot::Main, crate::goals::Slot::Off, crate::goals::Slot::Head, crate::goals::Slot::Chest, crate::goals::Slot::Legs, crate::goals::Slot::Feet, crate::goals::Slot::Body]
        .into_iter()
        .filter(|s| crate::equip_stack::get(world, e, stack_slot(*s)).is_some_and(|x| x.count > 0) && chances[crate::equipment_drop::index(*s)] <= 1.0)
        .count();
    if drops && mob_drops {
        let (killer, luck) = crate::credit::player_of(world, e).map_or((None, 0.0), |id| {
            let (p, luck) = player_entity(world, e, id);
            (Some(p), luck)
        });
        let this = entity_for_loot(world, e).unwrap_or_else(|| LootEntity::new(kind));
        let damage = damage_of(world, e);
        out.extend(roll_params(world, EntityLootParams { this: this.clone(), damage: damage.clone(), player: killer, luck }, seed));
        out.extend(equipment_stacks(world, e, this, damage, &mut r));
        // `Piglin.dropCustomDeathLoot` (inside `shouldDropLoot`): the inventory goes whole, after the table.
        out.extend(crate::item_store::death_stacks(world, e));
    }
    let mut names = Vec::with_capacity(out.len());
    for stack in out {
        names.extend(crate::items::item_name(crate::items::item_id(&stack.item)));
        crate::item::spawn_at_location(world, stack, at, &mut r);
    }
    // What a body of a size leaves: the smaller ones.
    crate::size::split_on_death(world, e);
    // The experience (`dropExperience`): only while a player's blow is remembered, not for a baby that does not keep it
    // (`shouldDropExperience`), and not with `mob_drops` off. A jockey's reward replaces the species' one.
    let over = world.get::<RewardOverride>(e).map(|o| XpReward { min: o.0, max: o.0 });
    // `Slime`/`MagmaCube`: `xpReward` is the size.
    let sized = world.get::<crate::size::SizeRule>(e).filter(|r| r.p.xp_per_size).and_then(|_| world.get::<crate::size::Size>(e)).map(|s| XpReward { min: s.0, max: s.0 });
    if let Some(x) = over.or(sized).or_else(|| world.get::<XpReward>(e).copied()).filter(|_| xp_drops && mob_drops && crate::credit::killed_by_player(world, e)) {
        // Only a fixed `Mob.xpReward` takes the equipment's bonus: a drawn reward (`Animal`), `Hoglin`/`Piglin` and the jockey override it.
        let gear = over.is_none() && x.min == x.max && world.get::<FlatXp>(e).is_none();
        let mut amount = xp_amount(x.min, x.max, &mut r);
        if baby && let Some(k) = kid {
            amount = (amount as f32 * k.xp_scale) as u32;
        }
        if gear && amount > 0 {
            amount += (0..held).map(|_| 1 + r.next_int_bound(3).unwrap_or(0) as u32).sum::<u32>();
        }
        amount = processed_by_killer(world, e, amount);
        crate::orb::award(world, at, amount, &mut r);
    }
    names
}
