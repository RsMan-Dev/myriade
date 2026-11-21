//! Les comportements du lot 6 du villageois, un test par comportement, sur un troupeau éveillé. Valeurs attendues lues dans le `Mob`
//! (`villager_brain.rs`: `SetLookAndInteract` à 4 blocs, `LookAtEntity` à 8 blocs, `InteractWith` à 8 blocs, `CalmDown` à 6 blocs),
//! pas dans le code ECS. Écrits, non lancés.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_ecs_proto::behaviours::play_tag::INTERACTION_TARGET;
use horizon_ecs_proto::brain::behaviours::{calm_down, look_and_interact, look_at_entity, villager_interact};
use horizon_ecs_proto::{brain::*, cow::*, goals::{Entered, Online, Player}, targeting::Who};

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:villager"])
}

fn villager_at(h: &mut Herd, x: f64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:villager").unwrap();
    let e = spawn_species(&mut h.world, &sp, x, 0.5, 1);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    e
}

fn player_at(h: &mut Herd, x: f64) {
    h.world.resource_mut::<Online>().set(Player { id: 1, x, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
}

fn ctx(params: &'static (dyn std::any::Any + Sync)) -> Ctx {
    Ctx { id: 0, nth: 0, params }
}

static LOOK_PLAYERS: look_at_entity::Params = look_at_entity::Params { species: &[], players: true };
static INTERACT: villager_interact::Params = villager_interact::Params { species: "minecraft:villager", in_love: false, breed: false };
static CALM: calm_down::Params = calm_down::Params { range_sqr: 36.0 };

#[test]
fn the_villager_interacts_with_a_player_within_4_blocks_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = villager_at(&mut h, 0.5);
    set(&mut h.world, e, &VISIBLE_MOBS, MemValue::Whos(vec![Who::Player(1)]), None);
    player_at(&mut h, 4.5);
    assert!((look_and_interact::BEHAVIOUR.can_start)(&mut h.world, e, &ctx(&())));
    assert!(matches!(get(&h.world, e, &INTERACTION_TARGET), Some(MemValue::Who(Who::Player(1)))));
    // Beyond 4 blocks (16 squared), nothing is set.
    let far = villager_at(&mut h, 20.5);
    set(&mut h.world, far, &VISIBLE_MOBS, MemValue::Whos(vec![Who::Player(1)]), None);
    player_at(&mut h, 25.5);
    assert!(!(look_and_interact::BEHAVIOUR.can_start)(&mut h.world, far, &ctx(&())));
    assert!(!present(&h.world, far, &INTERACTION_TARGET));
}

#[test]
fn the_villager_looks_at_a_player_within_8_blocks_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = villager_at(&mut h, 0.5);
    set(&mut h.world, e, &VISIBLE_MOBS, MemValue::Whos(vec![Who::Player(1)]), None);
    player_at(&mut h, 7.5);
    assert!((look_at_entity::BEHAVIOUR.can_start)(&mut h.world, e, &ctx(&LOOK_PLAYERS)));
    assert!(matches!(look_target(&h.world, e), Some(Tracker::Entity(Who::Player(1), true))));
    // Beyond 8 blocks (64 squared), no look target.
    let far = villager_at(&mut h, 20.5);
    set(&mut h.world, far, &VISIBLE_MOBS, MemValue::Whos(vec![Who::Player(1)]), None);
    player_at(&mut h, 29.5);
    assert!(!(look_at_entity::BEHAVIOUR.can_start)(&mut h.world, far, &ctx(&LOOK_PLAYERS)));
    assert!(look_target(&h.world, far).is_none());
}

#[test]
fn the_villager_walks_to_a_villager_it_wants_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = villager_at(&mut h, 0.5);
    let other = villager_at(&mut h, 4.5);
    set(&mut h.world, e, &VISIBLE_MOBS, MemValue::Whos(vec![Who::Entity(other)]), None);
    assert!((villager_interact::BEHAVIOUR.can_start)(&mut h.world, e, &ctx(&INTERACT)));
    (villager_interact::BEHAVIOUR.start)(&mut h.world, e, &ctx(&INTERACT));
    // `InteractWith`: the partner is the interaction target, looked at, walked to at 0.5 blocks per tick, close 2.
    assert!(matches!(get(&h.world, e, &INTERACTION_TARGET), Some(MemValue::Who(Who::Entity(k))) if *k == other));
    assert!(matches!(walk_target(&h.world, e), Some(WalkTarget { speed, close: 2, .. }) if (speed - 0.5).abs() < 1e-6));
}

#[test]
fn the_villager_forgets_a_hurt_once_its_hurter_is_far_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = villager_at(&mut h, 0.5);
    set(&mut h.world, e, &HURT_BY_ENTITY, MemValue::Who(Who::Player(1)), None);
    player_at(&mut h, 10.5);
    assert!((calm_down::BEHAVIOUR.can_start)(&mut h.world, e, &ctx(&CALM)));
    // 10 blocks away (100 squared, beyond 36): calm, the hurt memory is forgotten.
    assert!(!present(&h.world, e, &HURT_BY_ENTITY));
    // A hurter at 5 blocks (25 squared) keeps the villager scared.
    let near = villager_at(&mut h, 20.5);
    set(&mut h.world, near, &HURT_BY_ENTITY, MemValue::Who(Who::Player(1)), None);
    player_at(&mut h, 25.5);
    assert!((calm_down::BEHAVIOUR.can_start)(&mut h.world, near, &ctx(&CALM)));
    assert!(present(&h.world, near, &HURT_BY_ENTITY));
}
