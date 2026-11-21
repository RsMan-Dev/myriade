//! Point 0: the `on_hit` hook of a melee blow runs only when the victim took damage, and gets the damage dealt.
use bevy_ecs::prelude::*;
use horizon_ecs_proto::cow::{spawn_species, DamageQueue, Health, Herd, SpeciesTable};
use horizon_ecs_proto::hostile::{ensure_attack, Attack};
use horizon_ecs_proto::targeting::Who;
use std::sync::atomic::{AtomicU32, Ordering::SeqCst};

static DEALT_X100: AtomicU32 = AtomicU32::new(0);
fn record(_: &mut World, _: Entity, _: Who, dealt: f32) {
    DEALT_X100.store((dealt * 100.0) as u32, SeqCst);
}

#[test]
fn the_hook_runs_with_the_damage_when_the_blow_lands() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let cow = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
    let attacker = spawn_species(&mut h.world, &cow, 0.5, 0.5, 61);
    let prey = spawn_species(&mut h.world, &cow, 2.5, 0.5, 62);
    ensure_attack(&mut h.world, attacker);
    h.world.get_mut::<Attack>(attacker).unwrap().on_hit = Some(record);
    let before = h.world.get::<Health>(prey).unwrap().0;
    h.world.resource_mut::<DamageQueue>().add_by_entity(prey, 3.0, (0.5, 0.5), attacker);
    h.tick();
    h.tick();
    let lost = before - h.world.get::<Health>(prey).unwrap().0;
    assert!(lost > 0.0);
    assert_eq!(DEALT_X100.load(SeqCst), (lost * 100.0) as u32);
}
