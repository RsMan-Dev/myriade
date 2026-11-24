//! The network-id table and the view of a bridged creature (greffe L0): an id is bound, refused when held, freed by removal or
//! despawn and taken again; the view reads the components of a creature made by `spawn_species`; the queue of modified entities
//! is empty at rest and never holds a sleeping one.
use crate::common::*;
use bevy_ecs::{entity_disabling::Disabled, prelude::*};
use horizon_ecs_proto::bridge::{ids, view};
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::{Head, Uid};

#[test]
fn ids_are_bound_refused_freed_and_reused() {
    let mut h = Herd::with_slab_awake(60, true, false);
    let sp = *species();
    let a = spawn_species(&mut h.world, &sp, 0.5, 0.5, 11);
    let b = spawn_species(&mut h.world, &sp, 1.5, 0.5, 12);
    assert!(ids::bind(&mut h.world, a, 5));
    assert!(!ids::bind(&mut h.world, b, 5), "an id held by another entity is refused");
    assert!(!ids::bind(&mut h.world, a, 6), "a bridged entity keeps its id");
    assert_eq!((ids::id_of(&h.world, a), h.world.resource::<ids::NetIds>().entity(5)), (Some(5), Some(a)));
    ids::unbind(&mut h.world, a);
    assert!(h.world.resource::<ids::NetIds>().entity(5).is_none(), "removing the marker frees the id");
    assert!(ids::bind(&mut h.world, b, 5), "a freed id is taken again");
    assert!(h.world.despawn(b));
    assert!(h.world.resource::<ids::NetIds>().entity(5).is_none(), "a despawned entity frees its id");
    assert!(ids::bind(&mut h.world, a, 5), "the same id goes to another entity once freed");
}

#[test]
fn view_reads_the_components_of_a_spawned_creature() {
    let mut h = Herd::with_slab_awake(60, true, false);
    if !ground_walker() {
        return;
    }
    let sp = *species();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 21);
    let v = view::view(&h.world, e).expect("a live creature has a view");
    let p = h.world.get::<Pos>(e).unwrap().0;
    let vel = h.world.get::<Vel>(e).unwrap().0;
    assert_eq!(v.position, [p.x, p.y, p.z]);
    assert_eq!(v.velocity, [vel.x, vel.y, vel.z]);
    assert_eq!(v.yaw, h.world.get::<Yaw>(e).unwrap().0);
    assert_eq!(v.kind, h.world.get::<TypeName>(e).unwrap().0);
    assert_eq!(v.uid, h.world.get::<Uid>(e).unwrap().0);
    let head = h.world.get::<Head>(e).map(|x| (x.yaw.to_bits(), x.pitch.to_bits()));
    assert_eq!(v.head.map(|(y, p)| (y.to_bits(), p.to_bits())), head);
    assert!(v.rider.is_none() && v.passenger.is_none() && v.leash.is_none());
}

#[test]
fn modified_queue_is_empty_at_rest_and_skips_a_sleeping_entity() {
    let mut h = Herd::with_slab_awake(60, true, false);
    let sp = *species();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 31);
    assert!(ids::bind(&mut h.world, e, 7));
    view::mark(&mut h.world, e);
    assert_eq!(view::drain(&mut h.world), vec![(e, true)], "a bound entity is queued once, for its first view");
    assert!(view::drain(&mut h.world).is_empty(), "nothing moved: the queue is empty");
    h.world.entity_mut(e).insert(Disabled);
    view::mark(&mut h.world, e);
    assert!(view::drain(&mut h.world).is_empty(), "a sleeping entity is not queued");
    assert!(view::view(&h.world, e).is_none(), "a sleeping entity has no view");
}

#[test]
fn motion_only_marks_are_drained_without_the_data_flag_unless_marked_in_full() {
    let mut h = Herd::with_slab_awake(60, true, false);
    let sp = *species();
    let (a, b) = (spawn_species(&mut h.world, &sp, 0.5, 0.5, 41), spawn_species(&mut h.world, &sp, 1.5, 0.5, 42));
    view::drain(&mut h.world);
    view::mark_motion(&mut h.world, b);
    view::mark_motion(&mut h.world, a);
    view::mark_motion(&mut h.world, a);
    let both = view::drain(&mut h.world);
    assert_eq!(both.iter().map(|x| x.1).collect::<Vec<_>>(), vec![false, false], "moved only: no metadata to read");
    assert!(both[0].0 < both[1].0, "by index, each once");
    view::mark_motion(&mut h.world, a);
    view::mark(&mut h.world, a);
    assert_eq!(view::drain(&mut h.world), vec![(a, true)], "marked in full as well: the data is read");
    assert!(view::drain(&mut h.world).is_empty());
}

/// The guard of the motion-only queue (test only): an entity drained without the data flag must read the same synchronized entries as at the
/// tick before; a difference is a write of a synchronized component that lacks its `view::mark` (it was hidden by the movement).
#[test]
fn entities_drained_motion_only_keep_their_synchronized_entries() {
    use std::collections::HashMap;
    let mut h = Herd::with_slab_awake(60, true, false);
    let sp = *species();
    h.world.init_resource::<view::Modified>();
    let es: Vec<Entity> = (0..200).map(|i| spawn_species(&mut h.world, &sp, f64::from(i % 20) * 2.0 - 20.0, f64::from(i / 20) * 2.0 - 10.0, 100 + u64::from(i))).collect();
    for (i, &e) in es.iter().enumerate() {
        assert!(ids::bind(&mut h.world, e, (i + 1) as _));
    }
    let read = |w: &World, e: Entity| {
        let mut out = Vec::new();
        horizon_ecs_proto::bridge::sync::sync_entries(w, e, &mut out);
        out
    };
    let mut last: HashMap<Entity, Vec<_>> = HashMap::new();
    let mut motion_only = 0;
    for tick in 0..100 {
        h.tick();
        for (e, full) in view::drain(&mut h.world) {
            if !full {
                motion_only += 1;
                if let Some(before) = last.get(&e) {
                    assert_eq!(&read(&h.world, e), before, "tick {tick}: {e:?} was drained motion-only but its synchronized entries changed");
                }
            }
        }
        for &e in &es {
            last.insert(e, read(&h.world, e));
        }
    }
    assert!(motion_only > 0, "the herd walks: some entities are drained motion-only");
}
