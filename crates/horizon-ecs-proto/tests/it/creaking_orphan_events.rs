//! The creaking's heart link by events: the heart is asked when the creaking loads (`MiscEvent::HeartAsked`), never at rest; removing the
//! key `home_pos` unlinks it; a crumbling creature is no longer a target (`Creaking.tick`, `TargetingConditions` on the dead).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_core::{
    geometry::Vec3,
    nbt::{Compound, JavaString, Tag},
};
use horizon_ecs_proto::{
    behaviours::stare_freeze::{Params, StareFreeze},
    bridge::host_creaking::{HeartLink, HeartWatch},
    colony_register::MiscOut,
    cow::*,
    damage_link::DamageLink,
    host_out_misc::MiscEvent,
    nbt::{self, Saved},
    targeting::{self, Who},
    tear_down,
};

fn asked(h: &Herd) -> Vec<(Entity, [i32; 3])> {
    h.world.get_resource::<MiscOut>().map_or_else(Vec::new, |m| m.0.iter().filter_map(|e| if let MiscEvent::HeartAsked { entity, pos } = e { Some((*entity, *pos)) } else { None }).collect())
}

#[test]
fn an_orphan_fades_after_20_ticks_with_no_crumbling() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let cow = spawn_species(&mut h.world, &species_named("minecraft:cow"), 0.5, 0.5, 5);
    h.world.resource_mut::<Entered>().0.push(cow);
    h.tick();
    tear_down::fade(&mut h.world, cow);
    assert_eq!(h.world.get::<Health>(cow).unwrap().0, 0.0);
    for _ in 0..(tear_down::FADE_TICKS - 2) {
        h.tick();
    }
    assert!(h.world.get_entity(cow).is_ok(), "still lying before the 20th tick");
    for _ in 0..4 {
        h.tick();
    }
    assert!(h.world.get_entity(cow).is_err(), "gone after 20 ticks");
    assert!(!h.world.get_resource::<MiscOut>().is_some_and(|m| m.0.iter().any(|e| matches!(e, MiscEvent::Crumbled { .. }))));
}

fn creaking(h: &mut Herd) -> Entity {
    h.world.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), TypeName("minecraft:creaking"), Saved, StareFreeze(&Params::DEFAULT))).id()
}

fn home(pos: [i32; 3]) -> Compound {
    let mut c = Compound::new();
    c.insert(JavaString::from("home_pos"), Tag::IntArray(pos.to_vec()));
    c
}

#[test]
fn a_loaded_creaking_asks_its_heart_once_and_nothing_at_rest() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let c = creaking(&mut h);
    assert!(asked(&h).is_empty(), "a creaking with no link asks nothing");
    assert_eq!(nbt::load(&mut h.world, c, &home([5, 64, -7])), Some(0));
    assert_eq!(asked(&h), vec![(c, [5, 64, -7])]);
    for _ in 0..10 {
        h.tick();
    }
    assert_eq!(asked(&h).len(), 1, "no question while nothing changes");
}

#[test]
fn the_watch_follows_the_link_and_removing_the_key_unlinks() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let c = creaking(&mut h);
    nbt::load(&mut h.world, c, &home([5, 64, -7]));
    assert_eq!(h.world.resource::<HeartWatch>().0.get(&[5, 64, -7]), Some(&vec![c]), "a block change at the heart finds its creaking");
    assert!(h.world.get::<DamageLink>(c).is_some());

    assert!(nbt::remove(&mut h.world, c, "home_pos"));
    assert!(h.world.get::<HeartLink>(c).is_none());
    assert!(h.world.get::<DamageLink>(c).is_none(), "the damage guard goes with the link");
    assert!(h.world.resource::<HeartWatch>().0.is_empty());
    assert!(!nbt::remove(&mut h.world, c, "home_pos"), "nothing left to remove");
}

#[test]
fn a_crumbling_creature_is_not_a_target() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let cow = spawn_species(&mut h.world, &species_named("minecraft:cow"), 0.5, 0.5, 5);
    h.world.resource_mut::<Entered>().0.push(cow);
    h.tick();
    assert!(!targeting::info(&h.world, Who::Entity(cow)).unwrap().exempt);
    tear_down::start(&mut h.world, cow, "entity.creaking.death");
    assert!(targeting::info(&h.world, Who::Entity(cow)).unwrap().exempt);
}
