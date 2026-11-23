//! Lot manquant B, the Mob's `finalize_load_tests.rs` (the attribute instances a load leaves) and the wolf's armour repair: the owner puts an
//! armadillo scute on the worn armour while the wolf sits. Expected values are vanilla 26.3's (`/summon` saved `attributes`, `Wolf.mobInteract`).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::attr_mod::{self, AttrMods, Modifier, Operation};
use horizon_ecs_proto::behaviours::neutral_anger::Anger;
use horizon_ecs_proto::behaviours::sit_when_ordered::SitPose;
use horizon_ecs_proto::behaviours::Interaction;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::effects::{self, Kind};
use horizon_ecs_proto::equip_stack;
use horizon_ecs_proto::goals::{interact, Online, Player};
use horizon_ecs_proto::items;
use horizon_ecs_proto::nbt::{self, Saved};
use horizon_ecs_proto::player_uuids::join;
use horizon_ecs_proto::tame;
use horizon_ecs_proto::targeting::Who;

/// The uuid of the player 1 the owners name: a player's owner is saved by its uuid, which the player table must know.
const UUID: u128 = 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210;

fn spawn(h: &mut Herd, name: &str, x: f64, z: f64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    spawn_species(&mut h.world, &sp, x, z, 5)
}

/// The written base of attribute `name` of `e` (`None` where the species' base applies).
fn base_of(w: &World, e: Entity, name: &str) -> Option<f64> {
    w.get::<AttrMods>(e)?.0.iter().find(|x| x.0 == name).map(|x| x.1).filter(|b| !b.is_nan())
}

/// Saves a spawned `name` after `setup`, then loads the tag into a fresh spawn (a baby when `baby`) and reads its base of `attr`.
fn loaded_base(name: &str, attr: &str, baby: bool, setup: impl FnOnce(&mut World, Entity)) -> Option<f64> {
    let mut h = Herd::with_slab_awake(20, false, false);
    join(&mut h.world, 1, UUID);
    let e = spawn(&mut h, name, 0.5, 0.5);
    h.world.entity_mut(e).insert(Saved);
    setup(&mut h.world, e);
    let c = nbt::save(&h.world, e).unwrap();
    let mut g = Herd::with_slab_awake(20, false, false);
    join(&mut g.world, 1, UUID);
    let f = spawn(&mut g, name, 0.5, 0.5);
    g.world.entity_mut(f).insert(Saved);
    if baby {
        g.world.entity_mut(f).insert(GrowAt(1 << 40));
    }
    assert!(nbt::load(&mut g.world, f, &c).is_some(), "{name} did not load");
    base_of(&g.world, f, attr)
}

/// `Enderman.setTarget` reads movement_speed, and the load ends with it (`readPersistentAngerSaveData`), with or without saved anger.
#[test]
fn an_enderman_load_makes_its_speed_instance() {
    if !have_data() || wrong_species(&["minecraft:enderman"]) {
        return;
    }
    for angry in [false, true] {
        let got = loaded_base("minecraft:enderman", "movement_speed", false, |w, e| {
            if angry {
                w.get_mut::<Anger>(e).unwrap().until = 5000;
            }
        });
        assert_eq!(got, Some(f64::from(0.3f32)), "angry {angry}");
    }
}

/// `TamableAnimal.readAdditionalSaveData` with no owner: `setTame(false, true)` -> `Wolf.applyTamingSideEffects` sets 8.
#[test]
fn a_wolf_loaded_without_an_owner_makes_its_health_instance() {
    if !have_data() || wrong_species(&["minecraft:wolf"]) {
        return;
    }
    assert_eq!(loaded_base("minecraft:wolf", "max_health", false, |_, _| {}), Some(8.0));
}

