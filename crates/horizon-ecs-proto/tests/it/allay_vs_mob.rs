//! The allay fetches the item of its main hand from the ground into its pocket (`go_to_wanted` with `hand`), in the same scene as the
//! sulfur cube test (`HORIZON_SPECIES=minecraft:allay`, `HORIZON_TEST_RESOURCES` set).
//! NON FAIT : la comparaison de délai avec le `Mob` (`allay::Fetch`, `mobs/allay.rs`) : aucun point d'entrée public ne fait marcher le
//! `Fetch` du `Mob` hors de sa scène ; l'assertion est donc côté ECS seul.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::{set_main_hand, Entered};
use horizon_ecs_proto::item;
use horizon_ecs_proto::pocket::Pocket;

fn is(name: &str) -> bool {
    have_data() && !wrong_species(&[name])
}

/// An allay with `hand` in its main hand, and `ground` (3 items) dropped `dx` blocks away from it.
fn scene(hand: &str, ground: &str, dx: f64) -> (Herd, Entity, Entity) {
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 7);
    h.world.resource_mut::<Entered>().0.push(e);
    set_main_hand(&mut h.world, e, Some(ItemStack::new(hand, 1)));
    let at = h.world.get::<Pos>(e).unwrap().0;
    let it = item::drop_item(&mut h.world, ItemStack::new(ground, 3), Vec3::new(at.x + dx, at.y, at.z), Vec3::new(0.0, 0.0, 0.0), 0);
    (h, e, it)
}

/// An allay whose hand holds a cod takes the cod lying 10 blocks away into its pocket.
#[test]
fn an_allay_with_a_cod_in_hand_takes_the_cod_on_the_ground_into_its_pocket() {
    if horizon_ecs_proto::cow::skip_unless_small_body("an_allay_with_a_cod_in_hand_takes_the_cod_on_the_ground_into_its_pocket") {
        return;
    }
    if !is("minecraft:allay") {
        return;
    }
    let (mut h, e, it) = scene("minecraft:cod", "minecraft:cod", 10.0);
    for _ in 0..200 {
        h.tick();
    }
    let held = h.world.get::<Pocket>(e).and_then(|p| p.held()).map(|s| s.count);
    assert_eq!(held, Some(3), "the pocket holds the 3 cod of the ground");
    assert!(h.world.get_entity(it).is_err(), "the item was taken off the ground");
}

/// An item of another kind than the one in hand is left on the ground.
#[test]
fn an_allay_does_not_take_an_item_that_differs_from_its_hand() {
    if horizon_ecs_proto::cow::skip_unless_small_body("an_allay_does_not_take_an_item_that_differs_from_its_hand") {
        return;
    }
    if !is("minecraft:allay") {
        return;
    }
    let (mut h, e, it) = scene("minecraft:cod", "minecraft:stone", 10.0);
    for _ in 0..200 {
        h.tick();
    }
    assert!(h.world.get::<Pocket>(e).and_then(|p| p.held()).is_none(), "the pocket stays empty");
    assert!(h.world.get_entity(it).is_ok(), "the stone stays on the ground");
}

/// A pocket full of another item refuses the cod, even when the hand holds one.
#[test]
fn an_allay_with_a_pocket_full_of_another_item_does_not_take_the_cod() {
    if horizon_ecs_proto::cow::skip_unless_small_body("an_allay_with_a_pocket_full_of_another_item_does_not_take_the_cod") {
        return;
    }
    if !is("minecraft:allay") {
        return;
    }
    let (mut h, e, it) = scene("minecraft:cod", "minecraft:cod", 10.0);
    h.world.entity_mut(e).insert(Pocket(Some(ItemStack::new("minecraft:stone", 64))));
    for _ in 0..200 {
        h.tick();
    }
    let held = h.world.get::<Pocket>(e).and_then(|p| p.held()).map(|s| s.count);
    assert_eq!(held, Some(64), "the pocket keeps its stone");
    assert!(h.world.get_entity(it).is_ok(), "the cod stays on the ground");
}
