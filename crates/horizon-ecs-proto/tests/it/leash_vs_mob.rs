//! The lead and what hangs on it, against the `Mob` (`HORIZON_SPECIES=minecraft:llama`, `REGISTER_ALL=1`, `HORIZON_TEST_RESOURCES` set): the elastic
//! pull (the same function on 20 000 inputs), the rope that stretches and snaps as a player walks away (12 llamas a side), the caravan of three llamas
//! (8 caravans a side), the home of a held creature, the directory and the events, and the lamas of a trader that defend it.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::leash as mob_leash;
use horizon_client_lib::mobs::MobEvent;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::behaviours::{defend_leash_holder, follow_caravan, Interaction};
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::leash::{self, Holder, Leash};
use horizon_ecs_proto::targeting::{Target, Who};

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:llama"]) || std::env::var_os("REGISTER_ALL").is_none()
}
fn species_named(name: &str) -> Species {
    *species_table().0.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name}"))
}
fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len().max(1) as f64
}

/// `Leashable.computeElasticInteraction`: the ported function and the original's, on random positions, yaws, sizes, holders and motions.
#[test]
fn the_elastic_pull_is_the_original_function() {
    if skip() {
        return;
    }
    let mut s = 12_345_u64;
    let mut rnd = |lo: f64, hi: f64| {
        s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        lo + (hi - lo) * ((s >> 11) as f64 / (1u64 << 53) as f64)
    };
    let (mut some, mut none) = (0, 0);
    for _ in 0..20_000 {
        let (pos, yaw, size) = (Vec3::new(rnd(-3.0, 12.0), rnd(0.0, 3.0), rnd(-3.0, 12.0)), rnd(-720.0, 720.0) as f32, (rnd(0.3, 1.5), rnd(0.4, 2.5)));
        let motion = Vec3::new(rnd(-1.0, 1.0), rnd(-1.0, 1.0), rnd(-1.0, 1.0));
        let (h, w) = (rnd(0.5, 2.0), rnd(0.3, 1.0));
        let at = Vec3::new(rnd(-3.0, 3.0), rnd(0.0, 3.0), rnd(-3.0, 3.0));
        let hy = rnd(-720.0, 720.0) as f32;
        let m = mob_leash::HolderState { position: at, center: Vec3::new(at.x, at.y + h / 2.0, at.z), width: w, height: h, yaw: hy, velocity: Vec3::ZERO };
        let e = leash::HolderState { position: at, center: Vec3::new(at.x, at.y + h / 2.0, at.z), width: w, height: h, yaw: hy, velocity: Vec3::ZERO };
        let a = mob_leash::elastic_pull(pos, yaw, size, &m, motion, false);
        let b = leash::elastic_pull(pos, yaw, size, &e, motion, 6.0);
        match (a, b) {
            (None, None) => none += 1,
            (Some((fa, ta)), Some((fb, tb))) => {
                some += 1;
                assert!((0..3).all(|k| (fa[k] - fb[k]).abs() < 1e-12) && (ta - tb).abs() < 1e-12, "{fa:?} {fb:?} {ta} {tb}");
            }
            (a, b) => panic!("slack on one side only: mob {a:?} ecs {b:?}"),
        }
    }
    eprintln!("elastic pull on 20000 inputs: {some} taut, {none} slack, same on both sides");
    assert!(some > 3000 && none > 3000);
}

