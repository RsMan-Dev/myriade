//! Règle du joueur aimé de l'allay, reprise de `allay_liked_player_tests.rs::a_given_item_makes_the_liked_player_permanent_again` :
//! donner un objet fixe le donneur comme joueur aimé, le reprendre l'efface (`Allay.mobInteract`, `Brain.setMemory`).
//! Partiel : la partie « jamais expirer » suppose une durée de vie que l'ECS ne porte pas (hand_over.rs:20, IMPOSSIBLE au journal).
use crate::common;
use bevy_ecs::entity::Entity;
use common::*;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::items;
use horizon_ecs_proto::tame::Owner;
use horizon_ecs_proto::targeting::Who;

fn hold(h: &mut Herd, main: u32) {
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main, off: 0, ..Default::default() });
}
fn click(h: &mut Herd, e: Entity, item: u32) -> bool {
    interact(&mut h.world, e, &horizon_ecs_proto::behaviours::Interaction { player: 1, item })
}

#[test]
fn a_given_item_makes_the_liked_player_permanent_again() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(sp) = species_table().0.into_iter().find(|s| s.name == "minecraft:allay") else { return };
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    h.world.resource_mut::<Entered>().0.push(e);
    let cod = items::item_id("minecraft:cod");
    hold(&mut h, cod);
    assert!(click(&mut h, e, cod));
    assert_eq!(h.world.get::<Owner>(e).unwrap().0, Some(Who::Player(1)), "given: the giver is the liked player");
    assert!(click(&mut h, e, 0), "an empty hand takes the item back");
    assert_eq!(h.world.get::<Owner>(e).unwrap().0, None, "taken back: the liking is erased");
}
