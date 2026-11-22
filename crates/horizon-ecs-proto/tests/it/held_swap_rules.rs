//! `HELD_SWAP`, `STEW_FLOWER`, `STEW_BOWL` and the brush: what the host hears and what the hand holds afterwards.
use crate::common;
use bevy_ecs::entity::Entity;
use common::*;
use horizon_ecs_proto::behaviours::Interaction;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::items;

fn setup(name: &str, held: &str, seed: u64) -> Option<(Herd, Entity, u32)> {
    if !have_data() {
        return None;
    }
    let sp = species_table().0.into_iter().find(|s| s.name == name)?;
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, seed);
    h.world.resource_mut::<Entered>().0.push(e);
    let item = items::item_id(held);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: item, off: 0, ..Default::default() });
    Some((h, e, item))
}

fn events(h: &mut Herd) -> Vec<HostEvent> {
    h.world.resource_mut::<Events>().host.drain().collect()
}

fn has_sound(out: &[HostEvent], sound: &str) -> bool {
    out.iter().any(|ev| matches!(ev, HostEvent::Sound { name, .. } if *name == sound))
}

#[test]
fn a_bucket_on_a_cow_becomes_milk_with_its_sound() {
    let Some((mut h, e, bucket)) = setup("minecraft:cow", "minecraft:bucket", 21) else { return };
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: bucket }));
    let out = events(&mut h);
    assert!(out.iter().any(|ev| matches!(ev, HostEvent::HeldSwap { player: 1, to, .. } if to.item.as_ref() == "minecraft:milk_bucket")));
    assert!(has_sound(&out, "minecraft:entity.cow.milk"));
    assert_eq!(h.world.resource::<Online>().get(1).unwrap().main, items::item_id("minecraft:milk_bucket"));
}

#[test]
fn a_flower_then_a_bowl_gives_a_suspicious_stew_with_the_flowers_effect() {
    let Some((mut h, e, flower)) = setup("minecraft:mooshroom", "minecraft:dandelion", 22) else { return };
    h.world.entity_mut(e).insert(horizon_ecs_proto::behaviours::variant::Variant(1));
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: flower }), "the brown mooshroom eats the flower");
    assert_eq!(h.world.resource::<Online>().get(1).unwrap().main, 0);
    let bowl = items::item_id("minecraft:bowl");
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: bowl, off: 0, ..Default::default() });
    events(&mut h);
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: bowl }));
    let out = events(&mut h);
    let stew = out.iter().find_map(|ev| if let HostEvent::HeldSwap { to, .. } = ev { Some(to.clone()) } else { None }).expect("a swap");
    assert_eq!(stew.item.as_ref(), "minecraft:suspicious_stew");
    assert!(stew.data().is_some_and(|d| !d.stew_effects.is_empty()));
    assert!(has_sound(&out, "minecraft:entity.mooshroom.milk_suspiciously"));
}

#[test]
fn a_brush_on_an_armadillo_wears_by_16_and_plays_its_sound() {
    let Some((mut h, e, brush)) = setup("minecraft:armadillo", "minecraft:brush", 23) else { return };
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: brush }));
    let out = events(&mut h);
    assert!(has_sound(&out, "minecraft:entity.armadillo.brush"));
    assert!(out.iter().any(|ev| matches!(ev, HostEvent::HeldWorn { player: 1, .. })));
}