/// A player walks away at 0,45 block a tick for 100 ticks from 12 llamas on a lead each: the rope stretches, pulls, and snaps beyond 12 blocks. Each side's tick
/// of rupture, and the farthest a llama gets from the player while held.
#[test]
fn the_rope_snaps_as_the_player_walks_away() {
    if skip() {
        return;
    }
    let n = 12;
    let spot = |k: usize| Vec3::new(-6.0 + 3.0 * (k % 5) as f64 + 0.5, 1.0, -6.0 + 4.0 * (k / 5) as f64 + 0.5);
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab(60, true, false);
    let mut es = Vec::new();
    for k in 0..n {
        let i = r.add_id(spot(k), 500 + 31 * k as u32);
        r.mobs[i].extra.leash = Some(mob_leash::Holder::Player);
        let e = spawn_species(&mut h.world, species(), spot(k).x, spot(k).z, 9 + k as u64);
        h.world.resource_mut::<Entered>().0.push(e);
        es.push(e);
    }
    let start = Vec3::new(0.5, 1.0, 3.5);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: start.x, y: start.y, z: start.z, main: 0, off: 0, ..Default::default() });
    for &e in &es {
        assert!(leash::attach(&mut h.world, e, Holder::Player(1)), "the lead did not go on");
    }
    let (mut mob_end, mut ecs_end) = (vec![None; n], vec![None; n]);
    let (mut mob_far, mut ecs_far) = (0.0_f64, 0.0_f64);
    let mut seen = 0;
    for t in 1..=400u32 {
        let at = Vec3::new(start.x, start.y, start.z + 0.45 * f64::from(t.min(100)) + if t >= 150 { 25.0 } else { 0.0 });
        h.world.resource_mut::<Online>().set(Player { id: 1, x: at.x, y: at.y, z: at.z, main: 0, off: 0, ..Default::default() });
        r.step(Some(&player_view(at, None)));
        h.tick();
        for e in &r.events[seen.min(r.events.len())..] {
            if let MobEvent::LeashBreak { mob, .. } = e
                && let Some(k) = r.mobs.iter().position(|m| m.id == *mob)
                && mob_end[k].is_none()
            {
                mob_end[k] = Some(t);
            }
        }
        seen = r.events.len();
        for k in 0..n {
            let (mp, ep) = (r.mobs[k].position, h.world.get::<Pos>(es[k]).unwrap().0);
            if mob_end[k].is_none() {
                mob_far = mob_far.max(((mp.x - at.x).powi(2) + (mp.z - at.z).powi(2)).sqrt());
            }
            if ecs_end[k].is_none() && h.world.get::<Leash>(es[k]).is_none() {
                ecs_end[k] = Some(t);
            }
            if ecs_end[k].is_none() {
                ecs_far = ecs_far.max(((ep.x - at.x).powi(2) + (ep.z - at.z).powi(2)).sqrt());
            }
        }
    }
    let m: Vec<f64> = mob_end.iter().flatten().map(|&t| f64::from(t)).collect();
    let e: Vec<f64> = ecs_end.iter().flatten().map(|&t| f64::from(t)).collect();
    eprintln!("rope snapped: mob {}/{n} at tick {:.1} (min {:?}), ecs {}/{n} at tick {:.1} (min {:?}); farthest held: mob {mob_far:.2}, ecs {ecs_far:.2}", m.len(), mean(&m), m.iter().cloned().fold(f64::MAX, f64::min), e.len(), mean(&e), e.iter().cloned().fold(f64::MAX, f64::min));
    assert!(m.len() == n && e.len() == n, "every rope snaps: {} {}", m.len(), e.len());
    assert!((mean(&m) - mean(&e)).abs() < 2.0, "the tick of rupture: mob {} ecs {}", mean(&m), mean(&e));
    assert!(mob_far < 12.0 && ecs_far < 12.0 && (mob_far - ecs_far).abs() < 1.0, "{mob_far} {ecs_far}");
}

