//! Règles fermées au lot 11 (restes de `finalize.rs` et `animal_brain.rs` de l'ancien Mob) : bornes et lecture des attributs, tirages
//! de naissance (panda), effets d'attribut, type de villageois par biome, jour d'Halloween. Référence = jar vanilla 26.3.
use crate::common::*;
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;
use horizon_core::nbt::Tag;
use horizon_core::random::{LegacyRandom, RandomSource};
use horizon_ecs_proto::{
    attr_mod::{self, touched, AttrMods, Modifier, Operation},
    behaviours::{spawn_draw::{Genes, VILLAGER_BY_BIOME}, variant::Variant, Interaction},
    brain::{self, behaviours::{make_love, play_dead::{is_playing_dead, PLAY_DEAD_TICKS}}, sensors::attackables::HAS_HUNTING_COOLDOWN, MemValue, HURT_BY_ENTITY},
    cow::{base_max_health, max_health, spawn_species, species_table, AttackDamage, GrowAt, Health, Herd, MaxHealth, Resist},
    effects::{self, Kind},
    equip_stack, equipment::set_slot, equipment_wear::hurt_armor, goals, items,
    goals::Slot,
    rules::spawn_draw::{roll, Ctx},
    species::{self, panda::GENE},
    spawn_gear::halloween_date,
    tame,
    targeting::Who,
};

fn spawn(w: &mut World, name: &str) -> Entity {
    let sp = species_table().0.into_iter().find(|s| s.name == name).expect("a known species");
    spawn_species(w, &sp, 0.5, 0.5, 1)
}

fn modifier(amount: f64, operation: Operation) -> Modifier {
    Modifier { id: "test:mod".to_owned(), amount, operation, permanent: true }
}

fn ctx(biome: &str) -> Ctx<'_> {
    Ctx { biome, biome_has: &|_| false, special: 0.0, baby: false }
}

/// The first seed whose first `next_int_bound(16)` is `want`: the panda's gene draw, fixed.
fn seed_drawing(want: i32) -> u64 {
    (0u64..).find(|s| LegacyRandom::new(*s).next_int_bound(16).ok() == Some(want)).unwrap()
}

#[test]
fn a_wolf_health_boost_keeps_its_base_and_its_health() {
    // The wild 8, the tamed 40 and a saved custom 80 stay the bases; the boost adds 8 at level 2, does not heal, and its end clamps.
    for (tamed, base) in [(false, 8.0), (true, 40.0)] {
        let mut a = Herd::new(0, 1.0, true, false, 1);
        let e = spawn(&mut a.world, "minecraft:wolf");
        if tamed {
            tame::tame(&mut a.world, e, Who::Player(1), base as f32);
        }
        effects::add(&mut a.world, e, Kind::HealthBoost, 1, 600);
        a.world.get_mut::<Health>(e).unwrap().0 = 4.0;
        assert_eq!(max_health(&a.world, e), base as f32 + 8.0);
        assert_eq!(a.world.get::<Health>(e).map(|h| h.0), Some(4.0), "adding Health Boost must not heal");
        assert_eq!(base_max_health(&a.world, e), base as f32);
        a.world.get_mut::<Health>(e).unwrap().0 = max_health(&a.world, e);
        effects::remove(&mut a.world, e, Kind::HealthBoost);
        assert_eq!(max_health(&a.world, e), base as f32);
        assert_eq!(a.world.get::<Health>(e).map(|h| h.0), Some(base as f32), "removal clamps to the unboosted maximum");
    }
    let mut b = Herd::new(0, 1.0, true, false, 1);
    let e = spawn(&mut b.world, "minecraft:wolf");
    b.world.entity_mut(e).insert(AttrMods(vec![("max_health".to_owned(), 80.0, vec![])]));
    touched(&mut b.world, e, "max_health");
    assert_eq!(base_max_health(&b.world, e), 80.0);
    assert_eq!(max_health(&b.world, e), 80.0);
}

#[test]
fn a_knockback_base_keeps_the_vanilla_range_and_max_health_stops_at_1024() {
    // RangedAttribute knockback_resistance is [-2, 1]: -1.5 stays. max_health is [1, 1024]: 5000 reads 1024.
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let e = spawn(&mut a.world, "minecraft:wolf");
    a.world.entity_mut(e).insert(AttrMods(vec![("knockback_resistance".to_owned(), -1.5, vec![])]));
    touched(&mut a.world, e, "knockback_resistance");
    assert_eq!(a.world.get::<Resist>(e).map(|r| r.0), Some(-1.5));
    a.world.entity_mut(e).insert(AttrMods(vec![("max_health".to_owned(), 5000.0, vec![])]));
    touched(&mut a.world, e, "max_health");
    assert_eq!(max_health(&a.world, e), 1024.0);
    assert_eq!(a.world.get::<MaxHealth>(e).map(|m| m.0), Some(1024.0));
}

