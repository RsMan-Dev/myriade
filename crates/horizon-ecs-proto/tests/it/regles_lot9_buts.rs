//! Lot 9 of the rules of the old goals (`mobs/goals.rs`): a spectator does not tempt an animal (`TemptGoal`, `forNonCombat`), and an
//! invisible player tempts only within its scaled range (`TargetingConditions.test`, 2 blocks at visibility 0.1 for a range of 10).
//! Expected values are the vanilla 26.3 ones (`javap` of `TemptGoal` and `TargetingConditions`).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, JavaString, Tag};
use horizon_ecs_proto::behaviours::hurt_by::{set_player_created, PlayerCreated};
use horizon_ecs_proto::nbt::{self, Saved};
use horizon_ecs_proto::nbt_fields::common as nbt_common;
use horizon_ecs_proto::behaviours::tempt::Tempts;
use horizon_ecs_proto::behaviours::Cap;
use horizon_ecs_proto::cow::{spawn_species, BlockChanges, Herd, Level, Mw, Pos, SpeciesTable};
use horizon_ecs_proto::goals::{Online, Player};
use horizon_ecs_proto::hostile::{HurtBy, PlayerState};
use horizon_ecs_proto::targeting::{Target, Who};

/// A player at `x` (z 0.5) holding the item that tempts the animal, the animal at the origin.
fn tempt_setup(seed: u64, x: f64) -> (Herd, Entity) {
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = ecs_cow(&mut h, 0.5, 1.0, 0.5, seed);
    h.world.resource_mut::<Online>().set(Player { id: 1, x, y: 1.0, z: 0.5, main: tempt_item().0, off: 0, ..Default::default() });
    (h, e)
}

/// The player the tempt goal of the animal has chosen (0 for none).
fn tempted_by(h: &Herd, e: Entity) -> u32 {
    h.world.get::<Tempts>(e).map_or(0, |t| t.0[0].player)
}

#[test]
fn a_spectator_does_not_tempt_an_animal() {
    if !have_data() || wrong_species(&["minecraft:cow"]) {
        return;
    }
    // Control: the same player, not a spectator, is chosen.
    let (mut h, e) = tempt_setup(7, 3.5);
    h.tick();
    assert_eq!(tempted_by(&h, e), 1, "a player within reach holding the item tempts the animal");
    // A spectator at the same place does not.
    let (mut h, e) = tempt_setup(7, 3.5);
    h.world.resource_mut::<PlayerState>().spectator = vec![false, true];
    h.tick();
    assert_eq!(tempted_by(&h, e), 0, "a spectator never tempts an animal");
}

#[test]
fn an_invisible_player_tempts_only_within_two_blocks() {
    if !have_data() || wrong_species(&["minecraft:cow"]) {
        return;
    }
    // At 3 blocks, visibility 1 reaches the animal (range 10), visibility 0.1 scales the range to 2 blocks and does not.
    let (mut h, e) = tempt_setup(7, 3.5);
    h.tick();
    assert_eq!(tempted_by(&h, e), 1, "a visible player at 3 blocks tempts the animal");
    let (mut h, e) = tempt_setup(7, 3.5);
    h.world.resource_mut::<PlayerState>().set_visibility(1, 0.1);
    h.tick();
    assert_eq!(tempted_by(&h, e), 0, "an invisible player at 3 blocks does not tempt the animal");
}

/// The block changes a sheep's eat goal makes over 3000 ticks, with `mobGriefing` set to `griefing`; the sheep stands on
/// `floor` (y 0) with `feet` at its feet (y 1).
fn grazes(griefing: bool, floor: &'static str, feet: &'static str) -> Vec<(i32, i32, i32, &'static str)> {
    let mut h = Herd::with_slab_awake(16, false, false);
    let mut level = *h.world.resource::<Level>();
    let rule: &'static horizon_client_lib::mobs::MobWorld<'static> =
        Box::leak(Box::new(horizon_client_lib::mobs::MobWorld { mob_griefing: griefing, ..level.mw.0.clone() }));
    level.mw = Mw(rule);
    *h.world.resource_mut::<Level>() = level;
    h.set_block(0, 0, 0, floor);
    h.set_block(0, 1, 0, feet);
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:sheep").unwrap();
    spawn_species(&mut h.world, &sp, 0.5, 0.5, 9);
    let mut changes = Vec::new();
    for _ in 0..3000 {
        h.tick();
        changes.extend(h.world.resource::<BlockChanges>().0.iter().copied());
    }
    changes
}

