//! The lightning bolt against the jar's script (`LightningBolt.tick`, read with javap): the strike on its first tick (two sounds, fire
//! through the host's block facts), then life 2 -> 1 -> 0 -> -1, and the discard at the tick its flashes are spent. Expected values are
//! written from the jar, not from the code.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::{Herd, Pos};
use horizon_ecs_proto::goals::Events;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::lightning::*;
use horizon_world::registry::BlockStateId;

/// The host's block facts: fire stands everywhere, the rod and the copper are absent.
struct Stub;
impl BoltLevel for Stub {
    fn may_spread_fire(&self, _: [i32; 3]) -> bool {
        true
    }
    fn fire_state(&self, _: [i32; 3]) -> Option<BlockStateId> {
        Some(BlockStateId(7))
    }
    fn powered_rod(&self, _: [i32; 3]) -> Option<BlockStateId> {
        None
    }
    fn copper_strike(&self, _: [i32; 3]) -> Option<BlockStateId> {
        None
    }
}

fn scene() -> Herd {
    let mut h = Herd::with_slab_awake(16, true, false);
    h.world.insert_resource(Strikes(Some(Box::new(Stub))));
    h
}

fn drain(h: &mut Herd) -> Vec<HostEvent> {
    h.world.resource_mut::<Events>().host.drain().collect()
}

/// The strike's sounds come on its first tick, the thunder then the impact, with the jar's volumes and pitch ranges.
#[test]
fn a_strike_sounds_on_its_first_tick_as_the_jar() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let at = Vec3::new(0.5, 64.0, 0.5);
    strike(&mut h.world, at, false, 7);
    assert!(drain(&mut h).is_empty(), "nothing before the first tick");
    h.tick();
    let sounds: Vec<_> = drain(&mut h)
        .into_iter()
        .filter_map(|e| match e {
            HostEvent::Sound { name, volume, pitch, .. } => Some((name, volume, pitch)),
            _ => None,
        })
        .collect();
    assert_eq!(sounds.len(), 2, "{sounds:?}");
    assert_eq!(sounds[0].0, "entity.lightning_bolt.thunder");
    assert_eq!(sounds[0].1, 10000.0);
    assert!((0.8..=1.0).contains(&sounds[0].2), "{sounds:?}");
    assert_eq!(sounds[1].0, "entity.lightning_bolt.impact");
    assert_eq!(sounds[1].1, 2.0);
    assert!((0.5..=0.7).contains(&sounds[1].2), "{sounds:?}");
}

/// Without flashes left, a bolt struck at tick 0 is spent at tick 3 (life 2, 1, 0, then -1 with no flash to come): it lives two
/// ticks of thunder and no more, and the fire it set stands on the strike's block.
#[test]
fn a_bolt_without_flashes_ends_at_the_third_tick() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let e = strike(&mut h.world, Vec3::new(0.5, 64.0, 0.5), false, 7);
    h.world.get_mut::<horizon_ecs_proto::lightning::Bolt>(e).unwrap().flashes = 0;
    h.tick();
    let fires: Vec<_> = drain(&mut h)
        .into_iter()
        .filter(|ev| matches!(ev, HostEvent::SetBlock { pos: [0, 64, 0], .. }))
        .collect();
    assert!(!fires.is_empty(), "the strike's block takes fire at normal difficulty");
    assert!(h.world.get::<Bolt>(e).is_some(), "alive after tick 1");
    h.tick();
    assert!(h.world.get::<Bolt>(e).is_some(), "alive after tick 2");
    h.tick();
    assert!(h.world.get::<Bolt>(e).is_none(), "spent at tick 3");
    assert!(h.world.get::<Pos>(e).is_none());
}

/// A skeleton trap's bolt is visual: it sounds like the others and sets no fire.
#[test]
fn a_visual_bolt_sounds_and_sets_no_fire() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    strike(&mut h.world, Vec3::new(0.5, 64.0, 0.5), true, 7);
    h.tick();
    let ev = drain(&mut h);
    assert_eq!(ev.iter().filter(|e| matches!(e, HostEvent::Sound { .. })).count(), 2);
    assert!(ev.iter().all(|e| !matches!(e, HostEvent::SetBlock { .. })));
}