#[test]
fn a_panda_gene_table_follows_the_vanilla_draw() {
    // The draw of 0, 3, 4, 9 and 15 out of 16: lazy, weak, aggressive, brown, normal.
    for (draw, want) in [(0, 1), (3, 5), (4, 6), (9, 4), (15, 0)] {
        let got = roll(&GENE, &ctx("minecraft:plains"), &mut LegacyRandom::new(seed_drawing(draw)));
        assert_eq!(got, want, "draw {draw}");
    }
    // The shown gene: a recessive main gene shows only when the hidden one matches it.
    use horizon_ecs_proto::behaviours::spawn_draw::{shown, Gene};
    assert_eq!(shown(Genes { main: Gene::Brown, hidden: Gene::Normal }), Gene::Normal);
    assert_eq!(shown(Genes { main: Gene::Brown, hidden: Gene::Brown }), Gene::Brown);
    assert_eq!(shown(Genes { main: Gene::Aggressive, hidden: Gene::Normal }), Gene::Aggressive);
}

#[test]
fn a_command_modifier_with_a_runtime_id_changes_the_value() {
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let e = spawn(&mut a.world, "minecraft:zombie");
    let base = a.world.get::<AttackDamage>(e).unwrap().0;
    a.world.entity_mut(e).insert(AttrMods(vec![("attack_damage".to_owned(), 4.0, vec![Modifier { id: "test:typed".to_owned(), amount: 3.0, operation: Operation::AddValue, permanent: true }])]));
    touched(&mut a.world, e, "attack_damage");
    assert_eq!(attr_mod::read(&a.world, e, "attack_damage", 4.0), 7.0);
    assert_eq!(a.world.get::<AttackDamage>(e).map(|d| d.0), Some(7.0));
    a.world.entity_mut(e).insert(AttrMods(vec![("attack_damage".to_owned(), 4.0, vec![])]));
    touched(&mut a.world, e, "attack_damage");
    assert_eq!(a.world.get::<AttackDamage>(e).map(|d| d.0), Some(4.0));
    assert_eq!(attr_mod::read(&a.world, e, "attack_damage", base as f64), base as f64);
}

#[test]
fn a_spawn_egg_offspring_is_a_baby() {
    // SpawnEggItem.spawnOffspringFromSpawnEgg: the cow's own egg makes a baby cow that keeps the variant; another egg does nothing.
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = spawn(&mut a.world, "minecraft:cow");
    a.world.entity_mut(cow).insert(Variant(3));
    assert!(!goals::interact(&mut a.world, cow, &Interaction { player: 1, item: items::item_id("minecraft:pig_spawn_egg") }));
    assert!(goals::interact(&mut a.world, cow, &Interaction { player: 1, item: items::item_id("minecraft:cow_spawn_egg") }));
    let babies: Vec<Entity> = a.world.query_filtered::<Entity, With<GrowAt>>().iter(&a.world).collect();
    assert_eq!(babies.len(), 1);
    assert_eq!(a.world.get::<Variant>(babies[0]).map(|v| v.0), Some(3));
}

#[test]
fn every_command_attribute_reaches_the_store_the_mob_reads() {
    // Each attribute a command names is read back from its base and its modifier, and the removal is seen at the next read.
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let e = spawn(&mut a.world, "minecraft:zombie");
    for name in ["max_health", "movement_speed", "follow_range", "attack_damage", "knockback_resistance", "spawn_reinforcements", "jump_strength"] {
        a.world.entity_mut(e).insert(AttrMods(vec![(name.to_owned(), 0.5, vec![modifier(0.25, Operation::AddValue)])]));
        assert_eq!(attr_mod::read(&a.world, e, name, 0.0), 0.75, "{name}");
        a.world.entity_mut(e).insert(AttrMods(vec![(name.to_owned(), 0.5, vec![])]));
        assert_eq!(attr_mod::read(&a.world, e, name, 0.0), 0.5, "{name} after removal");
    }
}

