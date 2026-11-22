//! `Entity.isInvulnerableToBase`: the permanent `Invulnerable` flag and the `invulnerableTime` window refuse every hit but
//! `#bypasses_invulnerability` (`out_of_world`).
use crate::common;
use common::*;
use horizon_core::nbt::{Compound, JavaString, Tag};
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::Entered;
use horizon_ecs_proto::nbt::NbtData;

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:cow"])
}
fn species_named(name: &str) -> Species {
    *species_table().0.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name}"))
}

fn health_after(h: &mut Herd, e: bevy_ecs::entity::Entity, cause: Cause) -> f32 {
    h.world.resource_mut::<DamageQueue>().add(e, 1.0, cause);
    h.tick();
    h.world.get::<Health>(e).unwrap().0
}

fn cow_in(h: &mut Herd) -> bevy_ecs::entity::Entity {
    let cow = spawn_species(&mut h.world, &species_named("minecraft:cow"), 0.5, 0.5, 5);
    h.world.resource_mut::<Entered>().0.push(cow);
    h.tick();
    cow
}

#[test]
fn the_flag_refuses_all_but_the_void() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let cow = cow_in(&mut h);
    let full = h.world.get::<Health>(cow).unwrap().0;
    h.world.entity_mut(cow).insert(NbtData(Box::new(Compound::from([(JavaString::from("Invulnerable"), Tag::Byte(1))]))));
    assert_eq!(health_after(&mut h, cow, Cause::Lava), full);
    assert!(health_after(&mut h, cow, Cause::OutOfWorld) < full);
}

#[test]
fn the_window_refuses_while_it_runs() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let cow = cow_in(&mut h);
    let full = h.world.get::<Health>(cow).unwrap().0;
    let until = now_of(&h.world, cow) + 5;
    h.world.entity_mut(cow).insert(InvulnerableUntil(until));
    assert_eq!(health_after(&mut h, cow, Cause::Cactus), full);
}
