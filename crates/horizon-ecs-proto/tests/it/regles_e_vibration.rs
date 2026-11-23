//! `Warden$VibrationUser.canReceiveVibration`: a vibration from a living source the warden cannot target (an armor stand, another warden,
//! an invulnerable creature) is not heard; a hostile zombie's vibration is, and puts the disturbance where it came from.
use crate::common;
use common::*;
use horizon_ecs_proto::behaviours::act_phase::RareState;
use horizon_ecs_proto::behaviours::vibration_listen::Vibration;
use horizon_ecs_proto::cow::{spawn_species, ActiveUntil, Herd, Pos, SpeciesTable};
use horizon_ecs_proto::goals::Events;
use horizon_ecs_proto::targeting::Who;
use bevy_ecs::prelude::*;

fn spawn(h: &mut Herd, name: &str, x: f64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    let e = spawn_species(&mut h.world, &sp, x, 0.5, 5);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    e
}

fn vibrate_from(h: &mut Herd, source: Entity) {
    let p = h.world.get::<Pos>(source).unwrap().0;
    let v = Vibration { pos: [p.x, p.y, p.z], source: Some(Who::Entity(source)), owner: None, projectile: false };
    h.world.resource_mut::<Events>().vibration.push(v);
}

#[test]
fn a_vibration_from_an_untargetable_living_source_is_not_heard() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let warden = spawn(&mut h, "minecraft:warden", 0.5);
    let stand = spawn(&mut h, "minecraft:armor_stand", 2.5);
    vibrate_from(&mut h, stand);
    h.tick();
    let rare = h.world.get::<RareState>(warden).copied().unwrap_or_default();
    assert!(rare.disturbance.is_none(), "an armor stand's vibration is not heard");
}

#[test]
fn a_hostile_creature_vibration_is_heard() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let warden = spawn(&mut h, "minecraft:warden", 0.5);
    let zombie = spawn(&mut h, "minecraft:zombie", 2.5);
    vibrate_from(&mut h, zombie);
    h.tick();
    let rare = h.world.get::<RareState>(warden).copied().unwrap_or_default();
    assert!(rare.disturbance.is_some(), "a zombie's vibration is heard");
}
