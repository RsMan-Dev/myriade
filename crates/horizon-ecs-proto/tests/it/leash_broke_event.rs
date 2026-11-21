//! `Leashable.tickLeash`: a lead snaps once the creature's box centre is more than 12 blocks (the default snapping distance) from the holder's;
//! the host is told with `MiscEvent::LeashBroke` (`sound` true: the snap). A holder that vanished ends the lead without the snap.
use crate::common::*;
use horizon_ecs_proto::{
    colony_register::MiscOut,
    cow::*,
    host_out_misc::MiscEvent,
    leash::{self, Holder},
};

fn broke(h: &Herd) -> Vec<MiscEvent> {
    h.world.get_resource::<MiscOut>().map_or_else(Vec::new, |m| m.0.iter().copied().filter(|e| matches!(e, MiscEvent::LeashBroke { .. })).collect())
}

fn cow(h: &mut Herd) -> (bevy_ecs::entity::Entity, i32) {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:cow").expect("the cow");
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 61);
    let y = h.world.get::<Pos>(e).unwrap().0.y.floor() as i32;
    (e, y)
}

#[test]
fn a_knot_within_12_blocks_holds() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let (e, y) = cow(&mut h);
    assert!(leash::attach_from_save(&mut h.world, e, Holder::Knot([10, y, 0])));
    for _ in 0..5 {
        h.tick();
    }
    assert!(broke(&h).is_empty(), "10 blocks is within the snapping distance");
    assert!(leash::is_leashed(&h.world, e));
}

#[test]
fn a_knot_past_12_blocks_snaps_the_lead_with_the_sound() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let (e, y) = cow(&mut h);
    let knot = [14, y, 0];
    assert!(leash::attach_from_save(&mut h.world, e, Holder::Knot(knot)));
    for _ in 0..3 {
        h.tick();
    }
    let events = broke(&h);
    assert_eq!(events.len(), 1, "one break: {events:?}");
    let MiscEvent::LeashBroke { entity, sound, knot: k, .. } = events[0] else { unreachable!() };
    assert_eq!((entity, sound, k), (e, true, Some(knot)));
    assert!(!leash::is_leashed(&h.world, e), "the lead is off");
}

#[test]
fn a_vanished_holder_ends_the_lead_without_the_snap() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let (e, _) = cow(&mut h);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
    let holder = spawn_species(&mut h.world, &sp, 3.5, 3.5, 62);
    assert!(leash::attach_from_save(&mut h.world, e, Holder::Entity(holder)));
    h.world.entity_mut(holder).despawn();
    for _ in 0..3 {
        h.tick();
    }
    let events = broke(&h);
    assert_eq!(events.len(), 1, "{events:?}");
    let MiscEvent::LeashBroke { sound, knot, .. } = events[0] else { unreachable!() };
    assert_eq!((sound, knot), (false, None));
}
