//! `SulfurCube.mobInteract` by hand: an adult takes a swallowable item as its block (the same item again is refused, another one throws the
//! first out); a baby takes no block. Scene as in sulfur_cube_vs_mob (`HORIZON_SPECIES`, vanilla data).
use crate::common;
use common::*;
use myriade_engine_lib::mobs::sulfur_cube::archetype_of;
use horizon_ecs_proto::behaviours::swallow_hold::Held;
use horizon_ecs_proto::behaviours::Interaction;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::{interact, Entered, Online, Player};
use horizon_ecs_proto::items;

#[test]
fn an_adult_cube_takes_a_swallowable_item_in_hand_and_a_baby_does_not() {
    if !have_data() || wrong_species(&["minecraft:sulfur_cube"]) {
        return;
    }
    let block = "minecraft:magma_block";
    if archetype_of(block).is_none() {
        return;
    }
    let id = items::item_id(block);
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 7);
    h.world.resource_mut::<Entered>().0.push(e);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: id, off: 0, ..Default::default() });
    let now = now_of(&h.world, e);
    h.world.entity_mut(e).insert(GrowAt(now + 24_000));
    assert!(!interact(&mut h.world, e, &Interaction { player: 1, item: id }), "a baby takes no block");
    h.world.entity_mut(e).remove::<GrowAt>();
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: id }));
    assert_eq!(&*h.world.get::<Held>(e).unwrap().item, block);
    assert!(!interact(&mut h.world, e, &Interaction { player: 1, item: id }), "the same item again is refused");
}