/// A caravan of three: the first llama is held by the player, two free ones nearby fall in behind it, in a line (8 caravans a side).
#[test]
fn three_llamas_make_a_caravan_as_in_the_original() {
    if skip() {
        return;
    }
    let trials = 8;
    let (mut mob_join, mut ecs_join, mut mob_gap, mut ecs_gap) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for trial in 0..trials {
        let off = 1.5 * trial as f64;
        let pts = [Vec3::new(0.5, 1.0, 6.5), Vec3::new(3.5 + off * 0.2, 1.0, 2.5), Vec3::new(-4.5, 1.0, 1.5 + off * 0.3)];
        let player = Vec3::new(0.5, 1.0, 12.5);
        let mut r = MobRef::new(60, false);
        let mut h = Herd::with_slab(60, true, false);
        let mut es = Vec::new();
        for (k, p) in pts.iter().enumerate() {
            let i = r.add_id(*p, 700 + 11 * k as u32 + 3 * trial as u32);
            if k == 0 {
                r.mobs[i].extra.leash = Some(mob_leash::Holder::Player);
            }
            let e = spawn_species(&mut h.world, species(), p.x, p.z, 21 + k as u64 + 7 * trial as u64);
            h.world.resource_mut::<Entered>().0.push(e);
            es.push(e);
        }
        h.world.resource_mut::<Online>().set(Player { id: 1, x: player.x, y: player.y, z: player.z, main: 0, off: 0, ..Default::default() });
        for _ in 0..3 {
            h.tick();
        }
        assert!(leash::attach(&mut h.world, es[0], Holder::Player(1)));
        let view = player_view(player, None);
        let (mut mj, mut ej) = ([None, None], [None, None]);
        let (mut mg, mut eg) = (Vec::new(), Vec::new());
        for t in 1..=320u32 {
            r.step(Some(&view));
            h.tick();
            for k in 1..3 {
                if mj[k - 1].is_none() && r.mobs[k].extra.ints[1] != 0 {
                    mj[k - 1] = Some(t);
                }
                if ej[k - 1].is_none() && h.world.get::<follow_caravan::Lined>(es[k]).is_some() {
                    ej[k - 1] = Some(t);
                }
            }
            if t > 200 {
                let d = |a: Vec3, b: Vec3| ((a.x - b.x).powi(2) + (a.z - b.z).powi(2)).sqrt();
                // The distance of each follower to the creature it follows.
                for k in 1..3 {
                    if r.mobs[k].extra.ints[1] != 0 {
                        let hd = (r.mobs[k].extra.ints[1] - 1) as u32;
                        if let Some(o) = r.mobs.iter().find(|m| m.id == hd) {
                            mg.push(d(r.mobs[k].position, o.position));
                        }
                    }
                    if let Some(l) = h.world.get::<follow_caravan::Lined>(es[k]) {
                        eg.push(d(h.world.get::<Pos>(es[k]).unwrap().0, h.world.get::<Pos>(l.head).unwrap().0));
                    }
                }
            }
        }
        for k in 0..2 {
            if let Some(t) = mj[k] {
                mob_join.push(f64::from(t));
            }
            if let Some(t) = ej[k] {
                ecs_join.push(f64::from(t));
            }
        }
        mob_gap.push(mean(&mg));
        ecs_gap.push(mean(&eg));
        eprintln!("trial {trial}: joins mob {mj:?} ecs {ej:?}; spacing mob {:.2} ecs {:.2}", mean(&mg), mean(&eg));
        if trial < 3 {
            eprintln!("final mob {:?} ecs {:?}", (0..3).map(|k| (r.mobs[k].position.x as i32, r.mobs[k].position.z as i32)).collect::<Vec<_>>(), es.iter().map(|&e| { let p = h.world.get::<Pos>(e).unwrap().0; (p.x as i32, p.z as i32) }).collect::<Vec<_>>());
            eprintln!("caravan 0: joins mob {mj:?} ecs {ej:?}; the ecs line is {:?}", es.iter().map(|&e| h.world.get::<follow_caravan::Lined>(e).map(|l| l.head)).collect::<Vec<_>>());
        }
    }
    eprintln!("{trials} caravans of 3 llamas: joined {}/{} (mob) and {}/{} (ecs), mean join tick mob {:.1} ecs {:.1}; spacing after tick 200: mob {:.2} ecs {:.2} blocks", mob_join.len(), 2 * trials, ecs_join.len(), 2 * trials, mean(&mob_join), mean(&ecs_join), mean(&mob_gap), mean(&ecs_gap));
    assert!(mob_join.len() == 2 * trials && ecs_join.len() == 2 * trials, "every free llama joins on both sides");
    assert!((mean(&mob_join) - mean(&ecs_join)).abs() < 12.0, "join tick: mob {} ecs {}", mean(&mob_join), mean(&ecs_join));
    assert!((mean(&mob_gap) - mean(&ecs_gap)).abs() < 1.2, "spacing: mob {} ecs {}", mean(&mob_gap), mean(&ecs_gap));
}

/// The lead as a component: put on by the item, listed under its holder, a home at the holder's block while it lasts, the events, off by the holder's
/// empty hand, dropped when the holder is gone; a creature on a lead is the predicate `horizon:is_leashed`.
#[test]
fn a_lead_is_a_component_with_a_directory_events_and_a_home() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab(60, true, false);
    let e = spawn_species(&mut h.world, species(), 2.5, 0.5, 5);
    let o = spawn_species(&mut h.world, species(), 4.5, 0.5, 6);
    h.world.resource_mut::<Entered>().0.extend([e, o]);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 0.5, main: lead(), off: 0, ..Default::default() });
    assert!(!leash::is_leashed(&h.world, e));
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: lead() }), "the lead item puts the lead on");
    assert_eq!(leash::holder_of(&h.world, e), Some(Holder::Player(1)));
    assert!(!interact(&mut h.world, e, &Interaction { player: 2, item: lead() }), "already held");
    assert!(leash::attach(&mut h.world, o, Holder::Entity(e)), "a creature may hold another");
    assert_eq!(leash::held_by(&h.world, Holder::Player(1)), vec![e]);
    assert_eq!(leash::held_by(&h.world, Holder::Entity(e)), vec![o]);
    h.tick();
    h.tick();
    let home = h.world.get::<horizon_ecs_proto::spots::Home>(e).expect("home at the holder");
    assert_eq!((home.center, home.radius), ([0, 1, 0], 5));
    // The holder's empty hand takes it off; the creature it held is free of the home.
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: 0 }));
    h.tick();
    assert!(!leash::is_leashed(&h.world, e) && h.world.get::<horizon_ecs_proto::spots::Home>(e).is_none());
    assert!(leash::held_by(&h.world, Holder::Player(1)).is_empty());
    // A holder that goes away (despawned) drops the lead; a knot holds like a player.
    h.tick();
    h.tick();
    assert!(!leash::is_leashed(&h.world, o) || { h.world.entity_mut(e).despawn(); h.tick(); h.tick(); !leash::is_leashed(&h.world, o) });
    assert!(leash::attach(&mut h.world, o, Holder::Knot([3, 1, 3])));
    assert_eq!(leash::holder_of(&h.world, o), Some(Holder::Knot([3, 1, 3])));
    assert!(leash::detach(&mut h.world, o).is_some());
}

