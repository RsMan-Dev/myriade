//! A zombie's `finalizeSpawn` gear: the armour and weapon draws grow with the difficulty's special multiplier, the Halloween pumpkin fills an empty
//! head, and a `Conversion` draws none of it.
use crate::common;
use common::*;
use horizon_core::random::LegacyRandom;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::{equipped, Slot};
use horizon_ecs_proto::spawn_finalize::{finalize, kind_of};
use horizon_ecs_proto::spawn_gear::Halloween;
use horizon_ecs_proto::spawn_reason::SpawnReason;

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:zombie"])
}
fn species_named(name: &str) -> Species {
    *species_table().0.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name}"))
}

fn geared(h: &mut Herd, reason: SpawnReason, special: f32, seeds: u64) -> (usize, usize) {
    let (mut worn, mut armed) = (0, 0);
    for seed in 0..seeds {
        let z = spawn_species(&mut h.world, &species_named("minecraft:zombie"), 0.5, 0.5, seed);
        let mut r = LegacyRandom::new(seed as i64);
        let kind = kind_of(&h.world, z);
        finalize(&mut h.world, z, kind, None, reason, special, &mut r);
        let slots = [Slot::Head, Slot::Chest, Slot::Legs, Slot::Feet];
        worn += usize::from(slots.iter().any(|s| equipped(&h.world, z, *s) != 0));
        armed += usize::from(equipped(&h.world, z, Slot::Main) != 0);
    }
    (worn, armed)
}

#[test]
fn zombie_gear_grows_with_the_difficulty_and_a_conversion_draws_none() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let (easy_worn, _) = geared(&mut h, SpawnReason::Natural, 0.75, 600);
    let (hard_worn, hard_armed) = geared(&mut h, SpawnReason::Natural, 1.5, 600);
    assert!(hard_worn > easy_worn, "more armour at the higher multiplier: {easy_worn} then {hard_worn}");
    assert!(hard_armed > 0, "some zombies get a weapon");
    assert_eq!(geared(&mut h, SpawnReason::Conversion, 1.5, 600), (0, 0), "a converted zombie keeps its own gear");
}

#[test]
fn halloween_puts_a_pumpkin_on_an_empty_head() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    h.world.insert_resource(Halloween(true));
    let mut pumpkins = 0;
    for seed in 0..400u64 {
        let z = spawn_species(&mut h.world, &species_named("minecraft:zombie"), 0.5, 0.5, seed);
        let mut r = LegacyRandom::new(seed as i64);
        let kind = kind_of(&h.world, z);
        let out = finalize(&mut h.world, z, kind, None, SpawnReason::Natural, 0.75, &mut r);
        pumpkins += out.gear.iter().filter(|(s, n)| *s == Slot::Head && (n.ends_with("pumpkin") || n.ends_with("jack_o_lantern"))).count();
    }
    assert!((40..160).contains(&pumpkins), "about a quarter of the heads, got {pumpkins}");
    let mut h = Herd::with_slab_awake(60, true, false);
    let z = spawn_species(&mut h.world, &species_named("minecraft:zombie"), 0.5, 0.5, 1);
    let mut r = LegacyRandom::new(1);
    let kind = kind_of(&h.world, z);
    let out = finalize(&mut h.world, z, kind, None, SpawnReason::Natural, 0.75, &mut r);
    assert!(!out.gear.iter().any(|(_, n)| n.contains("pumpkin")), "no pumpkin outside Halloween");
}
