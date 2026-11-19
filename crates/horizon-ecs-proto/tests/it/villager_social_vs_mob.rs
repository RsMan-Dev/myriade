//! The social side of the villager, ECS only. No new public access on the client side: the Mob's birth trigger (`BreedVillager`
//! interaction), its `LastSlept` and its golem call are private, so no comparison with the `Mob` is made here (notes, section 4).
//! The checks are the vanilla numbers: the gossip of a day, the iron golem of five villagers who slept, the birth and rest.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::{
    behaviours::{breed_love, food_stock, gossip as eg, golem_summon},
    brain::*,
    cow::*,
    goals::*,
};

fn skip() -> bool {
    !have_data() || std::env::var("HORIZON_SPECIES").as_deref() != Ok("minecraft:villager")
}
fn villager_at(h: &mut Herd, at: (f64, f64), seed: u64) -> Entity {
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == species().name).unwrap();
    let e = spawn_species(&mut h.world, &sp, at.0, at.1, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    e
}

/// A villager hears of player 7 (a minor negative, 25): after one day (24000 ticks) the value has decayed by its daily amount
/// (20), so 5 is left, weight -1: the reputation is -5.
#[test]
fn the_gossip_of_a_player_decays_a_day() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab(60, true, false);
    let e = villager_at(&mut h, (0.5, 0.5), 7);
    eg::add(&mut h.world, e, 7, eg::Kind::MinorNegative, 25);
    for _ in 0..eg::DAY {
        h.tick();
    }
    let ecs = h.world.get::<eg::Gossips>(e).map_or(0, |g| g.reputation(7));
    assert_eq!(ecs, -5, "25 - 20 = 5 left, weight -1");
}

/// Five villagers in a box, all of whom slept within the day and have seen no golem: the call spawns one iron golem on a
/// column near them (not on a villager's column), with its y a whole block, and all five have seen it.
#[test]
fn five_villagers_who_slept_call_an_iron_golem_on_a_column() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab(60, true, false);
    let ids: Vec<Entity> = (0..5).map(|k| villager_at(&mut h, (2.0 * k as f64, 0.5), k as u64 + 1)).collect();
    for &e in &ids {
        h.world.entity_mut(e).insert(golem_summon::LastSlept(0));
    }
    let called = golem_summon::check(&mut h.world, ids[0], golem_summon::NEEDED);
    let golem_id = h.world.resource::<SpeciesTable>().0.iter().position(|s| s.name == "minecraft:iron_golem").unwrap() as u8;
    let golems: Vec<[f64; 3]> = {
        let mut q = h.world.query::<(&SpeciesId, &Pos)>();
        q.iter(&h.world).filter(|(s, _)| s.0 == golem_id).map(|(_, p)| [p.0.x, p.0.y, p.0.z]).collect()
    };
    assert!(called, "the five villagers call a golem");
    assert_eq!(golems.len(), 1, "one iron golem spawned");
    let g = golems[0];
    assert!((g[0] - 0.5).abs() <= 9.0 && (g[2] - 0.5).abs() <= 9.0, "the column is near the villagers: {g:?}");
    assert_eq!(g[1], g[1].round(), "the golem stands on a whole block: {g:?}");
    assert!(ids.iter().all(|&e| present(&h.world, e, &golem_summon::GOLEM_DETECTED)), "each villager has seen the golem");
}

/// Two villagers with food for breeding (12 points each) start to make love: the baby comes at the planned tick
/// (275 + rand(50) after the start), spawns at the partner, and both parents rest for 6000 ticks.
#[test]
fn a_birth_comes_at_its_tick_and_the_parents_rest() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab(60, true, false);
    let a = villager_at(&mut h, (0.5, 0.5), 11);
    let b = villager_at(&mut h, (1.5, 0.5), 12);
    for &p in &[a, b] {
        h.world.entity_mut(p).insert(food_stock::Food(food_stock::WANT));
    }
    let start = now_of(&h.world, a);
    breed_love::start(&mut h.world, a, b);
    let due = h.world.get::<breed_love::Mating>(a).unwrap().birth;
    while now_of(&h.world, a) < due {
        h.tick();
    }
    let items = food_stock::items(&h.world);
    let baby = breed_love::birth(&mut h.world, a, items).expect("the birth is due");
    let now = now_of(&h.world, a);
    assert!((275..325).contains(&(due - start)), "delay 275 + rand(50): {}", due - start);
    assert_eq!(now, due, "the birth comes at its tick");
    assert!(h.world.get::<GrowAt>(baby).is_some_and(|g| g.0 == now + breed_love::GROWN), "the baby grows in 24000 ticks");
    for &p in &[a, b] {
        assert!(h.world.get::<Resting>(p).is_some_and(|r| r.0 == now + breed_love::REST), "the parents rest 6000 ticks");
    }
}
