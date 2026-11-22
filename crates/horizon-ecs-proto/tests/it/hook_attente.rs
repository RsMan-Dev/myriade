//! The waits of a hook in the water as deadlines (`hook_wait`): the lure and the approach are dates on the wheel, reached by a clock
//! jump, not by counting ticks. The draws keep vanilla's bounds (`lureWait`, `Mth.nextInt(20, 80)`).
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_core::random::LegacyRandom;
use horizon_ecs_proto::cow::{Clock, Herd, Pos, Vel};
use horizon_ecs_proto::fishing_hook::cast;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::host_out_hook::{HookEvent, HookOut};
use horizon_ecs_proto::bridge::host::PlayerFact;
use horizon_ecs_proto::hook_wait::{left_now, leave_water, speed_changed, Wait};
use horizon_ecs_proto::rules::hook_wait::{hooked_after, lured_after};

const SEED: i64 = 11;
fn at() -> Vec3 {
    Vec3::new(0.5, 5.0, 0.5)
}

fn stand(h: &mut Herd) {
    h.world.resource_mut::<Online>().set(Player { id: 1, x: at().x, y: at().y, z: at().z, main: 0, off: 0, ..Default::default() });
}
/// A pool of water over the hook's cell and the cells around it.
fn lake(h: &mut Herd) {
    for x in 0..=1 {
        for y in 0..=3 {
            for z in 0..=1 {
                h.set_block(x, y, z, "minecraft:water");
            }
        }
    }
}
/// Casts into the lake and ticks until the hook floats and its lure is armed: the hook, and its wait.
fn land(h: &mut Herd) -> (Entity, Wait) {
    lake(h);
    stand(h);
    let hook = cast(&mut h.world, 1, at(), Vec3::ZERO, 0, 0, SEED);
    for _ in 0..60 {
        stand(h);
        h.tick();
        if let Some(w) = h.world.get::<Wait>(hook) {
            return (hook, *w);
        }
    }
    panic!("the hook never armed its lure");
}
/// The hook's own time now.
fn own(h: &Herd, hook: Entity) -> u64 {
    h.world.resource::<Clock>().of(hook)
}
/// Jumps the clock to the hook's own time `at` and runs that tick: the ticks before it are not run.
fn skip_to(h: &mut Herd, hook: Entity, at: u64) {
    let world_at = h.world.resource::<Clock>().at(hook, at);
    h.world.resource_mut::<Clock>().0 = world_at - 1;
    stand(h);
    h.tick();
}
fn hook_events(h: &mut Herd) -> Vec<HookEvent> {
    std::mem::take(&mut h.world.get_resource_or_insert_with(HookOut::default).0)
}

#[test]
fn the_lure_is_a_date_with_vanillas_bounds() {
    let mut h = Herd::with_slab(16, true, false);
    let (_, wt) = land(&mut h);
    assert!((100..=600).contains(&wt.left), "lure {}", wt.left);
    assert!(!wt.approach);
}

#[test]
fn a_lured_fish_approaches_at_its_date_and_the_bite_comes_at_the_end_of_the_approach() {
    let mut h = Herd::with_slab(16, true, false);
    let (hook, _) = land(&mut h);
    speed_changed(&mut h.world, hook, 1);
    let now = own(&h, hook);
    let lured = now + left_now(&h.world, hook) as u64;
    skip_to(&mut h, hook, lured - 1);
    assert!(!h.world.get::<Wait>(hook).unwrap().approach, "lured one tick early");
    skip_to(&mut h, hook, lured);
    let wt = *h.world.get::<Wait>(hook).unwrap();
    assert!(wt.approach && (20..=80).contains(&wt.left), "approach {wt:?}");
    let bitten = lured + wt.left as u64;
    skip_to(&mut h, hook, bitten - 1);
    assert!(h.world.get::<Wait>(hook).is_some_and(|w| w.approach), "approach ended one tick early");
    assert!(hook_events(&mut h).is_empty());
    skip_to(&mut h, hook, bitten);
    assert!(h.world.get::<Wait>(hook).is_none());
    assert!(!hook_events(&mut h).is_empty(), "no bite at the end of the approach");
}

