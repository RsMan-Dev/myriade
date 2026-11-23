//! Lot 16b, the bee of the old Mob (`bee.rs`): the anger ends at once at Peaceful against a player, the crop sparks before it grows, the
//! sting poisons a player, universal anger turns a bee on the nearest player, and the anger is saved. Expected values are the old Mob's
//! tests and vanilla 26.3 (`NeutralMob.updatePersistentAnger`, `BeeGrowCropGoal`, `Bee.doHurtTarget`, `NeutralMob.isAngryAtAllPlayers`).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::nbt::{JavaString, Tag};
use horizon_ecs_proto::behaviours::bee::Bee;
use horizon_ecs_proto::behaviours::neutral_anger::{anger_at, is_angry, Anger};
use horizon_ecs_proto::blocks::int_property;
use horizon_ecs_proto::cow::{spawn_species, Herd, Level, Pos, SpeciesTable};
use horizon_ecs_proto::effects::{player_has, Kind};
use horizon_ecs_proto::hostile::PlayerState;
use horizon_ecs_proto::bridge::uuids::to_words;
use horizon_ecs_proto::nbt::{self, Saved};
use horizon_ecs_proto::player_uuids::join;
use horizon_ecs_proto::targeting::{set_target, Target, Who};

/// The uuid of the one player the saved anger names.
const UUID: u128 = 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210;

fn spawn(h: &mut Herd, name: &str, x: f64, z: f64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    spawn_species(&mut h.world, &sp, x, z, 5)
}

