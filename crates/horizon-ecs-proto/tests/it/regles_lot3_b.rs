//! Lot 3b of the rules of the mobs (`camel_pose_tests`, `camel_husk_tests`, `cramming_tests`, `enderman_anger_tests`): a camel stands up
//! for its walk target, kneels under a forward key, a baby camel does not follow a sitting parent, a camel husk never breeds, an
//! unsaddled camel sits under its rider, cactus tempts the camel, a ridden horse is not crowding, an angry enderman takes its player
//! back on sight, and a far target is kept. Expected values are the vanilla 26.3 ones (`javap`).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::behaviours::breed::Love;
use horizon_ecs_proto::behaviours::neutral_anger::{anger_at, Anger};
use horizon_ecs_proto::behaviours::posture::{is_sitting, pose_time, sit_down};
use horizon_ecs_proto::behaviours::tempt;
use horizon_ecs_proto::cow::{spawn_species, Clock, GrowAt, Herd, Pos, SpeciesTable};
use horizon_ecs_proto::goals::{go_to_at, Events, GoalSet};
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::mount::{self, Mount};
use horizon_ecs_proto::pilot::{self, PilotInput};
use horizon_ecs_proto::targeting::{clear_target, set_target, Target, Who};

/// The species of the table by name.
fn spawn(h: &mut Herd, name: &str, x: f64, z: f64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    spawn_species(&mut h.world, &sp, x, z, 5)
}

/// The tempt entry of the species: the items that tempt it, by name.
fn tempt_items(h: &Herd, e: Entity) -> Vec<&'static str> {
    let goals = h.world.get::<GoalSet>(e).unwrap();
    goals
        .entries()
        .iter()
        .filter(|en| horizon_ecs_proto::behaviours::same(en.module, &tempt::MODULE))
        .filter_map(|en| en.params.downcast_ref::<tempt::Params>())
        .flat_map(|p| p.items.iter().copied())
        .collect()
}

/// Whether the species has a breeding goal.
fn breeds(h: &Herd, e: Entity) -> bool {
    let goals = h.world.get::<GoalSet>(e).unwrap();
    goals.entries().iter().any(|en| horizon_ecs_proto::behaviours::same(en.module, &horizon_ecs_proto::behaviours::breed::MODULE))
}

/// The game time left of the anger: its end less the game time.
fn anger_left(h: &Herd, e: Entity) -> i64 {
    let now = h.world.resource::<Clock>().game_now();
    h.world.get::<Anger>(e).map_or(0, |a| a.until as i64 - now)
}

