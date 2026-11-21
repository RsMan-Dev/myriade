//! The teleport service against the `Mob` enderman (`mobs::enderman::teleport_random`, `HORIZON_TEST_RESOURCES` set): the same scene (a flat
//! floor; half the spots draw a height under the world), 600 random teleports on each side: the share that succeed and where they land
//! (floor height, distance). Then the dodge of a projectile. Not tested against the `Mob`: the landing near liquids and tagged blocks.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{self, MobEvent};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    behaviours::{ender_teleport, Entry},
    cow::*,
    projectile::{launch, Flight},
    targeting::Who,
};

fn on() -> bool {
    have_data()
}
fn stats(v: &[(f64, f64, f64)], tries: usize) -> (f64, f64, f64, f64) {
    let n = v.len() as f64;
    let mx = v.iter().map(|p| p.0.abs()).sum::<f64>() / n;
    let mz = v.iter().map(|p| p.2.abs()).sum::<f64>() / n;
    let my = v.iter().map(|p| p.1).sum::<f64>() / n;
    (n / tries as f64, mx, mz, my)
}
fn mob_landings(tries: usize) -> Vec<(f64, f64, f64)> {
    let mut r = MobRef::new(40, false);
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:enderman"));
    let mut v = Vec::new();
    for _ in 0..tries {
        r.mobs[i].position = Vec3::new(0.5, 1.0, 0.5);
        let mut events: Vec<MobEvent> = Vec::new();
        if mobs::enderman::teleport_random(&mut r.mobs[i], r.w, &mut events) {
            let p = r.mobs[i].position;
            v.push((p.x - 0.5, p.y, p.z - 0.5));
        }
    }
    v
}
fn ecs_landings(tries: usize) -> Vec<(f64, f64, f64)> {
    let mut h = Herd::with_slab_awake(40, false, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let mut sp = *table.iter().find(|s| s.name == "minecraft:enderman").unwrap();
    sp.goals = horizon_ecs_proto::species::Config { entries: Box::leak(Box::new([Entry::new(&ender_teleport::MODULE, 255, &ender_teleport::Params::ENDERMAN)])) };
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 4242);
    h.world.resource_mut::<horizon_ecs_proto::goals::Entered>().0.push(e);
    h.tick();
    let mut v = Vec::new();
    for _ in 0..tries {
        h.teleport(e, Vec3::new(0.5, 1.0, 0.5));
        if ender_teleport::random(&mut h.world, e) {
            let p = h.world.get::<Pos>(e).unwrap().0;
            v.push((p.x - 0.5, p.y, p.z - 0.5));
        }
    }
    v
}

#[test]
fn random_teleports_land_where_the_mob_s_do() {
    if !on() {
        return;
    }
    let (m, e) = (mob_landings(600), ecs_landings(600));
    let (ms, es) = (stats(&m, 600), stats(&e, 600));
    eprintln!("(success, mean |dx|, mean |dz|, mean y): mob {ms:?} ecs {es:?}");
    assert!((ms.0 - es.0).abs() < 0.08, "the share that succeed");
    assert!((ms.1 / es.1 - 1.0).abs() < 0.15 && (ms.2 / es.2 - 1.0).abs() < 0.15, "the spread");
    assert!((ms.3 - es.3).abs() < 0.2, "the floor height");
}

/// A projectile that meets the enderman does it no harm and it leaves.
#[test]
fn an_arrow_is_dodged() {
    if !on() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, false, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let mut sp = *table.iter().find(|s| s.name == "minecraft:enderman").unwrap();
    sp.goals = horizon_ecs_proto::species::Config { entries: Box::leak(Box::new([Entry::new(&ender_teleport::MODULE, 255, &ender_teleport::Params::ENDERMAN)])) };
    let e = spawn_species(&mut h.world, &sp, 10.5, 0.5, 4242);
    h.world.resource_mut::<horizon_ecs_proto::goals::Entered>().0.push(e);
    for _ in 0..3 {
        h.tick();
    }
    let hp = health(&h, e);
    let at = h.world.get::<Pos>(e).unwrap().0;
    launch(&mut h.world, Flight::ARROW, Vec3::new(2.5, at.y + 1.5, 0.5), Vec3::new(1.5, 0.0, 0.0), Some(Who::Entity(e)));
    let mut moved = false;
    for _ in 0..12 {
        h.tick();
        let p = h.world.get::<Pos>(e).unwrap().0;
        moved |= (p.x - at.x).abs() > 1.0 || (p.z - at.z).abs() > 1.0;
    }
    eprintln!("health {hp} -> {}, moved: {moved}", health(&h, e));
    assert_eq!(health(&h, e), hp, "no harm");
    assert!(moved, "it left");
}
