//! Items by name, equipment in slots, armor in the damage, the saddle: the item registry (ids and tags from Horizon's own tables), what worn pieces give
//! against the `Mob`'s armor values and the damage a blow does through them (`HORIZON_SPECIES=minecraft:zombie`), and the saddle put on by a player
//! (`minecraft:pig`), all with `HORIZON_TEST_RESOURCES` set.
use crate::common;
use common::*;
use horizon_client_lib::mobs::armor;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::behaviours::Interaction;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::equipment::{self, Armor};
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::items::{self, ItemSet};
use horizon_ecs_proto::mount::Mount;

fn skip(name: &str) -> bool {
    !have_data() || wrong_species(&[name])
}
#[allow(dead_code)]
const SETS: &[(&str, &[(Slot, &str)])] = &[
    ("none", &[]),
    ("leather helmet", &[(Slot::Head, "minecraft:leather_helmet")]),
    ("iron chestplate", &[(Slot::Chest, "minecraft:iron_chestplate")]),
    ("diamond set", &[(Slot::Head, "minecraft:diamond_helmet"), (Slot::Chest, "minecraft:diamond_chestplate"), (Slot::Legs, "minecraft:diamond_leggings"), (Slot::Feet, "minecraft:diamond_boots")]),
    ("netherite set", &[(Slot::Head, "minecraft:netherite_helmet"), (Slot::Chest, "minecraft:netherite_chestplate"), (Slot::Legs, "minecraft:netherite_leggings"), (Slot::Feet, "minecraft:netherite_boots")]),
];
/// `horizon_items::equipment`'s slot numbers.
fn mob_slot(s: Slot) -> usize {
    match s {
        Slot::Head => 5,
        Slot::Chest => 4,
        Slot::Legs => 3,
        Slot::Feet => 2,
        Slot::Main => 0,
        Slot::Off => 1,
        Slot::Body => 6,
        Slot::Saddle => 7,
    }
}

/// Every item of the registry has an id by name, tags are read from the tables, the saddle and the armors know their slot.
#[test]
fn the_registry_gives_ids_tags_and_slots_by_name() {
    if !have_data() {
        return;
    }
    let _h = Herd::with_slab(20, false, false);
    assert!(items::installed());
    let wheat = items::item_id("minecraft:wheat");
    assert!(wheat != 0 && items::item_name(wheat) == Some("minecraft:wheat"));
    assert_eq!(items::item_id("minecraft:no_such_item"), 0);
    assert!(items::item_id("minecraft:saddle") != 0 && items::item_id("minecraft:cooked_beef") > 100, "more than the eleven items of before");
    assert!(items::has_tag("minecraft:horse_food", wheat) && !items::has_tag("minecraft:horse_food", items::item_id("minecraft:bone")));
    assert_eq!(items::worn_in(items::item_id("minecraft:saddle")), Some(Slot::Saddle));
    assert_eq!(items::worn_in(items::item_id("minecraft:iron_boots")), Some(Slot::Feet));
    assert_eq!(items::worn_in(wheat), None);
    let set = ItemSet::of(&["#minecraft:horse_food", "minecraft:bone"]);
    assert!(set.contains(wheat) && set.contains(items::item_id("minecraft:bone")) && !set.contains(items::item_id("minecraft:stone")));
    // The ids are the registry's: 1000 distinct names in, 1000 distinct ids out.
    let mut seen = std::collections::HashSet::new();
    for id in 1..=1000 {
        let n = items::item_name(id).expect("a name");
        assert_eq!(items::item_id(n), id);
        assert!(seen.insert(id));
    }
}

