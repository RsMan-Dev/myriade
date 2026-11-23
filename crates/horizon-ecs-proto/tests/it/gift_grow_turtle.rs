//! `Turtle.ageBoundaryReached`: a baby turtle that grows up drops one turtle scute (the table `gameplay/turtle_grow`) and only with the `mob_drops` rule on;
//! a creature with no growth gift drops nothing when it grows.
use crate::common::*;
use horizon_ecs_proto::{cow::*, farm::Grown, item::Item};

fn grow_and_collect(h: &mut Herd, e: bevy_ecs::entity::Entity) -> Vec<(String, u32)> {
    h.world.resource_mut::<Grown>().0.push(e);
    h.tick();
    h.world.query::<&Item>().iter(&h.world).map(|i| (i.stack.item.to_string(), i.stack.count)).collect()
}

fn named(h: &Herd, name: &str) -> Species {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    *table.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name}"))
}

#[test]
fn a_turtle_that_grows_up_drops_one_scute() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let sp = named(&h, "minecraft:turtle");
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 61);
    h.tick();
    assert!(h.world.query::<&Item>().iter(&h.world).next().is_none(), "nothing falls before the growth");
    let items = grow_and_collect(&mut h, e);
    assert_eq!(items, vec![("minecraft:turtle_scute".to_string(), 1)]);
}

#[test]
fn with_mob_drops_off_the_growth_drops_nothing() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let mut level = *h.world.resource::<Level>();
    let off: &'static myriade_engine_lib::mobs::MobWorld<'static> = Box::leak(Box::new(myriade_engine_lib::mobs::MobWorld { mob_drops: false, ..level.mw.0.clone() }));
    level.mw = Mw(off);
    *h.world.resource_mut::<Level>() = level;
    let sp = named(&h, "minecraft:turtle");
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 61);
    h.tick();
    assert!(grow_and_collect(&mut h, e).is_empty(), "the rule is off");
}

#[test]
fn a_cow_that_grows_up_drops_nothing() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let sp = named(&h, "minecraft:cow");
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 61);
    h.tick();
    assert!(grow_and_collect(&mut h, e).is_empty());
}
