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
    !have_data() || std::env::var("HORIZON_SPECIES").as_deref() != Ok("minecraft:piglin")
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

/// Piglins two blocks from a lit soul campfire walk away from it: over 20 seeds the ECS mean distance after 60 ticks lies within two
/// standard deviations of the Mob's (a single seed compares two noisy draws, the ECS one is 0.9 wide, the Mob's 0.5).
#[test]
fn piglins_flee_a_soul_campfire_as_a_mob_does() {
    if skip() {
        return;
    }
    let (mut md, mut ed) = (Vec::new(), Vec::new());
    for s in 0..20u32 {
        let n = 20;
        let mut r = MobRef::new(60, false);
        let mut h = Herd::with_slab(60, true, false);
        r.set_block(0, 1, 0, "minecraft:soul_campfire");
        h.set_block(0, 1, 0, "minecraft:soul_campfire");
        let ids: Vec<(usize, Entity)> = (0..n)
            .map(|k| {
                let (x, z) = (-6.0 + 0.5 * (k % 5) as f64, 2.5 + 0.5 * (k / 5) as f64);
                let id = s * 1000 + k as u32 + 1;
                (mob_piglin(&mut r, x, z, id), ecs_piglin(&mut h, x, z, u64::from(id)))
            })
            .collect();
        for _ in 0..60 {
            r.step(None);
            h.tick();
        }
        let c = Vec3::new(0.5, 1.0, 0.5);
        md.push(mean(&ids.iter().map(|&(i, _)| xz(r.mobs[i].position, c)).collect::<Vec<_>>()));
        ed.push(mean(&ids.iter().map(|&(_, e)| xz(h.world.get::<Pos>(e).unwrap().0, c)).collect::<Vec<_>>()));
    }
    let (m, e) = (mean(&md), mean(&ed));
    let sd = (md.iter().map(|x| (x - m).powi(2)).sum::<f64>() / md.len() as f64).sqrt();
    assert!(m > 6.0 && e > 6.0, "both walk away: Mob {m:.2}, ECS {e:.2}");
    assert!((m - e).abs() <= 2.0 * sd, "the means match within 2 sd of the Mob: Mob {m:.2} (sd {sd:.2}), ECS {e:.2}");
}

/// `SetWalkTargetAwayFrom` tries ten spots and takes the first found: a piglin two blocks from a soul campfire leaves it on
/// every seed (one try gave up on 5 seeds of 39). The ECS alone, 60 seeds, one piglin each.
#[test]
fn walk_away_takes_the_first_spot_of_ten_tries() {
    if skip() {
        return;
    }
    let c = Vec3::new(0.5, 1.0, 0.5);
    let stayed: Vec<u32> = (0..60u32)
        .filter(|&s| {
            let mut h = Herd::with_slab(60, true, false);
            h.set_block(0, 1, 0, "minecraft:soul_campfire");
            let e = ecs_piglin(&mut h, -1.5, 0.5, u64::from(s * 1001 + 1));
            for _ in 0..120 {
                h.tick();
            }
            xz(h.world.get::<Pos>(e).unwrap().0, c) < 8.0
        })
        .collect();
    assert!(stayed.is_empty(), "every piglin walks clear of the campfire; stayed within 8 blocks: seeds {stayed:?}");
}

/// The start condition of `SetWalkTargetAwayFrom.pos(.., 8, ..)`: a piglin whose sensor sees the campfire (box of 8 by 4) but
/// stands 10.6 blocks from it (a corner of the box) does not flee, in the Mob or in the ECS.
#[test]
fn walk_away_starts_only_within_eight_blocks_of_the_point() {
    if skip() {
        return;
    }
    let c = Vec3::new(0.5, 1.0, 0.5);
    let (mut md, mut ed) = (Vec::new(), Vec::new());
    for s in 0..20u32 {
        let mut r = MobRef::new(60, false);
        let mut h = Herd::with_slab(60, true, false);
        r.set_block(0, 1, 0, "minecraft:soul_campfire");
        h.set_block(0, 1, 0, "minecraft:soul_campfire");
        let id = s * 1001 + 1;
        let i = mob_piglin(&mut r, 7.5, 7.5, id);
        let e = ecs_piglin(&mut h, 7.5, 7.5, u64::from(id));
        for _ in 0..100 {
            r.step(None);
            h.tick();
        }
        md.push(xz(r.mobs[i].position, c) - 9.9);
        ed.push(xz(h.world.get::<Pos>(e).unwrap().0, c) - 9.9);
    }
    // A flight would take it 8 blocks further out; a stroll wanders a few.
    assert!(mean(&md) < 4.0 && mean(&ed) < 4.0, "neither flees: Mob {:.2}, ECS {:.2} blocks further out", mean(&md), mean(&ed));
}