/// `setTame(true, false)` on load: the owner alone, so no instance is made, and a saved base stays.
#[test]
fn a_wolf_loaded_with_an_owner_keeps_the_saved_max_health() {
    if !have_data() || wrong_species(&["minecraft:wolf"]) {
        return;
    }
    let owned = |w: &mut World, e: Entity| {
        w.entity_mut(e).insert(tame::Owner(Some(Who::Player(1))));
    };
    assert_eq!(loaded_base("minecraft:wolf", "max_health", false, owned), None);
    let saved = |w: &mut World, e: Entity| {
        w.entity_mut(e).insert((tame::Owner(Some(Who::Player(1))), AttrMods(vec![("max_health".to_owned(), 80.0, Vec::new())])));
    };
    assert_eq!(loaded_base("minecraft:wolf", "max_health", false, saved), Some(80.0));
}

/// `Wolf.applyTamingSideEffects` (`setTame(true, true)`) sets the base to 40 and saves it.
#[test]
fn a_wolf_tamed_in_game_saves_its_forty_health() {
    if !have_data() || wrong_species(&["minecraft:wolf"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:wolf", 0.5, 0.5);
    tame::tame(&mut h.world, e, Who::Player(1), 40.0);
    assert_eq!(base_of(&h.world, e, "max_health"), Some(40.0), "the tamed wolf did not save 40");
    // `setTame(false, true)` only runs at a load without an owner: the saved 40 is replaced by the wild wolf's 8 there.
    let wild = loaded_base("minecraft:wolf", "max_health", false, |w, e| {
        w.entity_mut(e).insert(AttrMods(vec![("max_health".to_owned(), 40.0, Vec::new())]));
    });
    assert_eq!(wild, Some(8.0), "the wolf loaded without an owner kept the taming base");
}

/// A wolf tamed in game before anything stored its max_health base: the maximum reads the 40 and a refresh keeps it there.
#[test]
fn a_wolf_with_no_stored_base_reads_forty_health_once_tamed() {
    if !have_data() || wrong_species(&["minecraft:wolf"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:wolf", 0.5, 0.5);
    assert_eq!(base_of(&h.world, e, "max_health"), None);
    tame::tame(&mut h.world, e, Who::Player(1), 40.0);
    assert_eq!(h.world.get::<MaxHealth>(e).unwrap().0, 40.0);
    attr_mod::touched(&mut h.world, e, "max_health");
    assert_eq!(h.world.get::<MaxHealth>(e).unwrap().0, 40.0);
}

/// Health Boost II is +8 on top of the base 40; when it ends the maximum is 40 again, and the health is not raised.
#[test]
fn a_wolf_tamed_under_health_boost_loses_the_bonus_when_it_ends() {
    if !have_data() || wrong_species(&["minecraft:wolf"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:wolf", 0.5, 0.5);
    effects::add(&mut h.world, e, Kind::HealthBoost, 1, 600);
    tame::tame(&mut h.world, e, Who::Player(1), 40.0);
    assert_eq!(h.world.get::<MaxHealth>(e).unwrap().0, 48.0);
    assert!(effects::remove(&mut h.world, e, Kind::HealthBoost));
    assert_eq!(h.world.get::<MaxHealth>(e).unwrap().0, 40.0);
    assert_eq!(h.world.get::<Health>(e).unwrap().0, 40.0);
}

/// `AgeableMob.setAge` crosses the baby boundary: a baby goat, hoglin or zoglin makes its attack instance; an adult makes none.
#[test]
fn a_baby_goat_or_hoglin_makes_its_attack_instance() {
    if !have_data() || wrong_species(&["minecraft:goat", "minecraft:hoglin", "minecraft:zoglin"]) {
        return;
    }
    for (name, baby_base) in [("minecraft:goat", 1.0), ("minecraft:hoglin", 0.5), ("minecraft:zoglin", 0.5)] {
        assert_eq!(loaded_base(name, "attack_damage", false, |_, _| {}), None, "{name} adult");
        assert_eq!(loaded_base(name, "attack_damage", true, |_, _| {}), Some(baby_base), "{name} baby");
    }
}

/// The species that have no load instance of their own keep their values; the zombie's speed is its species' own, as a load writes it.
#[test]
fn the_other_species_stay_as_they_were() {
    if !have_data() || wrong_species(&["minecraft:pig", "minecraft:cat", "minecraft:shulker", "minecraft:zombie"]) {
        return;
    }
    assert_eq!(loaded_base("minecraft:pig", "movement_speed", false, |_, _| {}), None);
    let owned = |w: &mut World, e: Entity| {
        w.entity_mut(e).insert(tame::Owner(Some(Who::Player(1))));
    };
    assert_eq!(loaded_base("minecraft:cat", "movement_speed", false, owned), None);
    assert_eq!(loaded_base("minecraft:shulker", "movement_speed", false, |_, _| {}), None);
    assert_eq!(loaded_base("minecraft:zombie", "movement_speed", false, |_, _| {}), Some(f64::from(0.23f32)));
}

/// A max_health modifier changes the value, never the saved base: 8 from the load of a wild wolf, the taming 40 of a tame one.
#[test]
fn a_wolf_with_a_health_modifier_keeps_its_taming_base() {
    if !have_data() || wrong_species(&["minecraft:wolf"]) {
        return;
    }
    for (tamed, base) in [(false, 8.0), (true, 40.0)] {
        let got = loaded_base("minecraft:wolf", "max_health", false, |w, e| {
            if tamed {
                tame::tame(w, e, Who::Player(1), 40.0);
            }
            let m = Modifier { id: "test:boost".to_owned(), amount: 10.0, operation: Operation::AddValue, permanent: false };
            let mut list = w.get::<AttrMods>(e).cloned().unwrap_or_default();
            match list.0.iter_mut().find(|x| x.0 == "max_health") {
                Some(x) => x.2.push(m),
                None => list.0.push(("max_health".to_owned(), f64::NAN, vec![m])),
            }
            w.entity_mut(e).insert(list);
            attr_mod::touched(w, e, "max_health");
        });
        assert_eq!(got, Some(base), "tamed {tamed}");
    }
}

fn species_named(name: &str) -> Option<Species> {
    species_table().0.into_iter().find(|s| s.name == name)
}
fn hold(h: &mut Herd, main: u32) {
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main, off: 0, ..Default::default() });
}
fn click(h: &mut Herd, e: Entity, item: u32) -> bool {
    interact(&mut h.world, e, &Interaction { player: 1, item })
}
fn main_of(h: &Herd) -> u32 {
    h.world.resource::<Online>().get(1).unwrap().main
}

/// `Wolf.mobInteract`: the owner puts the armour on, then an armadillo scute gives back an eighth of its durability (64 in vanilla), only
/// while the wolf sits; the scute is used up.
#[test]
fn a_wolf_repairs_its_armour_with_an_armadillo_scute_while_sitting() {
    if !have_data() || wrong_species(&["minecraft:wolf"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(sp) = species_named("minecraft:wolf") else { return };
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    tame::tame(&mut h.world, e, Who::Player(1), 40.0);
    let armour = items::item_id("minecraft:wolf_armor");
    let scute = items::item_id("minecraft:armadillo_scute");
    hold(&mut h, armour);
    assert!(click(&mut h, e, armour), "the owner did not put the armour on");
    let mut worn = equip_stack::get(&mut h.world, e, 6).expect("the armour is in the body slot");
    worn.damage = 20;
    equip_stack::set(&mut h.world, e, 6, Some(worn));
    hold(&mut h, scute);
    assert!(!click(&mut h, e, scute), "the wolf repaired its armour while not sitting");
    h.world.entity_mut(e).insert(SitPose);
    assert!(click(&mut h, e, scute), "the scute was refused while sitting");
    assert_eq!(main_of(&h), 0, "the scute is used up");
    assert_eq!(equip_stack::get(&mut h.world, e, 6).unwrap().damage, 12, "an eighth of 64 is given back");
}