#[test]
fn the_eat_goal_turns_grass_to_dirt_only_with_mob_griefing() {
    if !have_data() || wrong_species(&["minecraft:sheep"]) {
        return;
    }
    let dirt = |c: &[(i32, i32, i32, &str)]| c.iter().any(|b| b.3 == "minecraft:dirt");
    assert!(dirt(&grazes(true, "minecraft:grass_block", "minecraft:air")), "mobGriefing on: the grass becomes dirt");
    assert!(!dirt(&grazes(false, "minecraft:grass_block", "minecraft:air")), "mobGriefing off: no block changes");
}

#[test]
fn the_eat_goal_breaks_short_grass_only_with_mob_griefing() {
    if !have_data() || wrong_species(&["minecraft:sheep"]) {
        return;
    }
    let cut = |c: &[(i32, i32, i32, &str)]| c.iter().any(|b| b.1 == 1 && b.3 == "minecraft:air");
    assert!(cut(&grazes(true, "minecraft:stone", "minecraft:short_grass")), "mobGriefing on: the short grass is broken");
    assert!(!cut(&grazes(false, "minecraft:stone", "minecraft:short_grass")), "mobGriefing off: the short grass stays");
}

#[test]
fn only_a_golem_spares_the_player_that_built_it() {
    if !have_data() || wrong_species(&["minecraft:iron_golem"]) {
        return;
    }
    for built in [false, true] {
        let mut h = Herd::with_slab_awake(20, false, false);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 3.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
        let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:iron_golem").unwrap();
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 5);
        set_player_created(&mut h.world, e, built);
        h.world.resource_mut::<HurtBy>().0.push((e, Who::Player(1)));
        h.tick();
        h.tick();
        let targeted = h.world.get::<Target>(e).is_some_and(|t| t.who == Who::Player(1));
        assert_eq!(targeted, !built, "a golem the player built does not answer it (built: {built})");
    }
}

#[test]
fn pets_and_horses_are_tamable_but_an_allay_is_not() {
    let reg = horizon_ecs_proto::species::registry();
    let tamable = |n: &str| reg.by_name(n).is_ok_and(|r| r.caps.contains(&Cap::Tamable));
    for n in ["minecraft:wolf", "minecraft:parrot", "minecraft:horse", "minecraft:camel"] {
        assert!(tamable(n), "{n} is tamable");
    }
    for n in ["minecraft:allay", "minecraft:fox", "minecraft:zombie"] {
        assert!(!tamable(n), "{n} is not tamable");
    }
}

/// Whether a rabbit runs from the player 3 blocks away (x 3.5), after 20 ticks: it has moved 1 block or more away.
fn rabbit_flees(wall: bool, peaceful: bool) -> bool {
    let mut h = Herd::with_slab_awake(20, false, false);
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:rabbit").unwrap();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 4);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 3.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    h.world.resource_mut::<PlayerState>().peaceful = peaceful;
    if wall {
        h.set_block(1, 1, 0, "minecraft:stone");
        h.set_block(1, 2, 0, "minecraft:stone");
    }
    let start = h.world.get::<Pos>(e).unwrap().0.x;
    for _ in 0..20 {
        h.tick();
    }
    h.world.get::<Pos>(e).unwrap().0.x < start - 1.0
}

#[test]
fn a_rabbit_flees_a_player_it_can_see() {
    if !have_data() || wrong_species(&["minecraft:rabbit"]) {
        return;
    }
    assert!(rabbit_flees(false, false), "the rabbit runs from a player in the open");
}

#[test]
fn a_rabbit_does_not_flee_a_player_behind_a_wall() {
    if !have_data() || wrong_species(&["minecraft:rabbit"]) {
        return;
    }
    assert!(!rabbit_flees(true, false), "a player it cannot see is not run from");
}

#[test]
fn a_rabbit_does_not_flee_in_peaceful() {
    if !have_data() || wrong_species(&["minecraft:rabbit"]) {
        return;
    }
    assert!(!rabbit_flees(false, true), "in Peaceful no player is a target");
}

#[test]
fn a_golem_built_by_a_player_saves_player_created_and_loads_it_back() {
    let mut w = World::new();
    nbt::register(&mut w, nbt_common::FIELDS);
    let golem = w.spawn((Saved, PlayerCreated)).id();
    let saved = nbt::save(&w, golem).unwrap();
    assert_eq!(saved.get(&JavaString::from("PlayerCreated")), Some(&Tag::Byte(1)), "saved as a byte 1");
    let mut w2 = World::new();
    nbt::register(&mut w2, nbt_common::FIELDS);
    let loaded = w2.spawn(Saved).id();
    assert_eq!(nbt::load(&mut w2, loaded, &saved), Some(0));
    assert!(w2.get::<PlayerCreated>(loaded).is_some(), "loaded back as built by a player");
    let mut unset = Compound::new();
    unset.insert("PlayerCreated".into(), Tag::Byte(0));
    nbt::load(&mut w2, loaded, &unset);
    assert!(w2.get::<PlayerCreated>(loaded).is_none(), "a 0 takes the marker away");
}
