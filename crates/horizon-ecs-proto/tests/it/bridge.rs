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
    assert_eq!(view::drain(&mut h.world), vec![e], "a bound entity is queued once, for its first view");
    assert!(view::drain(&mut h.world).is_empty(), "nothing moved: the queue is empty");
    h.world.entity_mut(e).insert(Disabled);
    view::mark(&mut h.world, e);
    assert!(view::drain(&mut h.world).is_empty(), "a sleeping entity is not queued");
    assert!(view::view(&h.world, e).is_none(), "a sleeping entity has no view");
}
