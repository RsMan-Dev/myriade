//! The sulfur cube holding a block stands still (`ai_step` switches MOVE, LOOK and JUMP off with the body) and moves again once it
//! lets go, in the same scene as the rabbit test's herd (`HORIZON_SPECIES=minecraft:sulfur_cube`, `HORIZON_TEST_RESOURCES` set).
//! NON FAIT : pousse et brûlure (`push_on_touch`, `burn_on_touch`) : elles attendent `Events.touched`, `Events.item_touched` et la vitesse
//! du joueur, fournis par le socle ; la même scène côté `Mob` (`cube::adopt_body` via la classe de test `pub(crate)`, hors d'atteinte d'ici) ; la comparaison
//! des positions reste à écrire quand le `Mob` expose un constructeur public de cube porteur de bloc.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::sulfur_cube::ARCHETYPES;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::behaviours::swallow_hold::{accepts, hold, release};
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::{go_to_at, Entered};
use std::sync::Arc;

fn is(name: &str) -> bool {
    have_data() && !wrong_species(&[name])
}

/// A cube that holds a block does not walk toward a point; released, it walks there.
#[test]
fn a_cube_holding_a_block_stands_still_and_moves_again_once_released() {
    if horizon_ecs_proto::cow::skip_unless_small_body("a_cube_holding_a_block_stands_still_and_moves_again_once_released") {
        return;
    }
    if !is("minecraft:sulfur_cube") {
        return;
    }
    let mut h = Herd::with_slab(60, true, false);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 7);
    h.world.resource_mut::<Entered>().0.push(e);
    let start = h.world.get::<Pos>(e).unwrap().0;
    let goal = [start.x, start.y, start.z + 8.0];

    hold(&mut h.world, e, Arc::from("minecraft:stone"), &ARCHETYPES[0]);
    go_to_at(&mut h.world, e, goal, 0.2, 1);
    for _ in 0..100 {
        h.tick();
    }
    let p = h.world.get::<Pos>(e).unwrap().0;
    let held_drift = (p.x - start.x).hypot(p.z - start.z);
    eprintln!("holding: drift {held_drift:.4}");
    assert!(held_drift < 1e-6, "a cube holding a block moved by {held_drift}");

    release(&mut h.world, e);
    go_to_at(&mut h.world, e, goal, 0.2, 1);
    for _ in 0..100 {
        h.tick();
    }
    let p: Vec3 = h.world.get::<Pos>(e).unwrap().0;
    let free_travel = (p.x - start.x).hypot(p.z - start.z);
    eprintln!("released: travel {free_travel:.3}");
    assert!(free_travel > 1.0, "a released cube did not walk: {free_travel}");
}

/// The swallow filter: a cube takes only the items on its list (`isSwallowableItem`), by name.
#[test]
fn a_cube_swallows_only_the_items_on_its_list() {
    let list: &[&str] = &["minecraft:tnt"];
    assert!(accepts("minecraft:tnt", list));
    assert!(!accepts("minecraft:stone", list));
}
