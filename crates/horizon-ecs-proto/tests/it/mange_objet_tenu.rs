//! The meal of the item in the mouth (`eat_held`) and the arm swing (`swing`). The `Mob` has no public entry for the meal (its
//! block is `Fox.aiStep`): the expected values are the jar's (more than 600 ticks since the last meal, a refused meal asked again,
//! one of the stack eaten, the clock restarted) with the delay shortened by the parameters.
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;
use horizon_ecs_proto::behaviours::eat_held::{self, EatDue, Eaten};
use horizon_ecs_proto::behaviours::Entry;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::{main_hand, set_main_hand, Req, Requests};
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::species::Config;
use horizon_ecs_proto::swing;

static FOOD: [&str; 1] = ["minecraft:sweet_berries"];
static ENTRIES: [Entry; 1] = [Entry::new(&eat_held::MODULE, 0, &eat_held::Params { delay: 30, crumbs_from: 20, retry: 10, foods: &FOOD, ..eat_held::Params::DEFAULT })];

fn berry(count: u32) -> ItemStack {
    ItemStack::new("minecraft:sweet_berries", count)
}

/// A creature on the ground that eats berries, its mouth empty.
fn scene() -> Option<(Herd, Entity)> {
    if !crate::common::have_data() {
        return None;
    }
    let mut h = Herd::with_slab_awake(30, false, false);
    let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
    sp.goals = Config { entries: &ENTRIES };
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 13);
    h.world.flush();
    h.world.resource_mut::<Requests>().0.push(Req::Arm(e));
    h.world.flush();
    for _ in 0..5 {
        h.tick();
    }
    h.drain_host_out();
    Some((h, e))
}

fn meals(h: &mut Herd) -> usize {
    h.drain_host_out().iter().filter(|ev| matches!(ev, HostEvent::HeldConsume { .. })).count()
}

/// Nothing is armed while the mouth is empty; a held berry is asked of the host once more than 30 ticks have passed, and not before.
#[test]
fn the_meal_is_asked_after_the_delay_and_not_before() {
    let Some((mut h, e)) = scene() else { return };
    assert!(h.world.get::<EatDue>(e).is_none(), "an empty mouth arms nothing");
    set_main_hand(&mut h.world, e, Some(berry(1)));
    eat_held::restart(&mut h.world, e);
    assert!(h.world.get::<EatDue>(e).is_some(), "a held item arms the deadline");
    for _ in 0..30 {
        h.tick();
    }
    assert_eq!(meals(&mut h), 0, "30 ticks after the last meal is not more than 30");
    for _ in 0..8 {
        h.tick();
    }
    assert_eq!(meals(&mut h), 1, "the meal is asked once");
}

/// The host's answer takes one berry of a stack of two and restarts the clock; with one left, the remainder replaces it.
#[test]
fn finishing_a_meal_eats_one_and_restarts_the_clock() {
    let Some((mut h, e)) = scene() else { return };
    set_main_hand(&mut h.world, e, Some(berry(2)));
    eat_held::restart(&mut h.world, e);
    for _ in 0..40 {
        h.tick();
    }
    assert_eq!(meals(&mut h), 1);
    assert!(!eat_held::finish(&mut h.world, e, &berry(3), None), "a stack that is not the one held is refused");
    assert!(eat_held::finish(&mut h.world, e, &berry(2), None));
    assert_eq!(main_hand(&h.world, e).map(|s| s.count), Some(1));
    let now = now_of(&h.world, e);
    assert_eq!(h.world.get::<Eaten>(e).unwrap().0, now, "the clock restarts at the meal");
    for _ in 0..40 {
        h.tick();
    }
    assert_eq!(meals(&mut h), 1, "the last berry is asked in its turn");
    assert!(eat_held::finish(&mut h.world, e, &berry(1), None));
    assert!(main_hand(&h.world, e).is_none(), "the last berry leaves the mouth empty");
    assert!(h.world.get::<EatDue>(e).is_none(), "nothing is armed once the mouth is empty");
}

/// A refused meal (a target) is not asked, and is asked again once the target is gone.
#[test]
fn a_target_refuses_the_meal() {
    let Some((mut h, e)) = scene() else { return };
    h.world.entity_mut(e).insert(horizon_ecs_proto::targeting::Target::new(horizon_ecs_proto::targeting::Who::Player(1), 100, 0));
    set_main_hand(&mut h.world, e, Some(berry(1)));
    eat_held::restart(&mut h.world, e);
    for _ in 0..60 {
        h.tick();
    }
    assert_eq!(meals(&mut h), 0, "a creature with a target does not eat");
    h.world.entity_mut(e).remove::<horizon_ecs_proto::targeting::Target>();
    for _ in 0..10 {
        h.tick();
    }
    assert_eq!(meals(&mut h), 1, "the meal is asked again within the retry delay");
}

/// A swing starts, is refused in its first half, and starts again in its second; the host hears each start.
#[test]
fn a_swing_is_refused_in_its_first_half() {
    let Some((mut h, e)) = scene() else { return };
    assert!(swing::swing(&mut h.world, e, false));
    assert!(swing::is_swinging(&h.world, e));
    assert!(!swing::swing(&mut h.world, e, true), "a swing in its first half is not restarted");
    for _ in 0..3 {
        h.tick();
    }
    assert!(swing::swing(&mut h.world, e, true), "a swing in its second half restarts");
    let heard: Vec<bool> = h.drain_host_out().into_iter().filter_map(|ev| if let HostEvent::Swing { off_hand, .. } = ev { Some(off_hand) } else { None }).collect();
    assert_eq!(heard, vec![false, true]);
    for _ in 0..10 {
        h.tick();
    }
    assert!(h.world.get::<swing::Swinging>(e).is_none(), "the state is dropped when the swing is over");
}