#[test]
fn a_set_base_survives_a_later_modifier_refresh() {
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let e = spawn(&mut a.world, "minecraft:zombie");
    a.world.entity_mut(e).insert(AttrMods(vec![
        ("movement_speed".to_owned(), 0.5, vec![modifier(0.5, Operation::AddMultipliedTotal)]),
        ("max_health".to_owned(), 30.0, vec![]),
        ("attack_damage".to_owned(), 7.0, vec![]),
    ]));
    touched(&mut a.world, e, "max_health");
    assert_eq!(attr_mod::read(&a.world, e, "movement_speed", 0.0), 0.75);
    a.world.entity_mut(e).insert(AttrMods(vec![
        ("movement_speed".to_owned(), 0.5, vec![]),
        ("max_health".to_owned(), 30.0, vec![]),
        ("attack_damage".to_owned(), 7.0, vec![]),
    ]));
    touched(&mut a.world, e, "max_health");
    assert_eq!(attr_mod::read(&a.world, e, "movement_speed", 0.0), 0.5);
    assert_eq!(attr_mod::read(&a.world, e, "attack_damage", 0.0), 7.0);
}

#[test]
fn weakness_slowness_and_health_boost_carry_attribute_modifiers() {
    // Amounts per level: weakness -4 (ADD_VALUE), slowness -0.15 (ADD_MULTIPLIED_TOTAL), health boost +4 (ADD_VALUE), scaled by amp + 1.
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let e = spawn(&mut a.world, "minecraft:zombie");
    effects::add(&mut a.world, e, Kind::Weakness, 1, 200);
    effects::add(&mut a.world, e, Kind::Slowness, 1, 200);
    effects::add(&mut a.world, e, Kind::HealthBoost, 0, 200);
    let weak = effects::attribute_modifiers(&a.world, e, "attack_damage");
    assert_eq!(weak.len(), 1);
    assert!((weak[0].amount + 8.0).abs() < 1e-9);
    assert_eq!(weak[0].operation, Operation::AddValue);
    let slow = effects::attribute_modifiers(&a.world, e, "movement_speed");
    assert_eq!(slow.len(), 1);
    assert!((slow[0].amount - f64::from(-0.15f32) * 2.0).abs() < 1e-6);
    assert_eq!(slow[0].operation, Operation::AddMultipliedTotal);
    let boost = effects::attribute_modifiers(&a.world, e, "max_health");
    assert_eq!(boost.len(), 1);
    assert!((boost[0].amount - 4.0).abs() < 1e-9);
    assert_eq!(boost[0].operation, Operation::AddValue);
    assert!(effects::attribute_modifiers(&a.world, e, "movement_speed").iter().all(|m| m.operation != Operation::AddValue));
}

#[test]
fn villager_types_by_biome() {
    // VillagerType.byBiome: badlands is desert (1), grove is snow (4), plains is the default (0).
    let draw = |biome: &str| roll(&VILLAGER_BY_BIOME, &ctx(biome), &mut LegacyRandom::new(1));
    assert_eq!(draw("minecraft:badlands"), 1);
    assert_eq!(draw("minecraft:grove"), 4);
    assert_eq!(draw("minecraft:plains"), 0);
}

#[test]
fn october_31_is_halloween() {
    assert!(halloween_date(10, 31));
    assert!(!halloween_date(10, 30));
    assert!(!halloween_date(11, 1));
    assert!(!halloween_date(1, 1));
}

#[test]
fn a_nan_calculated_value_is_the_minimum_of_its_range() {
    // RangedAttribute.sanitizeValue on the calculated value: NaN is the minimum (max_health 1, knockback -2).
    let nan = |name: &str| vec![(name.to_owned(), 30.0, vec![Modifier { id: "test:nan".to_owned(), amount: f64::NAN, operation: Operation::AddValue, permanent: true }])];
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let e = spawn(&mut a.world, "minecraft:wolf");
    a.world.entity_mut(e).insert(AttrMods(nan("max_health")));
    touched(&mut a.world, e, "max_health");
    assert_eq!(max_health(&a.world, e), 1.0);
    a.world.entity_mut(e).insert(AttrMods(vec![("knockback_resistance".to_owned(), 0.0, vec![Modifier { id: "test:nan".to_owned(), amount: f64::NAN, operation: Operation::AddValue, permanent: true }])]));
    touched(&mut a.world, e, "knockback_resistance");
    assert_eq!(a.world.get::<Resist>(e).map(|r| r.0), Some(-2.0));
}

