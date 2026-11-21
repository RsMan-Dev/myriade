//! Villager behaviours that the host sees as events: the birth's entity event 12 (hearts on the child), and the golem call's
//! thresholds (meeting 5, panic 3). Not run by the author.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::{
    behaviours::{breed_love, food_stock, summon_defender},
    cow::*,
    goals::*,
    host_out::HostEvent,
};

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:villager"])
}

/// The birth sends entity event 12 for the child (`VillagerMakeLove.breed`: `broadcastEntityEvent(12)`), not heart particles.
#[test]
fn a_villager_birth_sends_event_12_for_the_baby() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == species().name).unwrap();
    let a = spawn_species(&mut h.world, &sp, 0.5, 0.5, 1);
    let b = spawn_species(&mut h.world, &sp, 1.5, 0.5, 2);
    for p in [a, b] {
        h.world.entity_mut(p).insert(food_stock::Food(12));
    }
    h.world.entity_mut(a).insert(breed_love::Mating { partner: b, birth: 0 });
    h.world.resource_mut::<Events>().host.drain().for_each(drop);
    let items = food_stock::items(&h.world).clone();
    let baby = breed_love::birth(&mut h.world, a, &items).expect("both can breed: a child is born");
    let out: Vec<HostEvent> = h.world.resource_mut::<Events>().host.drain().collect();
    assert!(out.iter().any(|e| matches!(e, HostEvent::EntityEvent { entity, code: 12 } if *entity == baby)));
}

/// The meeting call asks for 5 villagers, the panic call for 3 (`spawnGolemIfNeeded(.., 5)` / `(.., 3)`).
#[test]
fn the_golem_calls_ask_five_then_three() {
    assert_eq!(summon_defender::MEETING.needed, 5);
    assert_eq!(summon_defender::PANIC.needed, 3);
}
