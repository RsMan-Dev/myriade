//! Lot 13 of the villager rules (the old `Mob` tests): the professions and their job sites, the eight stacking slots, the half
//! stacks thrown, the golem window, the gossip day and wait, the food values and the offers of a new profession. Vanilla 26.3 values
//! (`javap` on `Villager`, `TradeWithVillager`, `GossipContainer`).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, Tag};
use horizon_ecs_proto::behaviours::bed::{self, Sleeping};
use horizon_ecs_proto::behaviours::food_inventory::{self, Stock, SLOTS};
use horizon_ecs_proto::behaviours::food_stock::Food;
use horizon_ecs_proto::behaviours::golem_recent::GOLEM_RECENT;
use horizon_ecs_proto::behaviours::summon_defender::{self, LastSlept};
use horizon_ecs_proto::behaviours::trade_offers::{self, Merchant};
use horizon_ecs_proto::behaviours::unhappy;
use horizon_ecs_proto::brain::{self, set, MemValue};
use horizon_ecs_proto::cow::{spawn_species, ActiveUntil, GrowAt, Herd, Pos, SpeciesTable};
use horizon_ecs_proto::goals::{Entered, Events};
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::nbt;
use horizon_ecs_proto::species::villager::POTENTIAL_JOB_SITE;
use horizon_ecs_proto::rules::gossip::{self, Kind};
use horizon_ecs_proto::rules::trade_with_villager;
use horizon_ecs_proto::species::villager::JOB_PROFESSIONS;
use horizon_world::chunk::BlockState;
use horizon_world::poi::saved_poi;

/// A villager of the species table, active for the whole test.
fn villager(h: &mut Herd) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:villager").unwrap();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 1);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    e
}

/// The vanilla professions of the trade table by row (`none` is row 0, `weaponsmith` the last), and the job site of each.
#[test]
fn vanilla_profession_table_and_job_sites() {
    if !have_data() {
        return;
    }
    let h = Herd::with_slab_awake(8, true, false);
    let names = [
        "none", "armorer", "butcher", "cartographer", "cleric", "farmer", "fisherman", "fletcher", "leatherworker", "librarian",
        "mason", "nitwit", "shepherd", "toolsmith", "weaponsmith",
    ];
    for (row, name) in names.iter().enumerate() {
        assert_eq!(trade_offers::profession_name(&h.world, row as u8), *name);
    }
    let sites = [
        ("blast_furnace", "armorer"),
        ("smoker", "butcher"),
        ("cartography_table", "cartographer"),
        ("brewing_stand", "cleric"),
        ("composter", "farmer"),
        ("barrel", "fisherman"),
        ("fletching_table", "fletcher"),
        ("cauldron", "leatherworker"),
        ("water_cauldron", "leatherworker"),
        ("lava_cauldron", "leatherworker"),
        ("powder_snow_cauldron", "leatherworker"),
        ("lectern", "librarian"),
        ("stonecutter", "mason"),
        ("loom", "shepherd"),
        ("smithing_table", "toolsmith"),
        ("grindstone", "weaponsmith"),
    ];
    for (site, name) in sites {
        let state = BlockState { name: format!("minecraft:{site}"), properties: Default::default() };
        let poi = saved_poi(&state).unwrap();
        assert_eq!(poi.id(), format!("minecraft:{name}"));
        assert_eq!(JOB_PROFESSIONS.iter().find(|(p, _)| *p == poi).map(|(_, n)| *n), Some(name));
    }
}

/// Eight kinds fill the eight slots; a later stack of a kind already held still merges, and a ninth kind is refused.
#[test]
fn a_later_stack_still_merges_into_a_full_set_of_slots() {
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = villager(&mut h);
    h.world.entity_mut(e).insert(Stock(Vec::new()));
    for k in ["stone", "dirt", "cobblestone", "oak_planks", "sand", "gravel", "oak_log", "glass"] {
        assert_eq!(food_inventory::insert(&mut h.world, e, &format!("minecraft:{k}"), 1), 0);
    }
    assert_eq!(h.world.get::<Stock>(e).unwrap().0.len(), SLOTS);
    assert_eq!(food_inventory::insert(&mut h.world, e, "minecraft:stone", 1), 0);
    assert_eq!(h.world.get::<Stock>(e).unwrap().0.len(), SLOTS);
    assert_eq!(food_inventory::insert(&mut h.world, e, "minecraft:ender_pearl", 1), 1);
}

/// Eight slots of 64 at most: 100 bread take two slots, six more kinds fill the rest, a ninth kind is refused and the same kind tops
/// up its stack, the 12 left over.
#[test]
fn the_inventory_has_eight_stacking_slots() {
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = villager(&mut h);
    h.world.entity_mut(e).insert(Stock(Vec::new()));
    assert_eq!(food_inventory::insert(&mut h.world, e, "minecraft:bread", 100), 0);
    assert_eq!(h.world.get::<Stock>(e).unwrap().0.len(), 2);
    assert_eq!(h.world.get::<Stock>(e).unwrap().0[0].1, 64);
    for i in 0..6 {
        assert_eq!(food_inventory::insert(&mut h.world, e, &format!("minecraft:item_{i}"), 1), 0);
    }
    assert_eq!(h.world.get::<Stock>(e).unwrap().0.len(), SLOTS);
    assert_eq!(food_inventory::insert(&mut h.world, e, "minecraft:stick", 1), 1);
    assert_eq!(food_inventory::insert(&mut h.world, e, "minecraft:bread", 40), 12);
}