/// The lamas of a trader: held on the trader's lead, they attack whoever hurt the trader (once per blow); a spectating player is not defended against.
#[test]
fn the_lamas_of_a_trader_defend_it() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab(60, true, false);
    let trader_sp = species_named("minecraft:wandering_trader");
    let llama_sp = species_named("minecraft:trader_llama");
    let zombie_sp = species_named("minecraft:cow");
    let trader = spawn_species(&mut h.world, &trader_sp, 0.5, 0.5, 5);
    let l1 = spawn_species(&mut h.world, &llama_sp, 2.5, 0.5, 6);
    let l2 = spawn_species(&mut h.world, &llama_sp, 3.5, 2.5, 7);
    let zombie = spawn_species(&mut h.world, &zombie_sp, 6.5, 0.5, 8);
    h.world.resource_mut::<Entered>().0.extend([trader, l1, l2, zombie]);
    for _ in 0..5 {
        h.tick();
    }
    assert!(leash::attach(&mut h.world, l1, Holder::Entity(trader)) && leash::attach(&mut h.world, l2, Holder::Entity(trader)));
    assert_eq!(defend_leash_holder::defenders_of(&h.world, trader).len(), 2);
    assert!(h.world.get::<Target>(l1).is_none());
    h.world.resource_mut::<DamageQueue>().add_by_entity(trader, 1.0, (6.5, 0.5), zombie);
    let mut when = [None, None];
    for t in 1..=40 {
        h.tick();
        for (k, l) in [l1, l2].into_iter().enumerate() {
            if when[k].is_none() && h.world.get::<Target>(l).is_some_and(|x| x.who == Who::Entity(zombie)) {
                when[k] = Some(t);
            }
        }
    }
    eprintln!("the trader's lamas took the zombie as their target at ticks {when:?} after the blow");
    assert!(when.iter().all(Option::is_some), "both lamas defend: {when:?}");
    // A llama that is not held by anyone gives nothing to defend.
    let loose = spawn_species(&mut h.world, &llama_sp, -3.5, 0.5, 9);
    h.world.resource_mut::<Entered>().0.push(loose);
    h.world.resource_mut::<DamageQueue>().add_by_entity(trader, 1.0, (6.5, 0.5), zombie);
    for _ in 0..40 {
        h.tick();
    }
    assert!(h.world.get::<Target>(loose).is_none(), "a loose llama defends nobody");
}

/// Probe of the open gap of the caravan (PROBE=1): the first ticks of trial 7, which joins at tick 82 here and 5 in the original.
#[test]
fn probe_the_caravan_gaps() {
    if skip() || std::env::var_os("PROBE").is_none() {
        return;
    }
    let off = 1.5 * 7.0;
    let pts = [Vec3::new(0.5, 1.0, 6.5), Vec3::new(3.5 + off * 0.2, 1.0, 2.5), Vec3::new(-4.5, 1.0, 1.5 + off * 0.3)];
    let player = Vec3::new(0.5, 1.0, 12.5);
    let mut h = Herd::with_slab(60, true, false);
    let mut es = Vec::new();
    for (k, p) in pts.iter().enumerate() {
        let e = spawn_species(&mut h.world, species(), p.x, p.z, 21 + k as u64 + 49);
        h.world.resource_mut::<Entered>().0.push(e);
        es.push(e);
    }
    h.world.resource_mut::<Online>().set(Player { id: 1, x: player.x, y: player.y, z: player.z, main: 0, off: 0, ..Default::default() });
    for _ in 0..3 {
        h.tick();
    }
    assert!(leash::attach(&mut h.world, es[0], Holder::Player(1)));
    for t in 1..=90u32 {
        h.tick();
        if t <= 12 || t % 10 == 0 {
            let pos = |k: usize| { let p = h.world.get::<Pos>(es[k]).unwrap().0; (p.x, p.z) };
            let g = |k: usize| h.world.get::<GoalSet>(es[k]).map(|g| g.entries().iter().enumerate().filter(|(i, _)| g.slot_running(*i as u8)).map(|(_, e)| e.module.name).collect::<Vec<_>>());
            eprintln!("t{t}: A {:?} B {:?} {:?} lined {} C {:?} {:?} lined {}", pos(0), pos(1), g(1), h.world.get::<follow_caravan::Lined>(es[1]).is_some(), pos(2), g(2), h.world.get::<follow_caravan::Lined>(es[2]).is_some());
        }
    }
}
