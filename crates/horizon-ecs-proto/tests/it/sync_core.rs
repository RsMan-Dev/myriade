//! The synched entries of a bridged creature (greffe L0, lot 04): without effect or fire it sends its base entries only; burning
//! adds entry 0 with the fire bit; the core's indices are the ones the tracker can read from the components.
use crate::common::*;
use horizon_ecs_proto::bridge::{sync, sync_core};
use horizon_ecs_proto::cow::*;

#[test]
fn a_creature_without_fire_sends_only_its_base_entries_then_fire_adds_entry_0() {
    let mut h = Herd::with_slab_awake(60, true, false);
    let sp = *species();
    let a = spawn_species(&mut h.world, &sp, 0.5, 0.5, 11);
    h.world.entity_mut(a).insert(Health(20.0));
    let mut out = Vec::new();
    sync::sync_entries(&h.world, a, &mut out);
    assert_eq!(out, vec![(9, sync::SyncValue::Float(20.0))], "no effect, no fire: the health alone");
    set_on_fire(&mut h.world, a, 100);
    out.clear();
    sync::sync_entries(&h.world, a, &mut out);
    assert!(out.contains(&(0, sync::SyncValue::Byte(1))), "on fire: entry 0 with the fire bit 0x01");
    assert!(out.contains(&(9, sync::SyncValue::Float(20.0))), "the base entries stay");
}

#[test]
fn the_core_indices_are_the_ones_it_can_read() {
    let indices: Vec<u8> = sync_core::SLOTS.iter().map(|s| s.index).collect();
    assert_eq!(indices, [0, 1, 2, 3, 4, 5, 7, 9, 11, 14, 15, 16, 17]);
}