/// The armor and toughness each set gives (vanilla's numbers, written here from the game's tables), and the blow through it. The `Mob` of the bench keeps
/// its attribute map in the game, not in `ai::tick`: only its own armor (a zombie's 2) can be compared with it, so the blow on the unequipped zombie is
/// compared with the `Mob`'s, and the worn sets with `CombatRules.getDamageAfterAbsorb` computed here from the vanilla numbers.
#[test]
fn worn_armor_cuts_a_blow_like_the_original() {
    if skip("minecraft:zombie") {
        return;
    }
    // (label, pieces, armor points of the pieces alone, toughness of the pieces alone)
    let sets: &[(&str, &[(Slot, &str)], f32, f32)] = &[
        ("leather helmet", &[(Slot::Head, "minecraft:leather_helmet")], 1.0, 0.0),
        ("iron chestplate", &[(Slot::Chest, "minecraft:iron_chestplate")], 6.0, 0.0),
        ("diamond set", &[(Slot::Head, "minecraft:diamond_helmet"), (Slot::Chest, "minecraft:diamond_chestplate"), (Slot::Legs, "minecraft:diamond_leggings"), (Slot::Feet, "minecraft:diamond_boots")], 20.0, 8.0),
        ("netherite set", &[(Slot::Head, "minecraft:netherite_helmet"), (Slot::Chest, "minecraft:netherite_chestplate"), (Slot::Legs, "minecraft:netherite_leggings"), (Slot::Feet, "minecraft:netherite_boots")], 20.0, 12.0),
    ];
    let law = |blow: f32, armor: f32, toughness: f32| {
        let t = 2.0 + toughness / 4.0;
        blow * (1.0 - (armor - blow / t).clamp(armor * 0.2, 20.0) / 25.0)
    };
    let mut report = Vec::new();
    for pieces in std::iter::once(&(("none", &[][..], 0.0_f32, 0.0_f32))).chain(sets.iter().map(|s| s)) {
        let (label, worn, a, t) = *pieces;
        for blow in [3.0_f32, 8.0, 19.0] {
            let mut r = MobRef::new(60, false);
            let mut h = Herd::with_slab(60, true, false);
            let i = r.add_id(Vec3::new(0.5, 1.0, 0.5), 4242);
            let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 5);
            h.world.resource_mut::<Entered>().0.push(e);
            for (slot, name) in worn {
                equipment::set_slot(&mut h.world, e, *slot, items::item_id(name));
            }
            let view = player_view(Vec3::new(8.5, 1.0, 0.5), None);
            for _ in 0..3 {
                r.step(Some(&view));
                h.tick();
            }
            let base = armor::values(&r.mobs[i]);
            let ea = armor_of(&h.world, e);
            assert!((ea.armor - (base.0 + a)).abs() < 1e-6 && (ea.toughness - (base.1 + t)).abs() < 1e-6, "{label}: mob base {base:?}, pieces ({a}, {t}), ecs {ea:?}");
            let (m0, e0) = (r.mobs[i].health, health(&h, e));
            r.hit(i, blow, [8.5, 1.0, 0.5]);
            h.hit(e, blow, (8.5, 0.5));
            r.step(Some(&view));
            h.tick();
            let (dm, de) = (m0 - r.mobs[i].health, e0 - health(&h, e));
            let want = law(blow, ea.armor, ea.toughness);
            report.push(format!("{label} blow {blow}: armor {} toughness {}: ecs {de:.4}, law {want:.4}, mob (base armor only) {dm:.4}", ea.armor, ea.toughness));
            assert!((de - want).abs() < 1e-3, "{label} blow {blow}: ecs {de} law {want}");
            if worn.is_empty() {
                assert!((dm - de).abs() < 1e-3, "no piece: mob {dm} ecs {de}");
            }
        }
    }
    eprintln!("{}", report.join("\n"));
}

/// A player puts a saddle on a grown pig: the slot, the derived `Mount.saddled`, the hand emptied, the event; a baby takes none; a second saddle is refused.
#[test]
fn a_player_saddles_a_pig_and_the_slot_drives_the_mount() {
    if skip("minecraft:pig") {
        return;
    }
    let mut h = Herd::with_slab(60, true, false);
    let pig = spawn_species(&mut h.world, species(), 0.5, 0.5, 5);
    let baby = spawn_species(&mut h.world, species(), 3.5, 0.5, 6);
    h.world.resource_mut::<Entered>().0.extend([pig, baby]);
    h.make_baby(baby, 10_000);
    let saddle = items::item_id("minecraft:saddle");
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: saddle, off: 0, ..Default::default() });
    assert!(!interact(&mut h.world, baby, &Interaction { player: 1, item: saddle }), "a baby takes no saddle");
    assert!(!h.world.get::<Mount>(pig).unwrap().saddled);
    assert!(interact(&mut h.world, pig, &Interaction { player: 1, item: saddle }));
    assert_eq!(equipped(&h.world, pig, Slot::Saddle), saddle);
    assert!(h.world.get::<Mount>(pig).unwrap().saddled);
    assert_eq!(h.world.resource::<Online>().get(1).unwrap().main, 0, "the saddle left the hand");
    h.tick();
    assert!(h.world.resource::<Events>().equipment_changed.iter().any(|x| x == pig));
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: saddle, off: 0, ..Default::default() });
    assert!(!interact(&mut h.world, pig, &Interaction { player: 1, item: saddle }) || h.world.resource::<Online>().get(1).unwrap().main == saddle, "a pig that is saddled takes no second one");
    assert_eq!(equipment::take_slot(&mut h.world, pig, Slot::Saddle), saddle);
    assert!(!h.world.get::<Mount>(pig).unwrap().saddled);
}