#[test]
fn a_faster_fish_halves_what_is_left_of_the_lure() {
    let mut h = Herd::with_slab(16, true, false);
    let (hook, _) = land(&mut h);
    speed_changed(&mut h.world, hook, 1);
    let now = own(&h, hook);
    let left = left_now(&h.world, hook) as u64;
    speed_changed(&mut h.world, hook, 2);
    let half = now + left.div_ceil(2);
    skip_to(&mut h, hook, half - 1);
    assert!(!h.world.get::<Wait>(hook).unwrap().approach, "the lure ended before half of its ticks");
    skip_to(&mut h, hook, half);
    assert!(h.world.get::<Wait>(hook).unwrap().approach, "the lure did not end at half of its ticks");
}

#[test]
fn a_hook_out_of_the_water_ignores_its_stale_deadline() {
    let mut h = Herd::with_slab(16, true, false);
    let (hook, _) = land(&mut h);
    speed_changed(&mut h.world, hook, 1);
    let now = own(&h, hook);
    let lured = now + left_now(&h.world, hook) as u64;
    leave_water(&mut h.world, hook);
    skip_to(&mut h, hook, lured);
    assert!(!h.world.get::<Wait>(hook).is_some_and(|w| w.approach), "a stale lure started an approach");
    assert!(hook_events(&mut h).is_empty());
}

#[test]
fn the_draws_stay_within_vanillas_bounds() {
    let mut r = LegacyRandom::new(SEED);
    let (mut lo, mut hi) = (i32::MAX, i32::MIN);
    for _ in 0..1000 {
        let t = hooked_after(&mut r);
        lo = lo.min(t);
        hi = hi.max(t);
    }
    assert!(lo >= 20 && hi <= 80, "approach {lo}..{hi}");
    let (mut lo, mut hi) = (i32::MAX, i32::MIN);
    for _ in 0..1000 {
        let t = lured_after(&mut r, 0);
        lo = lo.min(t);
        hi = hi.max(t);
    }
    assert!(lo >= 100 && hi <= 600, "lure {lo}..{hi}");
}

/// The player `1` set at `x` holding `main`, with the fact of a living player.
fn set_1(h: &mut Herd, main: u32, x: f64) {
    let p = Player { id: 1, x, y: at().y, z: at().z, main, ..Default::default() };
    horizon_ecs_proto::bridge::players::set_player(&mut h.world, p, PlayerFact { alive: true, targetable: true, hero: None });
}

#[test]
fn a_waiting_hook_goes_when_its_player_leaves_or_drops_the_rod() {
    let rod = horizon_ecs_proto::items::item_id("minecraft:fishing_rod");
    let mut h = Herd::with_slab(16, true, false);
    let (hook, _) = land(&mut h);
    set_1(&mut h, rod, at().x);
    assert!(h.world.get::<Wait>(hook).is_some(), "a player holding the rod keeps its hook");
    set_1(&mut h, 0, at().x);
    assert!(h.world.get_entity(hook).is_err(), "a hook whose rod is dropped stayed");
    let mut h = Herd::with_slab(16, true, false);
    let (hook, _) = land(&mut h);
    set_1(&mut h, rod, at().x + 40.0);
    assert!(h.world.get_entity(hook).is_err(), "a hook over 32 blocks away stayed");
    let mut h = Herd::with_slab(16, true, false);
    let (hook, _) = land(&mut h);
    horizon_ecs_proto::bridge::players::remove_player(&mut h.world, 1);
    assert!(h.world.get_entity(hook).is_err(), "a hook whose player left stayed");
}

#[test]
fn a_waiting_float_rests_and_no_system_visits_it_between_its_dates() {
    let mut h = Herd::with_slab(16, true, false);
    let (hook, wt) = land(&mut h);
    assert!(wt.left > 10, "the lure is too short to test the rest");
    let at_rest = *h.world.get::<Pos>(hook).unwrap();
    assert_eq!(h.world.get::<Vel>(hook).unwrap().0, Vec3::ZERO, "the waiting float moves");
    for _ in 0..5 {
        stand(&mut h);
        h.tick();
    }
    assert_eq!(h.world.get::<Pos>(hook).unwrap().0, at_rest.0, "the waiting float was visited between its dates");
}

#[test]
fn a_block_changed_under_a_waiting_float_puts_it_back_in_the_physics() {
    let mut h = Herd::with_slab(16, true, false);
    let (hook, _) = land(&mut h);
    let p = h.world.get::<Pos>(hook).unwrap().0;
    horizon_ecs_proto::fishing_hook::block_changed(&mut h.world, [p.x.floor() as i32, p.y.floor() as i32 + 1, p.z.floor() as i32]);
    assert!(h.world.get::<Wait>(hook).is_none(), "the float kept its wait after a block changed under it");
}
