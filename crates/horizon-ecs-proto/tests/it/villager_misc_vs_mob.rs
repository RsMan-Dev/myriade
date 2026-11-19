//! Le `Mob` et l'ecs dans la même scène plate (`HORIZON_SPECIES` nomme l'espèce) : récolte et replantation du blé mûr,
//! panique après un coup, ouverture d'une porte. Chaque cas pose le même état dans les deux mondes. Ne passe qu'une fois
//! le câblage des espèces fait (voir `villager-divers.md`, section 5) : pas d'`#[ignore]`.
use crate::common::*;
use horizon_ecs_proto::brain::{present, IS_PANICKING};
use horizon_ecs_proto::{blocks, cow};
use horizon_core::geometry::Vec3;
use horizon_core::position::BlockPos;

fn is(name: &str) -> bool {
    species().name == name
}

/// Writes a block state with properties into the `Mob` world (`MobRef::set_block` takes a default state only).
fn mob_block(r: &MobRef, x: i32, y: i32, z: i32, name: &str, props: &[(&str, &str)]) {
    let Some(id) = blocks::resolve(r.w, name, props) else { return };
    // SAFETY: as in `MobRef::set_block`: the test thread owns the world while it steps.
    unsafe {
        let world = &mut *r.level.blocks.0;
        let _ = world.set(x, y, z, id);
        world.path_type_block_updated(BlockPos::new(x, y, z));
    }
}

/// The name of the block at `(x, y, z)` in both worlds, `(mob, ecs)`.
fn names_at(r: &MobRef, h: &Herd, x: i32, y: i32, z: i32) -> (String, String) {
    let mob = r.w.block_name(x, y, z).to_string();
    let ecs = h.world.resource::<Level>().mw.0.block_name(x, y, z).to_string();
    (mob, ecs)
}

/// Spawns a villager of the process species at `(x, z)` in the ecs world.
fn spawn_villager(h: &mut Herd, x: f64, z: f64) -> Entity {
    cow::spawn_armed(&mut h.world, species(), x, z, 7)
}

#[test]
fn a_farmer_harvests_and_replants_the_mature_wheat_as_a_mob_does() {
    if !have_data() || !is("minecraft:villager") {
        return;
    }
    let (mut r, mut h) = flat_pair();
    let (x, y, z) = (0, 64, 0);
    let crop = [x + 1, y, z];
    mob_block(&r, crop[0], crop[1], crop[2], "minecraft:wheat", &[("age", "7")]);
    h.set_block_state(crop[0], crop[1], crop[2], "minecraft:wheat", &[("age", "7")]);
    r.add(Vec3::new(f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5));
    spawn_villager(&mut h, f64::from(x) + 0.5, f64::from(z) + 0.5);

    let (mut mob_at, mut ecs_at) = (None, None);
    for t in 0..200u64 {
        r.step(None);
        r.apply_block_events();
        h.tick();
        let (m, c) = names_at(&r, &h, crop[0], crop[1], crop[2]);
        let mob_age = r.w.prop(crop[0], crop[1], crop[2], "age");
        let ecs_age = h.world.resource::<Level>().mw.0.prop(crop[0], crop[1], crop[2], "age");
        if mob_at.is_none() && t > 0 && m == "minecraft:wheat" && mob_age == Some("0") {
            mob_at = Some(t);
        }
        if ecs_at.is_none() && t > 0 && c == "minecraft:wheat" && ecs_age == Some("0") {
            ecs_at = Some(t);
        }
        if mob_at.is_some() && ecs_at.is_some() {
            break;
        }
    }
    assert!(mob_at.is_some(), "the Mob farmer replants the crop within 200 ticks");
    assert_eq!(mob_at, ecs_at, "the ecs farmer replants at the same tick as the Mob");
}

#[test]
fn a_villager_flees_the_way_away_from_a_hit_as_a_mob_does() {
    if !have_data() || !is("minecraft:villager") {
        return;
    }
    let (mut r, mut h) = flat_pair();
    let i = r.add(Vec3::new(0.5, 64.0, 0.5));
    let e = spawn_villager(&mut h, 0.5, 0.5);
    r.step(None);
    h.tick();
    r.hit(i, 4.0, [-3.0, 64.0, 0.5]);
    h.hit_by(e, 4.0, 1);
    let (mut mob_x, mut ecs_x) = (0.5, 0.5);
    let (mut mob_end, mut ecs_end) = (None, None);
    for t in 0..400u64 {
        r.step(None);
        h.tick();
        mob_x = r.mobs[i].position().x;
        ecs_x = h.world.get::<cow::Pos>(e).map_or(ecs_x, |p| p.0.x);
        let mob_panic = r.mobs[i].is_panicking();
        let ecs_panic = present(&h.world, e, &IS_PANICKING);
        if mob_end.is_none() && t > 0 && !mob_panic {
            mob_end = Some(t);
        }
        if ecs_end.is_none() && t > 0 && !ecs_panic {
            ecs_end = Some(t);
        }
    }
    assert!(mob_x > 0.5 && ecs_x > 0.5, "both villagers flee east, away from the hit at x=-3 (mob {mob_x}, ecs {ecs_x})");
    assert_eq!(mob_end, ecs_end, "the panic ends at the same tick in both worlds");
}

#[test]
fn a_villager_opens_the_door_in_its_way_as_a_mob_does() {
    if !have_data() || !is("minecraft:villager") {
        return;
    }
    let (mut r, mut h) = flat_pair();
    let door = [2, 64, 0];
    mob_block(&r, door[0], door[1], door[2], "minecraft:oak_door", &[("open", "false"), ("half", "lower")]);
    h.set_block_state(door[0], door[1], door[2], "minecraft:oak_door", &[("open", "false"), ("half", "lower")]);
    r.add(Vec3::new(0.5, 64.0, 0.5));
    spawn_villager(&mut h, 0.5, 0.5);
    let (mut mob_open, mut ecs_open) = (None, None);
    for t in 0..200u64 {
        r.step(None);
        r.apply_block_events();
        h.tick();
        if mob_open.is_none() && r.w.prop(door[0], door[1], door[2], "open") == Some("true") {
            mob_open = Some(t);
        }
        if ecs_open.is_none() && h.world.resource::<Level>().mw.0.prop(door[0], door[1], door[2], "open") == Some("true") {
            ecs_open = Some(t);
        }
    }
    assert!(mob_open.is_some(), "the Mob villager opens the door within 200 ticks");
    assert_eq!(mob_open, ecs_open, "the ecs villager opens the door at the same tick as the Mob");
}