/// `throwHalfStack`: a stack over half a stack (32) gives its half; a stack over 24 gives what is over 24; a smaller one stays.
#[test]
fn half_stacks_are_thrown() {
    assert!(!trade_with_villager::is_thrown(10, 64));
    assert!(!trade_with_villager::is_thrown(24, 64));
    assert!(trade_with_villager::is_thrown(30, 64));
    assert_eq!(trade_with_villager::thrown_count(30, 64), 6);
    assert!(trade_with_villager::is_thrown(50, 64));
    assert_eq!(trade_with_villager::thrown_count(50, 64), 25);
}

/// A golem is wanted only after a sleep of under 24000 ticks, and never while a golem was seen recently.
#[test]
fn golems_need_sleep_and_no_golem_nearby() {
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = villager(&mut h);
    assert!(!summon_defender::wants(&h.world, e, 100));
    h.world.entity_mut(e).insert(LastSlept(0));
    assert!(summon_defender::wants(&h.world, e, 23_999));
    assert!(!summon_defender::wants(&h.world, e, 24_000));
    h.world.entity_mut(e).insert(LastSlept(1000));
    set(&mut h.world, e, &GOLEM_RECENT, MemValue::Unit, Some(600));
    assert!(!summon_defender::wants(&h.world, e, 1000));
}

/// The gossip decays once a day (`DAY` ticks after the last decay), and a heard gossip waits `HEARD_WAIT` ticks before it is heard again.
#[test]
fn gossip_decays_once_a_day_and_heard_gossip_waits() {
    assert!(!gossip::day_due(100, 24_099));
    assert!(gossip::day_due(100, 24_100));
    assert_eq!(gossip::decayed_value(25, Kind::MajorNegative), 15);
    assert!(!gossip::free_to_hear(5000, 5100));
    assert!(gossip::free_to_hear(5000, 6200));
    assert!(gossip::free_to_hear(5000, 4000));
}

/// The food points of the villager's items: bread 4, beetroot 1, apple 0 (not food for a villager).
#[test]
fn villager_food_values() {
    if !have_data() {
        return;
    }
    let (items, _, _) = registries();
    assert_eq!(items.villager_food("minecraft:bread").unwrap_or(0), 4);
    assert_eq!(items.villager_food("minecraft:beetroot").unwrap_or(0), 1);
    assert_eq!(items.villager_food("minecraft:apple").unwrap_or(0), 0);
}

/// A new profession replaces the offers: the old ones are cleared, and the new profession rolls its own.
#[test]
fn a_new_profession_rolls_new_offers() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = villager(&mut h);
    h.world.entity_mut(e).insert(Merchant::new(false, 5, 0));
    trade_offers::ensure_offers(&mut h.world, e);
    assert!(!h.world.get::<Merchant>(e).unwrap().trade.offers.is_empty());
    trade_offers::set_profession(&mut h.world, e, 1);
    assert!(h.world.get::<Merchant>(e).unwrap().trade.offers.is_empty());
    trade_offers::ensure_offers(&mut h.world, e);
    assert!(!h.world.get::<Merchant>(e).unwrap().trade.offers.is_empty());
}

/// A villager that loses its job site has no profession, hence no offers; a new job site gives it the trades of the new profession.
#[test]
fn a_villager_that_loses_its_job_site_offers_its_new_profession_trades() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = villager(&mut h);
    h.world.entity_mut(e).insert(Merchant::new(false, 5, 0));
    trade_offers::ensure_offers(&mut h.world, e);
    trade_offers::set_profession(&mut h.world, e, 0);
    assert!(h.world.get::<Merchant>(e).unwrap().trade.offers.is_empty());
    trade_offers::set_profession(&mut h.world, e, 13);
    trade_offers::ensure_offers(&mut h.world, e);
    assert_eq!(trade_offers::profession_name(&h.world, 13), "toolsmith");
    assert!(!h.world.get::<Merchant>(e).unwrap().trade.offers.is_empty());
}

/// The no sound of a refused trade is at the voice pitch: 1 ± 0,2 as an adult, 1,5 ± 0,2 as a baby (`GrowAt`).
#[test]
fn the_unhappy_sound_uses_the_voice_pitch() {
    for (baby, range) in [(false, 0.8..=1.2), (true, 1.3..=1.7)] {
        let mut h = Herd::with_slab_awake(8, true, false);
        let e = villager(&mut h);
        h.world.entity_mut(e).insert(Merchant::new(false, 5, 0));
        if baby {
            h.world.entity_mut(e).insert(GrowAt(u64::MAX));
        }
        unhappy::say_no(&mut h.world, e);
        let pitches: Vec<f32> = h
            .world
            .resource_mut::<Events>()
            .host
            .drain()
            .filter_map(|ev| match ev {
                HostEvent::Sound { pitch, .. } => Some(pitch),
                _ => None,
            })
            .collect();
        assert_eq!(pitches.len(), 1);
        assert!(range.contains(&pitches[0]));
    }
}