#[test]
fn a_walk_target_stands_a_sitting_camel_up_and_then_it_walks() {
    if !have_data() || wrong_species(&["minecraft:camel"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:camel", 0.5, 0.5);
    sit_down(&mut h.world, e);
    for _ in 0..41 {
        h.tick();
    }
    assert!(is_sitting(&h.world, e), "a settled sitting camel");
    h.world.resource_mut::<Events>().host.drain();
    let before = h.world.get::<Pos>(e).unwrap().0.x;
    go_to_at(&mut h.world, e, [10.5, 0.0, 0.5], 1.0, 1);
    let stood = !is_sitting(&h.world, e);
    let sound = h.world.resource_mut::<Events>().host.drain().any(|ev| matches!(ev, HostEvent::Sound { name, .. } if name == "entity.camel.stand"));
    // The stand-up's 52 ticks hold it in place...
    while pose_time(&h.world, e) < 51 {
        h.tick();
    }
    let held = (h.world.get::<Pos>(e).unwrap().0.x - before).abs();
    // ...then it walks toward the target.
    for _ in 0..25 {
        h.tick();
    }
    let walked = h.world.get::<Pos>(e).unwrap().0.x - before;
    assert!(stood && sound, "the walk target stands the sitting camel up with its stand sound");
    assert!(held < 0.01, "the stand-up transition holds it in place, moved {held}");
    assert!(walked > 0.5, "then it walks toward the target, moved {walked}");
}

#[test]
fn pressing_forward_during_the_kneel_waits_for_the_transition() {
    if !have_data() || wrong_species(&["minecraft:camel"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:camel", 0.5, 0.5);
    h.world.get_mut::<Mount>(e).unwrap().saddled = true;
    sit_down(&mut h.world, e);
    assert!(mount::ride(&mut h.world, e, Who::Player(1)), "the first seat is free");
    let forward = PilotInput { who: Who::Player(1), forward: 1.0, strafe: 0.0, yaw: 0.0, pitch: 0.0, jump: 0.0, sprint: false, sneak: false, steer: false, keys: [false; 4] };
    assert!(pilot::set(&mut h.world, e, forward));
    // 10 ticks into the 40-tick kneel.
    for _ in 0..10 {
        h.tick();
    }
    let during_kneel = is_sitting(&h.world, e);
    // The kneel is over: the forward key stands it up.
    for _ in 0..30 {
        h.tick();
    }
    assert_eq!((during_kneel, is_sitting(&h.world, e)), (true, false), "forward during the kneel leaves it sitting; once the kneel is over it stands up");
}

#[test]
fn a_sitting_baby_camel_does_not_follow_its_parent() {
    if !have_data() || wrong_species(&["minecraft:camel"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    spawn(&mut h, "minecraft:camel", 0.5, 5.5);
    let baby = spawn(&mut h, "minecraft:camel", 0.5, 0.5);
    h.world.entity_mut(baby).insert(GrowAt(u64::MAX));
    // Sit it before it ever follows the parent, and let the kneel finish.
    sit_down(&mut h.world, baby);
    for _ in 0..41 {
        h.tick();
    }
    assert!(is_sitting(&h.world, baby), "a settled sitting baby camel");
    let mut stood = false;
    for _ in 0..20 {
        h.tick();
        stood |= !is_sitting(&h.world, baby);
    }
    assert!(!stood, "a sitting baby camel does not follow its parent");
}

#[test]
fn a_camel_husk_never_breeds() {
    if !have_data() || wrong_species(&["minecraft:camel_husk", "minecraft:camel"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let husk = spawn(&mut h, "minecraft:camel_husk", 0.5, 0.5);
    let camel = spawn(&mut h, "minecraft:camel", 0.5, 0.5);
    h.world.entity_mut(husk).insert(Love(u64::MAX));
    h.world.entity_mut(camel).insert(Love(u64::MAX));
    assert!(!breeds(&h, husk), "a camel husk has no breeding goal");
    assert!(breeds(&h, camel), "a camel in love still breeds");
}

#[test]
fn an_unsaddled_camel_sits_down_under_its_rider() {
    if !have_data() || wrong_species(&["minecraft:camel"]) {
        return;
    }
    // Whether the camel kneels within 64 ticks of its pose age, under a rider or not.
    let sits = |saddled: bool, rider: bool| {
        let mut h = Herd::with_slab_awake(20, false, false);
        let e = spawn(&mut h, "minecraft:camel", 0.5, 0.5);
        h.world.get_mut::<Mount>(e).unwrap().saddled = saddled;
        if rider {
            assert!(mount::ride(&mut h.world, e, Who::Player(1)), "the first seat is free");
        }
        (0..64).any(|_| {
            h.tick();
            is_sitting(&h.world, e)
        })
    };
    assert!(sits(false, false), "a camel with nobody on it sits down");
    assert!(sits(false, true), "a player on an unsaddled camel does not stop it kneeling");
    assert!(!sits(true, true), "a saddled camel under its driver never kneels");
}

#[test]
fn a_camel_is_tempted_by_cactus() {
    if !have_data() || wrong_species(&["minecraft:camel", "minecraft:camel_husk"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let camel = spawn(&mut h, "minecraft:camel", 0.5, 0.5);
    let husk = spawn(&mut h, "minecraft:camel_husk", 0.5, 0.5);
    assert!(tempt_items(&h, camel).contains(&"minecraft:cactus"), "a cactus tempts a camel");
    assert!(tempt_items(&h, husk).contains(&"minecraft:rabbit_foot"), "a rabbit's foot tempts a camel husk");
}

#[test]
fn a_ridden_horse_is_not_counted_for_cramming() {
    if !have_data() || wrong_species(&["minecraft:pig", "minecraft:horse"]) {
        return;
    }
    // `maxEntityCramming` 3, the value of the Mob test: a pig with three others overlapping it is crushed (one tick in four, 6 damage).
    // Over 40 ticks, the pile hurts it; a ridden horse in the pile is not one of the others.
    let hurt = |others: usize, horse: Option<bool>| {
        let mut h = Herd::with_slab_awake(20, false, false);
        h.set_max_entity_cramming(3);
        let pig = spawn(&mut h, "minecraft:pig", 0.5, 0.5);
        for _ in 0..others {
            spawn(&mut h, "minecraft:pig", 0.5, 0.5);
        }
        if let Some(ridden) = horse {
            let horse = spawn(&mut h, "minecraft:horse", 0.5, 0.5);
            h.world.get_mut::<Mount>(horse).unwrap().saddled = true;
            if ridden {
                assert!(mount::ride(&mut h.world, horse, Who::Player(1)), "the first seat is free");
            }
        }
        let full = health(&h, pig);
        (0..40).any(|_| {
            h.tick();
            health(&h, pig) < full
        })
    };
    assert!(!hurt(2, None), "two pigs do not crush");
    assert!(hurt(3, None), "three pigs crush");
    assert!(!hurt(2, Some(true)), "a ridden horse is not counted: two pigs and it do not crush");
    assert!(hurt(2, Some(false)), "an empty horse is counted: two pigs and it crush");
}

#[test]
fn an_enderman_calms_down_when_its_anger_runs_out() {
    if !have_data() || wrong_species(&["minecraft:enderman"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.set_online(&[(6.5, 0.5)]);
    let e = spawn(&mut h, "minecraft:enderman", 0.5, 0.5);
    set_target(&mut h.world, e, Target::player(1, 20));
    h.tick();
    clear_target(&mut h.world, e);
    // The anger runs out with no target: the enderman is calm, and the look goal does not take the player back without a stare.
    for _ in 0..800 {
        if anger_left(&h, e) <= 0 {
            break;
        }
        h.tick();
    }
    h.tick();
    assert_eq!(anger_at(&h.world, e), None, "anger over: calm");
    assert!(h.world.get::<Target>(e).is_none(), "anger over: no target, no retargeting without a stare");
}

#[test]
fn an_angry_enderman_takes_its_player_back_on_sight_without_a_stare() {
    if !have_data() || wrong_species(&["minecraft:enderman"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.set_online(&[(6.5, 0.5)]);
    let e = spawn(&mut h, "minecraft:enderman", 0.5, 0.5);
    set_target(&mut h.world, e, Target::player(1, 20));
    h.tick();
    let held = anger_left(&h, e);
    clear_target(&mut h.world, e);
    for _ in 0..10 {
        h.tick();
    }
    let lost = anger_left(&h, e);
    assert_eq!(anger_at(&h.world, e), Some(Who::Player(1)), "a lost target keeps the anger");
    assert!((400..=780).contains(&held), "20 to 39 s of anger, got {held}");
    assert_eq!(lost, held - 10, "the anger runs down one tick at a time");
    h.tick();
    assert_eq!(h.world.get::<Target>(e).map(|t| t.who), Some(Who::Player(1)), "an angry enderman takes its player back on sight");
    // The control: a calm enderman ignores the same player without a stare.
    let calm = spawn(&mut h, "minecraft:enderman", 0.5, 2.5);
    for _ in 0..3 {
        h.tick();
    }
    assert!(h.world.get::<Target>(calm).is_none(), "a calm one needs a stare");
}

#[test]
fn an_enderman_keeps_a_target_beyond_its_follow_range() {
    if !have_data() || wrong_species(&["minecraft:enderman"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(90, false, false);
    h.set_online(&[(80.5, 0.5)]);
    let e = spawn(&mut h, "minecraft:enderman", 0.5, 0.5);
    set_target(&mut h.world, e, Target::player(1, 20));
    h.tick();
    assert_eq!(h.world.get::<Target>(e).map(|t| t.who), Some(Who::Player(1)), "the target 80 blocks away is kept");
}