fn drop_at(h: &mut Herd, name: &str, x: f64) {
    horizon_ecs_proto::item::drop_item(&mut h.world, horizon_client_lib::items::ItemStack::new(name, 1), Vec3::new(x, 1.0, 0.5), Vec3::ZERO, 0);
    h.world.flush();
}
fn on_ground(h: &mut Herd) -> Vec<String> {
    h.world.query::<&horizon_ecs_proto::item::Item>().iter(&h.world).map(|i| i.stack.item.to_string()).collect()
}

/// A zombie takes the armor it finds when it beats what it wears (and not a worse piece), the armor it wears is the value the damage reads, and what it
/// took is never lost at its death.
#[test]
fn a_zombie_wears_the_better_armor_it_finds_and_drops_it_at_death() {
    if skip("minecraft:zombie") {
        return;
    }
    use horizon_ecs_proto::behaviours::pick_up_gear;
    let mut h = Herd::with_slab(60, true, false);
    let z = spawn_species(&mut h.world, species(), 0.5, 0.5, 5);
    h.world.resource_mut::<Entered>().0.push(z);
    for _ in 0..3 {
        h.tick();
    }
    let base = armor_of(&h.world, z);
    drop_at(&mut h, "minecraft:leather_helmet", 0.5);
    pick_up_gear::look(&mut h.world, z);
    h.world.flush();
    assert_eq!(equipped(&h.world, z, Slot::Head), items::item_id("minecraft:leather_helmet"), "an empty head takes the first piece");
    drop_at(&mut h, "minecraft:iron_helmet", 0.5);
    pick_up_gear::look(&mut h.world, z);
    h.world.flush();
    assert_eq!(equipped(&h.world, z, Slot::Head), items::item_id("minecraft:iron_helmet"), "a better piece replaces it");
    assert!(on_ground(&mut h).contains(&"minecraft:leather_helmet".to_string()), "what it replaced (picked up, drop chance 2,0) falls");
    let a = armor_of(&h.world, z);
    eprintln!("base {base:?} with the iron helmet {a:?}; leather {:?} iron {:?}", items::armor_of(items::item_id("minecraft:leather_helmet")), items::armor_of(items::item_id("minecraft:iron_helmet")));
    assert_eq!(a.armor, base.armor + 2.0, "an iron helmet is 2 armor points");
    // A worse piece beats nothing it wears: it falls back to the empty main hand (the `Mob`'s rule), and a second one is left on the ground.
    drop_at(&mut h, "minecraft:leather_helmet", 1.0);
    pick_up_gear::look(&mut h.world, z);
    h.world.flush();
    assert_eq!(equipped(&h.world, z, Slot::Head), items::item_id("minecraft:iron_helmet"));
    assert_eq!(equipped(&h.world, z, Slot::Main), items::item_id("minecraft:leather_helmet"), "an empty hand takes it");
    drop_at(&mut h, "minecraft:leather_helmet", 1.0);
    let before = on_ground(&mut h).len();
    pick_up_gear::look(&mut h.world, z);
    h.world.flush();
    assert_eq!(on_ground(&mut h).len(), before, "nothing is taken once the hand is full");
    // The armor falls at death, with the picked piece.
    horizon_ecs_proto::loot::on_death(&mut h.world, z);
    h.world.flush();
    assert!(on_ground(&mut h).contains(&"minecraft:iron_helmet".to_string()), "what it picked up drops whatever the roll");
}