/// `Villager.readAdditionalSaveData` reads `FoodLevel` with `getByteOr`: an int is narrowed (`(byte)(value & 255)`), so 300 is 44.
#[test]
fn a_villagers_food_level_is_narrowed_to_a_byte() {
    for (saved, expected) in [(Tag::Int(300), 44), (Tag::Int(200), -56), (Tag::Byte(12), 12)] {
        let mut h = Herd::with_slab_awake(8, true, false);
        let e = villager(&mut h);
        h.world.entity_mut(e).insert(Food(0));
        let mut c = Compound::new();
        c.insert("FoodLevel".into(), saved);
        nbt::load(&mut h.world, e, &c);
        assert_eq!(h.world.get::<Food>(e).unwrap().0, expected);
    }
}

/// The brain memories of a villager are saved as `Brain.memories`: the golem seen (`value` 1, `ttl` left), the last sleep and the
/// potential job site with its dimension and block. The golem's and the site's ttl are the ticks left.
#[test]
fn villager_memory_nbt_shape() {
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = villager(&mut h);
    set(&mut h.world, e, &GOLEM_RECENT, MemValue::Unit, Some(42));
    h.world.entity_mut(e).insert(LastSlept(101));
    set(&mut h.world, e, &POTENTIAL_JOB_SITE, MemValue::Block([4, 5, 6]), Some(9));
    let expected = Tag::compound([(
        "memories",
        Tag::compound([
            ("minecraft:golem_detected_recently", Tag::compound([("value", Tag::Byte(1)), ("ttl", Tag::Long(42))])),
            ("minecraft:last_slept", Tag::compound([("value", Tag::Long(101))])),
            (
                "minecraft:potential_job_site",
                Tag::compound([
                    ("value", Tag::compound([("dimension", Tag::String("minecraft:overworld".into())), ("pos", Tag::IntArray(vec![4, 5, 6]))])),
                    ("ttl", Tag::Long(9)),
                ]),
            ),
        ]),
    )]);
    assert_eq!(nbt::get(&h.world, e, "Brain"), Some(expected));
}

/// The brain memories and `LastSlept` of a saved villager come back on a loaded one, with their ttl.
#[test]
fn villager_memory_round_trip() {
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = villager(&mut h);
    set(&mut h.world, e, &GOLEM_RECENT, MemValue::Unit, Some(42));
    h.world.entity_mut(e).insert(LastSlept(101));
    set(&mut h.world, e, &POTENTIAL_JOB_SITE, MemValue::Block([4, 5, 6]), Some(9));
    let saved = nbt::save(&h.world, e).unwrap();
    let f = villager(&mut h);
    nbt::load(&mut h.world, f, &saved);
    assert_eq!(brain::ttl_left(&h.world, f, &GOLEM_RECENT), Some(42));
    assert_eq!(h.world.get::<LastSlept>(f).unwrap().0, 101);
    assert!(matches!(brain::get(&h.world, f, &POTENTIAL_JOB_SITE), Some(MemValue::Block(p)) if *p == [4, 5, 6]));
    assert_eq!(brain::ttl_left(&h.world, f, &POTENTIAL_JOB_SITE), Some(9));
}

/// Waking (the sleeping component goes) keeps the body where it stands: no position is moved to the bed. Vanilla
/// `LivingEntity.stopSleeping` only sets the pose to standing, moves to the saved position and clears the sleeping position.
#[test]
fn waking_faces_the_bed_and_resets_pitch_without_moving_the_chosen_position() {
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = villager(&mut h);
    bed::lie_down(&mut h.world, e, [2, 200, 6]);
    let before = h.world.get::<Pos>(e).unwrap().0;
    h.world.entity_mut(e).remove::<Sleeping>();
    assert!(!h.world.entity(e).contains::<Sleeping>());
    assert_eq!(h.world.get::<Pos>(e).unwrap().0, before);
}

/// A sleeper is saved asleep in its bed (`sleeping_pos`), and a villager that is awake writes none.
#[test]
fn a_sleeper_is_saved_asleep_in_its_bed() {
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = villager(&mut h);
    h.world.entity_mut(e).insert(Sleeping([3, 64, -7]));
    let saved = nbt::save(&h.world, e).unwrap();
    assert_eq!(saved.get(&"sleeping_pos".into()), Some(&Tag::IntArray(vec![3, 64, -7])));
    let f = villager(&mut h);
    nbt::load(&mut h.world, f, &saved);
    assert_eq!(h.world.get::<Sleeping>(f).unwrap().0, [3, 64, -7]);
    h.world.entity_mut(e).remove::<Sleeping>();
    let awake = nbt::save(&h.world, e).unwrap();
    assert!(!awake.contains_key(&"sleeping_pos".into()));
}
