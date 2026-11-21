//! Items used on an entity: flint and steel lights a creeper's fuse and wears, a dye colours a tamed wolf's collar for its owner only, a spawn
//! egg of the animal's own type makes a baby, wolf armor goes on the owner's wolf. Written, not run.
use crate::common;
use common::*;
use horizon_ecs_proto::behaviours::Interaction;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::dye::{Collar, DEFAULT_COLLAR};
use horizon_ecs_proto::explosion::Fuse;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::items;
use horizon_ecs_proto::tame::Owner;
use horizon_ecs_proto::targeting::Who;

fn species_named(name: &str) -> Option<Species> {
    species_table().0.into_iter().find(|s| s.name == name)
}
fn hold(h: &mut Herd, item: u32) {
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: item, off: 0, ..Default::default() });
}
fn host_events(h: &mut Herd) -> Vec<HostEvent> {
    h.world.resource_mut::<Events>().host.drain().collect()
}

#[test]
fn flint_and_steel_lights_the_fuse_and_wears() {
    if !have_data() {
        return;
    }
    let Some(sp) = species_named("minecraft:creeper") else { return };
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 3);
    h.world.resource_mut::<Entered>().0.push(e);
    let steel = items::item_id("minecraft:flint_and_steel");
    hold(&mut h, steel);
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: steel }));
    assert!(h.world.get::<Fuse>(e).unwrap().ignited);
    let ev = host_events(&mut h);
    assert!(ev.iter().any(|v| matches!(v, HostEvent::Sound { name, .. } if name.contains("flintandsteel"))));
    assert!(ev.iter().any(|v| matches!(v, HostEvent::HeldWorn { amount: 1, .. })), "a tool wears, it is not used up");
}

#[test]
fn a_dye_colours_the_collar_for_the_owner_only() {
    if !have_data() {
        return;
    }
    let Some(sp) = species_named("minecraft:wolf") else { return };
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 4);
    h.world.resource_mut::<Entered>().0.push(e);
    let blue = items::item_id("minecraft:blue_dye");
    hold(&mut h, blue);
    h.world.entity_mut(e).insert(Owner(Some(Who::Player(2))));
    interact(&mut h.world, e, &Interaction { player: 1, item: blue });
    assert!(h.world.get::<Collar>(e).is_none(), "another player's wolf keeps its collar");
    h.world.entity_mut(e).insert(Owner(Some(Who::Player(1))));
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: blue }));
    assert_eq!(h.world.get::<Collar>(e).unwrap().0, 11);
    assert_ne!(DEFAULT_COLLAR, 11);
    assert_eq!(h.world.resource::<Online>().get(1).unwrap().main, 0, "one dye was used");
}

#[test]
fn a_spawn_egg_of_its_own_type_makes_a_baby() {
    if !have_data() {
        return;
    }
    let Some(sp) = species_named("minecraft:cow") else { return };
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 5);
    h.world.resource_mut::<Entered>().0.push(e);
    let egg = items::item_id("minecraft:cow_spawn_egg");
    let other = items::item_id("minecraft:pig_spawn_egg");
    hold(&mut h, other);
    assert!(!interact(&mut h.world, e, &Interaction { player: 1, item: other }), "an egg of another type does nothing");
    hold(&mut h, egg);
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: egg }));
    let babies = h.world.query::<&GrowAt>().iter(&h.world).count();
    assert_eq!(babies, 1);
    assert_eq!(h.world.resource::<Online>().get(1).unwrap().main, 0, "the egg was used up");
}

#[test]
fn wolf_armor_goes_on_the_owners_wolf() {
    if !have_data() {
        return;
    }
    let Some(sp) = species_named("minecraft:wolf") else { return };
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 6);
    h.world.resource_mut::<Entered>().0.push(e);
    let armor = items::item_id("minecraft:wolf_armor");
    hold(&mut h, armor);
    h.world.entity_mut(e).insert(Owner(Some(Who::Player(2))));
    assert!(!interact(&mut h.world, e, &Interaction { player: 1, item: armor }), "not the owner");
    h.world.entity_mut(e).insert(Owner(Some(Who::Player(1))));
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: armor }));
    assert_eq!(equipped(&h.world, e, Slot::Body), armor);
    assert_eq!(h.world.resource::<Online>().get(1).unwrap().main, 0);
}
