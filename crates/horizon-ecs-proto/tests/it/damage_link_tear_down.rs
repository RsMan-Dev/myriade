//! A damage-linked entity takes nothing: the blow goes to its link once per window (`MiscEvent::LinkedHurt`); a crumbling one lasts 46
//! ticks, then vanishes with no loot (`MiscEvent::Crumbled`).
use crate::common;
use bevy_ecs::entity::Entity;
use common::*;
use horizon_ecs_proto::colony_register::MiscOut;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::damage_link::{DamageLink, Verdict, verdict};
use horizon_ecs_proto::goals::Entered;
use horizon_ecs_proto::host_out_misc::MiscEvent;
use horizon_ecs_proto::tear_down::{self, Crumbling};
use horizon_ecs_proto::targeting::Who;

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:cow"])
}
fn species_named(name: &str) -> Species {
    *species_table().0.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name}"))
}

fn linked_cow(h: &mut Herd) -> Entity {
    let cow = spawn_species(&mut h.world, &species_named("minecraft:cow"), 0.5, 0.5, 5);
    h.world.resource_mut::<Entered>().0.push(cow);
    h.world.entity_mut(cow).insert(DamageLink::default());
    h.tick();
    cow
}

fn blow(h: &mut Herd, e: Entity, by: Option<Who>) {
    h.world.resource_mut::<DamageQueue>().0.push(Hit { target: e, amount: 1.0, cause: Cause::Entity, damage: "mob_attack", from: None, by, direct: true, power: None, projectile: None });
    h.tick();
}

fn hurts(h: &Herd) -> Vec<bool> {
    h.world.resource::<MiscOut>().0.iter().filter_map(|m| if let MiscEvent::LinkedHurt { by_player, .. } = m { Some(*by_player) } else { None }).collect()
}

#[test]
fn the_verdict_follows_the_jar_order() {
    assert_eq!(verdict(Cause::OutOfWorld, true, false), Verdict::Takes);
    assert_eq!(verdict(Cause::Entity, true, true), Verdict::Refused);
    assert_eq!(verdict(Cause::Lava, false, false), Verdict::Refused);
    assert_eq!(verdict(Cause::Entity, false, true), Verdict::Handed);
}

#[test]
fn a_blow_goes_to_the_link_once_per_window() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let cow = linked_cow(&mut h);
    let full = h.world.get::<Health>(cow).unwrap().0;
    blow(&mut h, cow, Some(Who::Player(1)));
    assert_eq!(h.world.get::<Health>(cow).unwrap().0, full, "no damage");
    assert_eq!(hurts(&h), vec![true]);
    blow(&mut h, cow, Some(Who::Player(1)));
    assert_eq!(hurts(&h).len(), 1, "the window refuses the second blow");
    for _ in 0..8 {
        h.tick();
    }
    blow(&mut h, cow, Some(Who::Player(1)));
    assert_eq!(hurts(&h).len(), 2, "a blow after the window goes through to the link again");
}

#[test]
fn an_environmental_hit_is_refused_and_the_void_goes_through() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let cow = linked_cow(&mut h);
    let full = h.world.get::<Health>(cow).unwrap().0;
    h.world.resource_mut::<DamageQueue>().add(cow, 1.0, Cause::Lava);
    h.tick();
    assert_eq!(h.world.get::<Health>(cow).unwrap().0, full);
    assert!(hurts(&h).is_empty());
    h.world.resource_mut::<DamageQueue>().add(cow, 1.0, Cause::OutOfWorld);
    h.tick();
    assert!(h.world.get::<Health>(cow).unwrap().0 < full);
}

#[test]
fn a_crumbling_death_lasts_46_ticks_and_leaves_no_loot() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let cow = linked_cow(&mut h);
    tear_down::start(&mut h.world, cow, "entity.creaking.death");
    assert!(h.world.get::<Crumbling>(cow).is_some());
    for _ in 0..(tear_down::TICKS - 2) {
        h.tick();
    }
    assert!(h.world.get_entity(cow).is_ok(), "still there before the 46th tick");
    blow(&mut h, cow, Some(Who::Player(1)));
    assert!(hurts(&h).is_empty(), "a crumbling entity takes no more blows");
    for _ in 0..4 {
        h.tick();
    }
    assert!(h.world.get_entity(cow).is_err(), "gone after the 46 ticks");
    assert!(h.world.resource::<MiscOut>().0.iter().any(|m| matches!(m, MiscEvent::Crumbled { entity, .. } if *entity == cow)));
}
