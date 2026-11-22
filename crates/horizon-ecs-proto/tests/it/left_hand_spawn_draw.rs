//! `Mob.finalizeSpawn` draws the hand (5% left-handed) for any creature: the marker follows the draw and the mob flags (index 15) carry the
//! bit 2.
use crate::common;
use common::*;
use horizon_core::random::LegacyRandom;
use horizon_ecs_proto::bridge::sync::{SyncValue, sync_entries};
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::left_handed::LeftHanded;
use horizon_ecs_proto::spawn_finalize::{Finalize, finalize};
use horizon_ecs_proto::spawn_reason::SpawnReason;

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:cow"])
}
fn species_named(name: &str) -> Species {
    *species_table().0.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name}"))
}

#[test]
fn the_spawn_draw_sets_the_marker_and_the_synced_flag() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let mut lefties = 0;
    for seed in 0..400u64 {
        let cow = spawn_species(&mut h.world, &species_named("minecraft:cow"), 0.5, 0.5, seed);
        let mut r = LegacyRandom::new(seed as i64);
        let out = finalize(&mut h.world, cow, Finalize::Mob, None, SpawnReason::Natural, 1.0, &mut r);
        assert_eq!(h.world.get::<LeftHanded>(cow).is_some(), out.mob.left_handed);
        let mut entries = Vec::new();
        sync_entries(&h.world, cow, &mut entries);
        let flags = entries.iter().find(|(i, _)| *i == 15).map(|(_, v)| matches!(v, SyncValue::Byte(b) if b & 2 != 0));
        assert_eq!(flags.unwrap_or(false), out.mob.left_handed, "the mob flags carry the left hand");
        lefties += usize::from(out.mob.left_handed);
    }
    assert!((1..100).contains(&lefties), "about 5% of the spawns are left-handed, got {lefties}");
}
