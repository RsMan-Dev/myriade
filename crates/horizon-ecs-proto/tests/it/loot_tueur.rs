//! What the killer changes in a death: the Looting of its weapon, the `mob_drops` rule, and the reinforcement base a zombie draws at its spawn.
//! `HORIZON_TEST_RESOURCES` set; the loot tests are written for a cow, the reinforcement test for a zombie (`HORIZON_SPECIES`).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::random::LegacyRandom;
use horizon_ecs_proto::bridge::players::set_player;
use horizon_ecs_proto::bridge::host::PlayerFact;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::credit::PlayerCredit;
use horizon_ecs_proto::goals::Player;
use horizon_ecs_proto::item::Item;
use horizon_ecs_proto::loot::{note_killer, DeathSeed};
use horizon_ecs_proto::orb::Orb;

/// A cow killed in the last 100 ticks by player 1, whose weapon is `weapon`; returns the items' total count and the orbs left.
fn kill_cow(h: &mut Herd, seed: i64, weapon: Option<ItemStack>) -> (i32, usize) {
    let player = Player { id: 1, x: 0.5, y: 1.0, z: 0.5, main: 0, off: 0, head: 0, chest: 0, legs: 0, feet: 0 };
    set_player(&mut h.world, player, PlayerFact { alive: true, targetable: true, hero: None });
    let sp = *species();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 9);
    h.world.entity_mut(e).insert((DeathSeed(seed), PlayerCredit { player: Some(1), uuid: None, until: u64::MAX }));
    note_killer(&mut h.world, e, 1, 0.0, weapon);
    // The blow is the player's: the loot reads the killer and its weapon from the combat log.
    h.world.resource_mut::<DamageQueue>().add_by(e, 1000.0, (0.0, 0.0), 1);
    h.tick();
    let items: Vec<(Entity, i32)> = h.world.query::<(Entity, &Item)>().iter(&h.world).map(|(e, i)| (e, i.stack.count as i32)).collect();
    let orbs: Vec<Entity> = h.world.query::<(Entity, &Orb)>().iter(&h.world).map(|(e, _)| e).collect();
    let total = items.iter().map(|x| x.1).sum();
    for e in items.iter().map(|x| x.0).chain(orbs.iter().copied()) {
        h.world.entity_mut(e).despawn();
    }
    (total, orbs.len())
}

fn sword_of_looting(level: i32) -> ItemStack {
    let mut s = ItemStack::new("minecraft:diamond_sword", 1);
    s.enchant("minecraft:looting", level as _);
    s
}

/// The Looting of the killing player's weapon (told by the host in its fact) adds to what the same death seed leaves.
#[test]
fn the_looting_of_the_killers_weapon_adds_to_the_loot() {
    if !have_data() || wrong_species(&["minecraft:cow"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(16, true, false);
    let (mut plain, mut looted) = (0, 0);
    for k in 0..60 {
        let seed = 1000 + 7919 * k;
        plain += kill_cow(&mut h, seed, None).0;
        looted += kill_cow(&mut h, seed, Some(sword_of_looting(3))).0;
    }
    assert!(plain > 0, "a cow leaves something");
    assert!(looted > plain, "Looting III over 60 deaths: {looted} against {plain}");
}

/// With the `mob_drops` rule off a killed cow leaves no item and no orb; with it on, it leaves both.
#[test]
fn mob_drops_off_leaves_neither_items_nor_experience() {
    if !have_data() || wrong_species(&["minecraft:cow"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(16, true, false);
    let (items, orbs) = (0..10).map(|k| kill_cow(&mut h, 77 + k, None)).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    assert!(items > 0 && orbs > 0, "rule on: {items} items, {orbs} orbs");
    let mut level = *h.world.resource::<Level>();
    let off: &'static horizon_client_lib::mobs::MobWorld<'static> = Box::leak(Box::new(horizon_client_lib::mobs::MobWorld { mob_drops: false, ..level.mw.0.clone() }));
    level.mw = Mw(off);
    *h.world.resource_mut::<Level>() = level;
    let (items, orbs) = (0..10).map(|k| kill_cow(&mut h, 77 + k, None)).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    assert_eq!((items, orbs), (0, 0), "rule off");
}

/// A zombie draws its reinforcement chance at its spawn: `nextDouble * 0.1` (no leader without the special multiplier), and it differs from one zombie to the next.
#[test]
fn a_zombie_draws_its_reinforcement_base_at_the_spawn() {
    if !have_data() || wrong_species(&["minecraft:zombie"]) {
        return;
    }
    use horizon_ecs_proto::reinforcements::{chance, Params};
    use horizon_ecs_proto::spawn_finalize::{finalize, Finalize};
    let mut h = Herd::with_slab_awake(16, true, false);
    let sp = *species();
    let mut seen = Vec::new();
    for k in 0..40 {
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 9 + k);
        let kind = Finalize::Zombie { jockey: "minecraft:chicken" };
        finalize(&mut h.world, e, kind, None, horizon_ecs_proto::spawn_reason::SpawnReason::Spawner, 0.0, &mut LegacyRandom::new(500 + k as i64));
        let c = chance(&h.world, e, &Params::ZOMBIE);
        assert!((0.0..=0.100_000_002).contains(&c), "base {c} within [0, 0.1]");
        seen.push(c);
    }
    assert!(seen.iter().any(|c| *c > 0.0) && seen.windows(2).any(|w| w[0] != w[1]), "the base is drawn, not fixed: {seen:?}");
}

/// A fixed experience reward (a monster's 5) draws nothing from the random; a range draws once.
#[test]
fn a_fixed_experience_reward_draws_nothing() {
    use horizon_ecs_proto::rules::loot::xp_amount;
    let (mut a, mut b) = (LegacyRandom::new(9), LegacyRandom::new(9));
    assert_eq!(xp_amount(5, 5, &mut a), 5);
    assert_eq!(a.next_int_bound(100), b.next_int_bound(100), "no draw was consumed");
    assert!((1..=3).contains(&xp_amount(1, 3, &mut a)));
}