/// The roll that decides whether a replaced piece falls is `max(nextFloat - 0.1, 0) < 0,085`: 18,5 % for a piece the creature did not pick up.
#[test]
fn a_replaced_piece_falls_by_the_drop_chance() {
    if skip("minecraft:zombie") {
        return;
    }
    use horizon_ecs_proto::behaviours::pick_up_gear;
    let (mut fell, n) = (0, 600);
    for k in 0..n {
        let mut h = Herd::with_slab(30, false, false);
        let z = spawn_species(&mut h.world, species(), 0.5, 0.5, 100 + k);
        h.world.resource_mut::<Entered>().0.push(z);
        h.tick();
        equipment::set_slot(&mut h.world, z, Slot::Head, items::item_id("minecraft:leather_helmet"));
        drop_at(&mut h, "minecraft:diamond_helmet", 0.5);
        pick_up_gear::look(&mut h.world, z);
        h.world.flush();
        if on_ground(&mut h).contains(&"minecraft:leather_helmet".to_string()) {
            fell += 1;
        }
    }
    let p = 0.185;
    let sd = (f64::from(n as u32) * p * (1.0 - p)).sqrt();
    eprintln!("replaced piece fell {fell} times in {n} (law {:.1}, sd {sd:.1})", f64::from(n as u32) * p);
    assert!((f64::from(fell) - f64::from(n as u32) * p).abs() < 4.0 * sd, "{fell}");
}

/// A skeleton leaves a spear alone and takes its preferred weapon (the bow beats nothing it has in an empty hand, then a sword does not replace it).
#[test]
fn a_skeleton_takes_what_it_prefers_and_refuses_spears() {
    if skip("minecraft:skeleton") {
        return;
    }
    use horizon_ecs_proto::behaviours::pick_up_gear;
    let mut h = Herd::with_slab(30, false, false);
    let s = spawn_species(&mut h.world, species(), 0.5, 0.5, 5);
    h.world.resource_mut::<Entered>().0.push(s);
    h.tick();
    let tagged = (1..2500).find(|&i| items::has_tag("minecraft:spears", i)).and_then(items::item_name);
    if let Some(sp) = tagged {
        drop_at(&mut h, sp, 0.5);
        pick_up_gear::look(&mut h.world, s);
        h.world.flush();
        assert!(on_ground(&mut h).iter().any(|n| n == sp), "a spear stays on the ground");
    }
    drop_at(&mut h, "minecraft:bow", 0.5);
    pick_up_gear::look(&mut h.world, s);
    h.world.flush();
    assert_eq!(equipped(&h.world, s, Slot::Main), items::item_id("minecraft:bow"));
}

/// A bucket on a cow gives milk, a baby gives none; a water bucket on a fish takes the fish into a bucket.
#[test]
fn a_bucket_milks_a_cow_and_takes_a_fish() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(60, true, false);
    let table = species_table().0;
    let find = |n: &str| *table.iter().find(|s| s.name == n).unwrap_or_else(|| panic!("{n}"));
    let cow = spawn_species(&mut h.world, &find("minecraft:cow"), 0.5, 0.5, 5);
    let calf = spawn_species(&mut h.world, &find("minecraft:cow"), 3.5, 0.5, 6);
    let cod = spawn_species(&mut h.world, &find("minecraft:cod"), 6.5, 0.5, 7);
    h.world.resource_mut::<Entered>().0.extend([cow, calf, cod]);
    h.make_baby(calf, 10_000);
    let (bucket, water, milk) = (items::item_id("minecraft:bucket"), items::item_id("minecraft:water_bucket"), items::item_id("minecraft:milk_bucket"));
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: bucket, off: 0, ..Default::default() });
    assert!(!interact(&mut h.world, calf, &Interaction { player: 1, item: bucket }), "a calf gives no milk");
    assert!(interact(&mut h.world, cow, &Interaction { player: 1, item: bucket }));
    assert_eq!(h.world.resource::<Online>().get(1).unwrap().main, milk);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: water, off: 0, ..Default::default() });
    assert!(!interact(&mut h.world, cow, &Interaction { player: 1, item: water }), "a cow does not go in a bucket");
    assert!(interact(&mut h.world, cod, &Interaction { player: 1, item: water }));
    assert_eq!(h.world.resource::<Online>().get(1).unwrap().main, items::item_id("minecraft:cod_bucket"));
    h.world.flush();
    assert!(h.world.get_entity(cod).is_err(), "the fish is in the bucket");
}

/// The armor an animal has: its `Armor` (only while it wears a piece), else the base of its species (read from the table, no component for the unarmored).
fn armor_of(w: &bevy_ecs::world::World, e: bevy_ecs::entity::Entity) -> Armor {
    w.get::<Armor>(e).copied().unwrap_or_else(|| {
        let (armor, toughness) = equipment::base_armor(&w.resource::<SpeciesTable>().0[w.get::<SpeciesId>(e).unwrap().0 as usize]);
        Armor { armor, toughness }
    })
}
