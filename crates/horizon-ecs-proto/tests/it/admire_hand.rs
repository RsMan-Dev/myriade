//! `PiglinAi.mobInteract` / `canAdmire` by hand: a gold ingot is taken by an adult piglin (the item in its off hand is thrown out first); for 400 ticks after
//! a player hit it, it admires nothing (`wasHurtBy` sets `ADMIRING_DISABLED`).
use crate::common;
use common::*;
use horizon_ecs_proto::behaviours::Interaction;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::{equipped, Entered, Slot};
use horizon_ecs_proto::hostile::HurtBy;
use horizon_ecs_proto::items;
use horizon_ecs_proto::targeting::Who;

#[test]
fn a_piglin_hit_by_a_player_does_not_admire_for_four_hundred_ticks() {
    if !have_data() || wrong_species(&["minecraft:piglin"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 7);
    h.world.resource_mut::<Entered>().0.push(e);
    let gold = items::item_id("minecraft:gold_ingot");
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: gold, off: 0, ..Default::default() });
    h.world.resource_mut::<HurtBy>().0.push((e, Who::Player(1)));
    h.tick();
    assert!(!interact(&mut h.world, e, &Interaction { player: 1, item: gold }), "disabled right after the hit");
    for _ in 0..400 {
        h.tick();
    }
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: gold }), "admires again after 400 ticks");
    assert_eq!(equipped(&h.world, e, Slot::Off), gold);
}