/// `NeutralMob.updatePersistentAnger`: at Peaceful the anger against a player ends at once, with its target and no countdown.
#[test]
fn peaceful_ends_a_bees_anger_at_the_player() {
    if !have_data() || wrong_species(&["minecraft:bee"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.set_online(&[(6.5, 0.5)]);
    let e = spawn(&mut h, "minecraft:bee", 0.5, 0.5);
    set_target(&mut h.world, e, Target::player(1, 20));
    h.tick();
    assert_eq!(anger_at(&h.world, e), Some(Who::Player(1)), "the bee is not angry at the player");
    h.world.resource_mut::<PlayerState>().peaceful = true;
    h.tick();
    assert!(!is_angry(&h.world, e), "peaceful: the bee kept its anger");
    assert!(h.world.get::<Target>(e).is_none(), "peaceful: the bee kept its player target");
}

/// `BeeGrowCropGoal`: a bee with nectar and a hive grows the wheat 1 block below it, the level event 2011 (data 15) before the block change
/// (issue 11377). The bee sits at y 3 over the wheat at y 2; its hive is 5 blocks away, so the bee has not entered it when the crop grows.
#[test]
fn a_bee_sparks_a_crop_before_growing_it() {
    if !have_data() || wrong_species(&["minecraft:bee"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:bee", 0.5, 0.5);
    h.set_block(5, 3, 5, "minecraft:beehive");
    h.set_block(0, 2, 0, "minecraft:wheat");
    h.tick();
    h.world.get_mut::<Pos>(e).unwrap().0.y = 3.0;
    {
        let mut b = h.world.get_mut::<Bee>(e).unwrap();
        b.nectar = true;
        b.hive = Some([5, 3, 5]);
    }
    for _ in 0..90 {
        h.tick();
    }
    let mw = h.world.resource::<Level>().mw.0;
    assert_eq!(int_property(mw, [0, 2, 0], "age"), Some(1), "the wheat did not grow one stage");
}

/// `Bee.doHurtTarget`: a sting that lands on a player poisons it for 10 seconds on Normal (200 ticks). The bee is set beside the player and
/// given time to reach it; the poison is timed from its first tick to its end.
#[test]
fn a_sting_on_a_player_poisons_it_for_200_ticks() {
    if !have_data() || wrong_species(&["minecraft:bee"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.set_online(&[(1.5, 0.5)]);
    let e = spawn(&mut h, "minecraft:bee", 0.5, 0.5);
    set_target(&mut h.world, e, Target::player(1, 20));
    let mut first = None;
    for t in 0..400 {
        h.tick();
        let on = player_has(&h.world, 1, Kind::Poison);
        match (first, on) {
            (None, true) => first = Some(t),
            (Some(s), false) => {
                assert_eq!(t - s, 200, "the poison lasted {} ticks, not 200", t - s);
                return;
            }
            _ => {}
        }
    }
    panic!("the sting did not poison the player for 200 ticks");
}

/// `NeutralMob.isAngryAtAllPlayers`: under universal anger a bee that no player hit is angry at the nearest player; without it, it is not.
#[test]
fn universal_anger_turns_a_bee_on_the_nearest_player() {
    if !have_data() || wrong_species(&["minecraft:bee"]) {
        return;
    }
    for (universal, angry) in [(false, false), (true, true)] {
        let mut h = Herd::with_slab_awake(20, false, false);
        h.set_online(&[(6.5, 0.5)]);
        let e = spawn(&mut h, "minecraft:bee", 0.5, 0.5);
        let mut level = *h.world.resource::<Level>();
        let rule: &'static MobWorld<'static> = Box::leak(Box::new(MobWorld { universal_anger: universal, ..level.mw.0.clone() }));
        level.mw = Mw(rule);
        *h.world.resource_mut::<Level>() = level;
        for _ in 0..5 {
            h.tick();
        }
        assert_eq!(anger_at(&h.world, e).is_some(), angry, "universal anger {universal}");
    }
}

/// `NeutralMob.addPersistentAngerSaveData`: the countdown is saved as `anger_end_time` and the target's uuid as `AngryAt`; both are read back.
#[test]
fn a_bees_anger_is_saved_and_loaded() {
    if !have_data() || wrong_species(&["minecraft:bee"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.set_online(&[(6.5, 0.5)]);
    join(&mut h.world, 1, UUID);
    let e = spawn(&mut h, "minecraft:bee", 0.5, 0.5);
    h.world.entity_mut(e).insert(Saved);
    {
        let mut a = h.world.get_mut::<Anger>(e).unwrap();
        a.holding = true;
        a.until = 1200;
        a.at = Some(Who::Player(1));
        a.at_player = true;
    }
    let c = nbt::save(&h.world, e).unwrap();
    assert_eq!(c.get(&JavaString::from("anger_end_time")), Some(&Tag::Long(1200)));
    assert_eq!(c.get(&JavaString::from("angry_at")), Some(&Tag::IntArray(to_words(UUID).to_vec())), "the target's uuid was not saved");
    {
        let mut a = h.world.get_mut::<Anger>(e).unwrap();
        a.holding = false;
        a.until = 0;
        a.at = None;
        a.at_player = false;
    }
    assert!(nbt::load(&mut h.world, e, &c).is_some(), "the saved bee did not load");
    let a = h.world.get::<Anger>(e).unwrap();
    assert_eq!((a.until, a.at), (1200, Some(Who::Player(1))), "the anger was not restored");
}

/// `NeutralMob`: a player who has not joined when the bee loads is waited for; the anger finds him when he joins with his uuid.
#[test]
fn a_bees_anger_finds_its_player_when_he_joins_after_the_load() {
    if !have_data() || wrong_species(&["minecraft:bee"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    join(&mut h.world, 1, UUID);
    let e = spawn(&mut h, "minecraft:bee", 0.5, 0.5);
    h.world.entity_mut(e).insert(Saved);
    h.world.get_mut::<Anger>(e).unwrap().at = Some(Who::Player(1));
    let c = nbt::save(&h.world, e).unwrap();
    let mut g = Herd::with_slab_awake(20, false, false);
    let f = spawn(&mut g, "minecraft:bee", 0.5, 0.5);
    g.world.entity_mut(f).insert(Saved);
    assert!(nbt::load(&mut g.world, f, &c).is_some(), "the saved bee did not load");
    assert_eq!(g.world.get::<Anger>(f).unwrap().at, None, "the player had not joined yet, yet the anger has a target");
    join(&mut g.world, 1, UUID);
    assert_eq!(g.world.get::<Anger>(f).unwrap().at, Some(Who::Player(1)), "the player who joined is not the anger's target");
}
