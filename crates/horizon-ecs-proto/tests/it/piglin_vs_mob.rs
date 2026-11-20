//! The piglin's admiration of gold, its flight from a soul campfire and its crossbow against the real `Mob` piglin in the same
//! scene (`HORIZON_TEST_RESOURCES` set, `HORIZON_SPECIES=minecraft:piglin`). The ECS side needs the equipment lot's
//! `equipment::set_slot` and the `Slot::Off` of goals.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{Mob, MobEvent, PlayerView};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, equipment, goals::*, projectile::Projectile};
use std::sync::Arc;

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:piglin"])
}
fn ecs_piglin(h: &mut Herd, x: f64, z: f64, seed: u64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:piglin").unwrap();
    let e = spawn_species(&mut h.world, &sp, x, z, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    e
}
fn mob_piglin(r: &mut MobRef, x: f64, z: f64, id: u32) -> usize {
    r.add_id(Vec3::new(x, 1.0, z), id)
}

/// A piglin with gold in its off hand admires it for as long as the Mob does, then the gold leaves the hand on the same tick.
#[test]
fn a_piglin_admires_gold_for_as_long_as_a_mob_does() {
    if skip() {
        return;
    }
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab(60, true, false);
    let i = mob_piglin(&mut r, 0.5, 0.5, 7);
    r.mobs[i].extra.set_slot_id(horizon_items::equipment::OFFHAND, Some(Arc::from("minecraft:gold_ingot")));
    // The game starts the admiring when the piglin picks the gold up (`using_item` = 119, counted down by the Mob) and takes the
    // off-hand item when the Mob raises `FinishAdmiring`: this scene plays the game's part.
    r.mobs[i].extra.using_item = horizon_client_lib::mobs::piglin::ADMIRE;
    let e = ecs_piglin(&mut h, 0.5, 0.5, 7);
    equipment::set_slot(&mut h.world, e, Slot::Off, horizon_ecs_proto::items::item_id("minecraft:gold_ingot"));
    let (mut mob_end, mut ecs_end) = (None, None);
    for t in 1..=300i64 {
        let from = r.events.len();
        r.step(None);
        h.tick();
        if mob_end.is_none() && r.events[from..].iter().any(|ev| matches!(ev, MobEvent::FinishAdmiring { .. })) {
            r.mobs[i].extra.take_slot_id(horizon_items::equipment::OFFHAND);
            mob_end = Some(t);
        }
        if ecs_end.is_none() && h.world.get::<Equipment>(e).is_none_or(|q| q.off == 0) {
            ecs_end = Some(t);
        }
    }
    let (m, c) = (mob_end.expect("the Mob trades the gold"), ecs_end.expect("the ECS trades the gold"));
    // The item put in the hand is seen by the goal one tick later (events are read the tick after), and the goal starts on the
    // next one: the recorded registry latency of 2 ticks.
    assert!((m - c).abs() <= 2, "the trade falls on the same tick: Mob {m}, ECS {c}");
    assert!((m - 119).abs() <= 2, "the admiration lasts 119 ticks: {m}");
}

/// Piglins two blocks from a lit soul campfire walk away from it: their mean distance after 60 ticks matches the Mob's.
#[test]
fn piglins_flee_a_soul_campfire_as_a_mob_does() {
    if skip() {
        return;
    }
    let n = 20;
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab(60, true, false);
    r.set_block(0, 1, 0, "minecraft:soul_campfire");
    h.set_block(0, 1, 0, "minecraft:soul_campfire");
    let ids: Vec<(usize, Entity)> = (0..n)
        .map(|k| {
            let (x, z) = (-6.0 + 0.5 * (k % 5) as f64, 2.5 + 0.5 * (k / 5) as f64);
            (mob_piglin(&mut r, x, z, k as u32 + 1), ecs_piglin(&mut h, x, z, k as u64 + 1))
        })
        .collect();
    for _ in 0..60 {
        r.step(None);
        h.tick();
    }
    let mob_d = mean(&ids.iter().map(|&(i, _)| xz(r.mobs[i].position, Vec3::new(0.5, 1.0, 0.5))).collect::<Vec<_>>());
    let ecs_d = mean(&ids.iter().map(|&(_, e)| xz(h.world.get::<Pos>(e).unwrap().0, Vec3::new(0.5, 1.0, 0.5))).collect::<Vec<_>>());
    assert!(mob_d > 6.0 && ecs_d > 6.0, "both walk away: Mob {mob_d:.1}, ECS {ecs_d:.1}");
    assert!((mob_d - ecs_d).abs() <= 3.0, "the mean distances match: Mob {mob_d:.1}, ECS {ecs_d:.1}");
}

/// A piglin with a crossbow and a player six blocks away shoots: the first arrow flies on the same tick window on both sides
/// (the charge of 25 ticks, then the wait of the goal).
#[test]
fn a_piglin_shoots_its_crossbow_as_a_mob_does() {
    if skip() {
        return;
    }
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab(60, true, false);
    let i = mob_piglin(&mut r, 0.5, 0.5, 9);
    r.mobs[i].extra.set_slot_id(horizon_items::equipment::MAINHAND, Some(Arc::from("minecraft:crossbow")));
    let e = ecs_piglin(&mut h, 0.5, 0.5, 9);
    equipment::set_slot(&mut h.world, e, Slot::Main, horizon_ecs_proto::items::item_id("minecraft:crossbow"));
    h.set_players(vec![(6.5, 0.5)]);
    h.set_online(&[(6.5, 0.5)]);
    let pv: PlayerView = player_view(Vec3::new(6.5, 1.0, 0.5), None);
    let (mut mob_first, mut ecs_first) = (None, None);
    for t in 1..=200i64 {
        r.step(Some(&pv));
        h.tick();
        let shot = r.events.iter().any(|ev| matches!(ev, MobEvent::CrossbowShot { .. }));
        if mob_first.is_none() && shot {
            mob_first = Some(t);
        }
        if ecs_first.is_none() && h.world.query::<&Projectile>().iter(&h.world).next().is_some() {
            ecs_first = Some(t);
        }
    }
    let (m, c) = (mob_first.expect("the Mob shoots"), ecs_first.expect("the ECS shoots"));
    assert!((m - c).abs() <= 10, "the first arrow flies at about the same tick: Mob {m}, ECS {c}");
}
