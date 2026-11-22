//! Shearing follow-ups: the stew a mooshroom keeps is saved (`stew_effects`), creative keeps the hand's item (`ItemUtils.createFilledResult`), the
//! copper golem shears through the generic rule (also with no player), and a sulfur cube's block comes out at the top of its body.
use crate::common;
use bevy_ecs::entity::Entity;
use common::*;
use horizon_core::nbt::{JavaString, Tag};
use horizon_ecs_proto::behaviours::shear::shear_entity;
use horizon_ecs_proto::behaviours::stew_flower::Stew;
use horizon_ecs_proto::behaviours::Interaction;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::items;
use horizon_ecs_proto::nbt;
use horizon_ecs_proto::used_stack::UsedStack;

fn spawn(name: &str, seed: u64) -> Option<(Herd, Entity)> {
    if !have_data() {
        return None;
    }
    let sp = species_table().0.into_iter().find(|s| s.name == name)?;
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, seed);
    h.world.resource_mut::<Entered>().0.push(e);
    Some((h, e))
}

fn hold(h: &mut Herd, name: &str) -> u32 {
    let item = items::item_id(name);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: item, off: 0, ..Default::default() });
    item
}

#[test]
fn the_kept_stew_effect_is_saved_and_restored() {
    let Some((mut h, e)) = spawn("minecraft:mooshroom", 31) else { return };
    h.world.entity_mut(e).insert(Stew("minecraft:night_vision", 100));
    let saved = nbt::save(&h.world, e).expect("an NBT");
    let Some(Tag::List(list)) = saved.get(&JavaString::from("stew_effects")) else { panic!("stew_effects is saved") };
    let Some(Tag::Compound(entry)) = list.first() else { panic!("one entry") };
    assert_eq!(entry.get(&JavaString::from("duration")), Some(&Tag::Int(100)));
    let Some((mut g, f)) = spawn("minecraft:mooshroom", 32) else { return };
    assert!(g.world.get::<Stew>(f).is_none());
    nbt::load(&mut g.world, f, &saved);
    assert_eq!(g.world.get::<Stew>(f).copied(), Some(Stew("minecraft:night_vision", 100)));
}

#[test]
fn creative_keeps_the_bucket_in_hand() {
    let Some((mut h, e)) = spawn("minecraft:cow", 33) else { return };
    let bucket = hold(&mut h, "minecraft:bucket");
    let stack = horizon_client_lib::items::ItemStack::new("minecraft:bucket", 1);
    h.world.insert_resource(UsedStack { stack, creative: true });
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: bucket }));
    assert_eq!(h.world.resource::<Online>().get(1).unwrap().main, bucket, "the bucket stays in creative");
    let out: Vec<HostEvent> = h.world.resource_mut::<Events>().host.drain().collect();
    assert!(out.iter().any(|ev| matches!(ev, HostEvent::HeldSwap { player: 1, .. })), "the host is still told, it adds the milk if missing");
}

#[test]
fn a_golem_is_sheared_by_the_rule_with_no_player() {
    let Some((mut h, e)) = spawn("minecraft:copper_golem", 34) else { return };
    assert!(!shear_entity(&mut h.world, e, None), "no flower: not ready");
    horizon_ecs_proto::equipment::set_slot(&mut h.world, e, Slot::Saddle, items::item_id("minecraft:poppy"));
    assert!(shear_entity(&mut h.world, e, None));
    assert_eq!(equipped(&h.world, e, Slot::Saddle), 0, "the antenna is empty");
    let out: Vec<HostEvent> = h.world.resource_mut::<Events>().host.drain().collect();
    assert!(out.iter().any(|ev| matches!(ev, HostEvent::Sound { name, .. } if *name == "minecraft:entity.copper_golem.shear")));
}

#[test]
fn a_sulfur_cube_drops_its_block_at_the_top_of_its_body() {
    let Some((mut h, e)) = spawn("minecraft:sulfur_cube", 35) else { return };
    let arch = horizon_client_lib::mobs::sulfur_cube::archetype_of("minecraft:magma_block").expect("an archetype");
    horizon_ecs_proto::behaviours::swallow_hold::hold(&mut h.world, e, "minecraft:magma_block".into(), arch);
    let shears = hold(&mut h, "minecraft:shears");
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: shears }));
    assert!(h.world.get::<horizon_ecs_proto::behaviours::swallow_hold::Held>(e).is_none(), "the block is released");
    assert!(h.world.get::<horizon_ecs_proto::behaviours::swallow_hold::SearchAfter>(e).is_some());
    let feet = h.world.get::<Pos>(e).unwrap().0.y;
    let top = f64::from(h.world.get::<Body>(e).unwrap().height);
    let mut q = h.world.query::<(&horizon_ecs_proto::item::Item, &Pos)>();
    let ys: Vec<f64> = q.iter(&h.world).map(|(_, p)| p.0.y).collect();
    assert_eq!(ys.len(), 1, "one item dropped");
    assert!((ys[0] - (feet + top)).abs() < 1e-3, "dropped at {}, expected {}", ys[0], feet + top);
}
