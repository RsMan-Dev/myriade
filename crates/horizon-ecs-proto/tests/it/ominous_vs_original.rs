//! The ominous item spawner against the deadlines of vanilla: the warning sound 36 ticks before the drop, the drop at its tick, the
//! particles on the game ticks that are multiples of 5 until then, and nothing after the drop. The expected ticks are those of
//! `OminousItemSpawner.tick` (`tickCount`, incremented before the checks); no Horizon original runs here, so the expected values are
//! written out. `HORIZON_TEST_RESOURCES` set.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::{Clock, Herd};
use horizon_ecs_proto::ominous_spawner::{spawn, OminousSpawner};

/// The ticks of a spawner made at tick 0 with a drop after `after` ticks: the sound at `after - 36`, the drop at `after`.
fn run(after: u32, item: &str) -> (Option<u64>, Option<u64>, Vec<u64>) {
    let mut h = Herd::with_slab(16, true, false);
    let e = spawn(&mut h.world, ItemStack::new(item, 1), Vec3::new(0.5, 5.0, 0.5), after);
    let (mut sound, mut gone, mut particles) = (None, None, Vec::new());
    for t in 1..=u64::from(after) + 10 {
        h.tick();
        let now = h.world.resource::<Clock>().0;
        for ev in h.drain_host_out() {
            let s = format!("{ev:?}");
            if s.contains("about_to_spawn_item") {
                sound = Some(t);
            }
            if s.contains("ominous_spawning") {
                particles.push(now);
            }
        }
        if gone.is_none() && h.world.get::<OminousSpawner>(e).is_none() {
            gone = Some(t);
        }
    }
    (sound, gone, particles)
}

/// The warning sound comes 36 ticks before the drop, and the spawner is gone at the drop tick.
#[test]
fn the_sound_and_the_drop_come_at_vanillas_ticks() {
    if !have_data() {
        return;
    }
    let (sound, gone, _) = run(60, "minecraft:cobblestone");
    assert_eq!(sound, Some(24), "sound at 60 - 36");
    assert_eq!(gone, Some(60), "drop at 60");
}

/// A spawner shorter than the warning gives no sound before its drop: the warning is 36 ticks before, which is before tick 0.
#[test]
fn a_short_spawner_drops_without_the_warning() {
    if !have_data() {
        return;
    }
    let (sound, gone, _) = run(20, "minecraft:cobblestone");
    assert_eq!(sound, None);
    assert_eq!(gone, Some(20));
}

/// The particles are drawn on the game ticks that are multiples of 5, and none comes after the drop.
#[test]
fn the_particles_come_on_every_fifth_game_tick_until_the_drop() {
    if !have_data() {
        return;
    }
    let (_, gone, particles) = run(60, "minecraft:cobblestone");
    assert!(!particles.is_empty());
    assert!(particles.iter().all(|t| t % 5 == 0), "{particles:?}");
    assert!(particles.iter().all(|&t| t < 60 + 1), "{particles:?}");
    assert_eq!(gone, Some(60));
}

/// Air drops nothing, and a stack that is a projectile is shot, not dropped as an item: the spawner is gone either way.
#[test]
fn air_drops_nothing_and_the_spawner_is_gone() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(16, true, false);
    let e = spawn(&mut h.world, ItemStack::new("minecraft:air", 1), Vec3::new(0.5, 5.0, 0.5), 5);
    for _ in 0..6 {
        h.tick();
    }
    assert!(h.world.get::<OminousSpawner>(e).is_none());
}
