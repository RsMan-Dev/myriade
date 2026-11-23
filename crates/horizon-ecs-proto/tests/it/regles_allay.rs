//! Durée de vie de la mémoire du joueur aimé de l'allay, reprise de `allay_liked_player_tests.rs` : décomptée sous IA seulement
//! (`NoAi` fige), une mémoire sans fin n'expire jamais, donner un objet la rend permanente, la reprendre l'efface.
use crate::common;
use bevy_ecs::entity::Entity;
use common::*;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::dragon::flight::NoAi;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::hand_over::Liked;
use horizon_ecs_proto::items;
use horizon_ecs_proto::tame::Owner;
use horizon_ecs_proto::targeting::Who;

fn hold(h: &mut Herd, main: u32) {
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main, off: 0, ..Default::default() });
}
fn click(h: &mut Herd, e: Entity, item: u32) -> bool {
    interact(&mut h.world, e, &horizon_ecs_proto::behaviours::Interaction { player: 1, item })
}
fn allay(h: &mut Herd) -> Option<Entity> {
    let sp = species_table().0.into_iter().find(|s| s.name == "minecraft:allay")?;
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    h.world.resource_mut::<Entered>().0.push(e);
    Some(e)
}

#[test]
fn a_finite_liked_player_counts_down_under_ai_then_expires() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(e) = allay(&mut h) else { return };
    h.world.entity_mut(e).insert((Owner(Some(Who::Player(1))), Liked { ttl: Some(2) }));
    let mut seen = Vec::new();
    for _ in 0..3 {
        h.step(None);
        seen.push(h.world.get::<Liked>(e).map(|l| l.ttl));
    }
    assert_eq!(seen, vec![Some(Some(1)), Some(Some(0)), None], "2, 1, 0, then gone");
    assert_eq!(h.world.get::<Owner>(e).unwrap().0, None, "forgotten: no longer liked");
}

#[test]
fn a_finite_liked_player_is_frozen_without_ai() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(e) = allay(&mut h) else { return };
    h.world.entity_mut(e).insert((Owner(Some(Who::Player(1))), Liked { ttl: Some(2) }, NoAi));
    for _ in 0..4 {
        h.step(None);
    }
    assert_eq!(h.world.get::<Liked>(e).map(|l| l.ttl), Some(Some(2)), "NoAi keeps it as it was");
}

#[test]
fn a_liked_player_without_time_to_live_never_expires() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(e) = allay(&mut h) else { return };
    h.world.entity_mut(e).insert((Owner(Some(Who::Player(1))), Liked { ttl: None }));
    for _ in 0..4 {
        h.step(None);
    }
    assert_eq!(h.world.get::<Liked>(e).map(|l| l.ttl), Some(None), "permanent");
    assert_eq!(h.world.get::<Owner>(e).unwrap().0, Some(Who::Player(1)));
}

#[test]
fn a_given_item_makes_the_liked_player_permanent_again() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(e) = allay(&mut h) else { return };
    h.world.entity_mut(e).insert((Owner(Some(Who::Player(2))), Liked { ttl: Some(2) }));
    let cod = items::item_id("minecraft:cod");
    hold(&mut h, cod);
    assert!(click(&mut h, e, cod));
    assert_eq!(h.world.get::<Liked>(e).map(|l| l.ttl), Some(None), "given: permanent");
    assert!(click(&mut h, e, 0));
    assert!(h.world.get::<Liked>(e).is_none(), "taken back: the memory is erased");
}

#[test]
fn a_loaded_finite_liked_player_is_saved_with_its_time_to_live() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(e) = allay(&mut h) else { return };
    h.world.entity_mut(e).insert((Owner(Some(Who::Player(1))), Liked { ttl: Some(5) }));
    let saved = horizon_ecs_proto::nbt::save(&h.world, e).expect("an allay saves");
    let mem = saved.get("Brain").and_then(|b| b.get("memories")).and_then(|m| m.get("minecraft:liked_player")).expect("the liked player");
    assert_eq!(mem.get("ttl"), Some(&horizon_core::nbt::Tag::Long(5)), "the time to live is saved as it is");
    assert!(saved.get("Owner").is_none(), "the liked player is not saved under Owner");
}