#[test]
fn a_worn_piece_keeps_its_enchantments_and_trim_as_it_wears() {
    // A chestplate with thorns 3 and a trim: each hurt adds its wear to the damage; the enchantments and the trim stay.
    have_data();
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let e = spawn(&mut a.world, "minecraft:zombie");
    set_slot(&mut a.world, e, Slot::Chest, items::item_id("minecraft:iron_chestplate"));
    let mut chest = ItemStack::new("minecraft:iron_chestplate", 1);
    chest.data_mut().set_other("minecraft:enchantments", Some(Tag::String("minecraft:thorns=3".into())));
    chest.data_mut().set_other("minecraft:trim", Some(Tag::String("minecraft:gold|minecraft:coast".into())));
    let trim = chest.data().and_then(|d| d.other("minecraft:trim").cloned());
    equip_stack::set(&mut a.world, e, 4, Some(chest));
    hurt_armor(&mut a.world, e, 8.0);
    assert_eq!(equip_stack::get(&a.world, e, 4).map(|s| s.damage), Some(2));
    hurt_armor(&mut a.world, e, 20.0);
    let worn = equip_stack::get(&a.world, e, 4).expect("the chestplate is still worn");
    assert_eq!(worn.damage, 7);
    assert_eq!(worn.enchantment_level("minecraft:thorns"), 3);
    assert_eq!(worn.data().and_then(|d| d.other("minecraft:trim").cloned()), trim);
}

#[test]
fn an_axolotl_leaving_the_fight_takes_the_hunting_cooldown() {
    // A hurt while it fights sends the axolotl to play dead (priority over fight): the attack target goes, the hunting cooldown starts at 2400.
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let e = spawn(&mut a.world, "minecraft:axolotl");
    brain::set(&mut a.world, e, &brain::ATTACK_TARGET, MemValue::Who(Who::Player(1)), None);
    brain::set(&mut a.world, e, &PLAY_DEAD_TICKS, MemValue::Int(200), None);
    brain::set(&mut a.world, e, &HURT_BY_ENTITY, MemValue::Who(Who::Player(1)), None);
    a.tick();
    assert!(!brain::present(&a.world, e, &brain::ATTACK_TARGET));
    assert!(brain::present(&a.world, e, &HAS_HUNTING_COOLDOWN));
    assert!(is_playing_dead(&a.world, e));
    assert_eq!(brain::int(&a.world, e, &PLAY_DEAD_TICKS), Some(199));
}

#[test]
fn axolotl_breeding_uses_native_110_tick_timeout() {
    // AnimalMakeLove's timer is 110 ticks in vanilla 26.3 (jar, AnimalMakeLove: bipush 110); the axolotl's make_love entry keeps it.
    let love = species::axolotl::DEF.brain.behaviours.iter().find(|b| std::ptr::eq(b.behaviour, &make_love::BEHAVIOUR)).expect("the axolotl makes love");
    assert_eq!(love.duration, Some((110, 110)));
}

#[test]
fn an_axolotl_has_a_native_brain_and_no_fallback_goals() {
    // The axolotl's own brain (AxolotlAi, jar), no goal entry left over from the old Mob.
    assert!(!species::axolotl::DEF.brain.is_empty());
    assert!(species::axolotl::DEF.goals.is_empty());
}

#[test]
fn variant_components_use_the_jar_serialized_names() {
    // getSerializedName (jar): equine.Variant 0..6, Llama$Variant 0..3, MushroomCow$Variant red=0, brown=1.
    use horizon_ecs_proto::rules::variant::component_name;
    use horizon_ecs_proto::species::variants::{HORSE, LLAMA, MUSHROOM};
    assert_eq!(component_name(&HORSE, 0), Some("white"));
    assert_eq!(component_name(&HORSE, 6), Some("dark_brown"));
    assert_eq!(component_name(&LLAMA, 0), Some("creamy"));
    assert_eq!(component_name(&MUSHROOM, 1), Some("brown"));
    assert_eq!(component_name(&MUSHROOM, 2), None);
}

#[test]
fn a_mooshroom_variant_one_sheds_brown_mushrooms() {
    // MushroomCow$Variant (jar): RED=0 (DEFAULT), BROWN=1; the shear table follows the same index.
    let mut w = World::new();
    let red = w.spawn(Variant(0)).id();
    let brown = w.spawn(Variant(1)).id();
    assert_eq!((species::mooshroom::SHEARING.table)(&w, red).as_deref(), Some("minecraft:shearing/mooshroom/red"));
    assert_eq!((species::mooshroom::SHEARING.table)(&w, brown).as_deref(), Some("minecraft:shearing/mooshroom/brown"));
}
