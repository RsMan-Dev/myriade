//! Lot 1 (panda, swell, silverfish, fox, armadillo...) against real `Mob`s of the species (`mob_kind`): the goals are private
//! to the `Mob`, so the state they leave (flags, counters, events) is read, not the trajectories. `--nocapture` prints.
mod common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{self, Mob, MobEvent};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    behaviours::{Entry, follow_parent, look_at_player, random_look, stroll, panda_lie_on_back, panda_roll, panda_sneeze, panda_state as ps, swell, silverfish_wake_up_friends as silver, fox_faceplant, fox_state as fs, nearest_target},
    cow::*,
    goals::*,
};
use std::sync::Arc;

fn leak<T>(v: T) -> &'static T {
    Box::leak(Box::new(v))
}
fn ecs_with(h: &mut Herd, entries: &'static [Entry], x: f64, z: f64, seed: u64) -> Entity {
    let mut sp = *species();
    sp.goals = horizon_ecs_proto::species::Config { entries };
    set_lazy_look(&mut h.world, false);
    let e = spawn_species(&mut h.world, &sp, x, z, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    e
}
fn mob_add(r: &mut MobRef, kind: Arc<mobs::MobType>, at: Vec3) -> usize {
    let mut m = Mob::new(r.mobs.len() as u32 + 1, kind, at, 0.0);
    m.persistent = true;
    m.ai.no_action_time = 100;
    r.mobs.push(m);
    r.searches.push(0);
    r.mobs.len() - 1
}
fn grid(k: usize) -> Vec3 {
    Vec3::new(-16.0 + 2.5 * (k % 14) as f64 + 0.5, 1.0, -16.0 + 2.5 * (k / 14) as f64 + 0.5)
}
fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len().max(1) as f64
}

/// Which panda goal, which gene, baby or not.
#[derive(Clone, Copy, PartialEq)]
enum Pk {
    Sneeze,
    Lie,
    Roll,
}
/// Per panda: the ticks of the rising edges of the watched flag, and the total ticks it was up.
fn pandas(kind: Pk, n: usize, ticks: usize, baby: bool, variant: u8, gene: ps::Gene) -> (Vec<(Vec<usize>, usize)>, Vec<(Vec<usize>, usize)>) {
    let flag = match kind {
        Pk::Sneeze => mobs::panda::FLAG_SNEEZE,
        Pk::Lie => mobs::panda::FLAG_ON_BACK,
        Pk::Roll => mobs::panda::FLAG_ROLL,
    };
    let mut r = MobRef::new(24, false);
    let k = mob_kind("minecraft:panda");
    let ids: Vec<usize> = (0..n)
        .map(|j| {
            let i = mob_add(&mut r, k.clone(), grid(j));
            r.mobs[i].extra.variant = variant;
            r.mobs[i].extra.variant2 = variant;
            if baby {
                mobs::finalize::set_baby(&mut r.mobs[i], true);
            }
            i
        })
        .collect();
    let mut mob = vec![(Vec::new(), 0usize); n];
    let mut up = vec![false; n];
    for t in 0..ticks {
        r.step(None);
        for (j, &i) in ids.iter().enumerate() {
            let now = mobs::panda::get(&r.mobs[i]).is_some_and(|s| s.flags & flag != 0);
            if now && !up[j] {
                mob[j].0.push(t);
            }
            if now {
                mob[j].1 += 1;
            }
            up[j] = now;
        }
    }
    let entries: &'static [Entry] = match kind {
        Pk::Sneeze => leak([Entry::new(&panda_sneeze::MODULE, 8, &panda_sneeze::Params::DEFAULT)]),
        Pk::Lie => leak([Entry::new(&panda_lie_on_back::MODULE, 8, &panda_lie_on_back::Params::DEFAULT)]),
        // The real panda's competitors for the flags: the look goals (9, 10) and the stroll (14) keep MOVE / LOOK part of
        // the time, and the `Mob` does not even ask `canUse` then.
        Pk::Roll => leak([
            Entry::new(&look_at_player::MODULE, 9, &look_at_player::Params { range: 6.0, ..look_at_player::Params::DEFAULT }),
            Entry::new(&random_look::MODULE, 10, &random_look::Params::DEFAULT),
            Entry::new(&panda_roll::MODULE, 12, &panda_roll::Params::DEFAULT),
            Entry::new(&stroll::MODULE, 14, &stroll::Params::DEFAULT),
        ]),
    };
    let ef = match kind {
        Pk::Sneeze => ps::FLAG_SNEEZE,
        Pk::Lie => ps::FLAG_ON_BACK,
        Pk::Roll => ps::FLAG_ROLL,
    };
    let mut h = Herd::with_slab(24, false, false);
    let es: Vec<Entity> = (0..n)
        .map(|j| {
            let p = grid(j);
            let e = ecs_with(&mut h, entries, p.x, p.z, 500 + j as u64);
            h.world.get_mut::<Phase>(e).unwrap().0 = u64::from(r.mobs[ids[j]].id);
            h.world.entity_mut(e).insert(gene);
            if baby {
                follow_parent::make_baby(&mut h.world, e, 24_000);
            }
            e
        })
        .collect();
    let mut ecs = vec![(Vec::new(), 0usize); n];
    let mut eup = vec![false; n];
    for t in 0..ticks {
        h.tick();
        for (j, &e) in es.iter().enumerate() {
            let now = ps::flag(&h.world, e, ef);
            if now && !eup[j] {
                ecs[j].0.push(t);
            }
            if now {
                ecs[j].1 += 1;
            }
            eup[j] = now;
            // Sneeze and roll are ended by the panda's own rule (not ported): emulate it by clearing the flag once seen
            // for the Mob's own duration, so the next edge can be counted.
            if kind != Pk::Lie && now {
                let hold = if kind == Pk::Sneeze { 20 } else { 32 };
                if ecs[j].1 % hold == 0 {
                    ps::set_flag(&mut h.world, e, ef, false);
                }
            }
        }
    }
    (mob, ecs)
}
fn rate(v: &[(Vec<usize>, usize)]) -> f64 {
    v.iter().map(|p| p.0.len()).sum::<usize>() as f64 / v.len() as f64
}
fn first(v: &[(Vec<usize>, usize)]) -> (usize, f64) {
    let f: Vec<f64> = v.iter().filter_map(|p| p.0.first().map(|&t| t as f64)).collect();
    (f.len(), mean(&f))
}

#[test]
fn panda_sneeze_rate_of_a_cub() {
    if !have_data() {
        return;
    }
    let ticks = 20_000;
    for (name, variant, gene) in [("normal", 0u8, ps::Gene::Normal), ("weak", 5u8, ps::Gene::Weak)] {
        let (m, e) = pandas(Pk::Sneeze, 80, ticks, true, variant, gene);
        eprintln!("panda_sneeze {name} cub: sneezes per panda in {ticks} ticks: mob {:.3} ecs {:.3}; first at mob {:?} ecs {:?}", rate(&m), rate(&e), first(&m), first(&e));
        let (rm, re) = (rate(&m), rate(&e));
        assert!((rm - re).abs() / rm.max(0.5) < 0.25, "{name}: {rm} vs {re}");
    }
}
#[test]
fn panda_lie_on_back_law() {
    if !have_data() {
        return;
    }
    let ticks = 30_000;
    let (m, e) = pandas(Pk::Lie, 80, ticks, false, 1, ps::Gene::Lazy);
    let tot = |v: &[(Vec<usize>, usize)]| v.iter().map(|p| p.1).sum::<usize>() as f64 / v.len() as f64;
    eprintln!("panda_lie_on_back lazy adult, {ticks} ticks: episodes per panda mob {:.2} ecs {:.2}; ticks on the back mob {:.0} ecs {:.0}; first at mob {:?} ecs {:?}", rate(&m), rate(&e), tot(&m), tot(&e), first(&m), first(&e));
    assert!((rate(&m) - rate(&e)).abs() / rate(&m) < 0.25);
    assert!((tot(&m) - tot(&e)).abs() / tot(&m) < 0.3);
    // A non-lazy panda never lies down on either side.
    let (m0, e0) = pandas(Pk::Lie, 20, 10_000, false, 0, ps::Gene::Normal);
    assert_eq!((rate(&m0), rate(&e0)), (0.0, 0.0));
}
#[test]
fn panda_roll_first_roll_of_a_cub() {
    if !have_data() {
        return;
    }
    let (m, e) = pandas(Pk::Roll, 80, 6_000, true, 0, ps::Gene::Normal);
    eprintln!("panda_roll normal cub: first roll (count, mean tick) mob {:?} ecs {:?}; rolls per panda in 6000 ticks mob {:.2} ecs {:.2}", first(&m), first(&e), rate(&m), rate(&e));
    let (fm, fe) = (first(&m).1, first(&e).1);
    assert!((fm - fe).abs() / fm < 0.3, "{fm} vs {fe}");
}

/// One run: the player appears at 2 blocks at tick 20, goes to 5 at 60, 8 at 100, 5 at 140, 2 at 180. Returns, for each
/// side, the direction per tick and the tick at which a target was held.
fn swell_run(salt: usize) -> ((Vec<i32>, Option<usize>), (Vec<i32>, Option<usize>)) {
    use mobs::goals::NearestTarget;
    let at = |t: usize| -> Option<f64> {
        match t {
            0..=19 => None,
            20..=59 => Some(2.0),
            60..=99 => Some(5.0),
            100..=139 => Some(8.0),
            140..=179 => Some(5.0),
            _ => Some(2.0),
        }
    };
    let mut r = MobRef::new(20, false);
    for _ in 0..salt {
        let k = r.kind.clone();
        mob_add(&mut r, k, Vec3::new(15.5, 1.0, 15.5));
    }
    let i = r.add(Vec3::new(0.5, 1.0, 0.5));
    r.mobs[i].ai.goals.clear();
    r.mobs[i].ai.targets.clear();
    r.mobs[i].ai.goals.add(2, mobs::hostile::Swell::default());
    let _ = NearestTarget::player(true);
    // The creeper starts with its sense at -1 (`hostile.rs:38`).
    r.mobs[i].extra.counter = -1;
    let (mut mob, mut mt) = (Vec::new(), None);
    for t in 0..220 {
        let pv = at(t).map(|d| player_view(Vec3::new(0.5 + d, 1.0, 0.5), None));
        if t == 20 {
            // The target is imposed (the acquisition law is the target service's, not the goal's).
            r.mobs[i].ai.target = Some(mobs::Who::Player);
        }
        r.step(pv.as_ref());
        mob.push(r.mobs[i].extra.counter);
        if mt.is_none() && r.mobs[i].ai.target.is_some() {
            mt = Some(t);
        }
    }
    let entries: &'static [Entry] = leak([Entry::new(&swell::MODULE, 2, &swell::Params::DEFAULT)]);
    let mut h = Herd::with_slab(20, false, false);
    let e = ecs_with(&mut h, entries, 0.5, 0.5, 77 + salt as u64);
    h.world.get_mut::<Phase>(e).unwrap().0 = u64::from(r.mobs[i].id);
    let (mut ecs, mut et) = (Vec::new(), None);
    for t in 0..220 {
        if let Some(d) = at(t) {
            h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5 + d, y: 1.0, z: 0.5, main: 0, off: 0 });
        }
        if t == 20 {
            horizon_ecs_proto::targeting::set_target(&mut h.world, e, horizon_ecs_proto::targeting::Target::player(1, 60));
        }
        h.tick();
        ecs.push(i32::from(h.world.get::<swell::SwellDir>(e).unwrap().0));
        if et.is_none() && h.world.get::<horizon_ecs_proto::targeting::Target>(e).is_some() {
            et = Some(t);
        }
    }
    ((mob, mt), (ecs, et))
}
#[test]
fn swell_direction_follows_the_player() {
    if !have_data() {
        return;
    }
    let first_up = |v: &[i32]| v.iter().position(|&d| d > 0);
    let (mut lm, mut le, mut edge_bad) = (Vec::new(), Vec::new(), 0);
    for salt in 0..16 {
        let ((m, mt), (e, et)) = swell_run(salt);
        // After the first rise, the directions must agree tick by tick (the later edges depend on the distances only).
        let edges = |v: &[i32]| v.windows(2).enumerate().filter(|(_, p)| p[0] != p[1]).map(|(t, p)| (t + 1, p[1])).collect::<Vec<_>>();
        let (em, ee) = (edges(&m), edges(&e));
        if salt < 2 {
            eprintln!("swell salt {salt}: target held at mob {mt:?} ecs {et:?}; edges mob {em:?} ecs {ee:?}");
        }
        if let (Some(a), Some(b)) = (mt, first_up(&m)) {
            lm.push((b - a) as f64);
        }
        if let (Some(a), Some(b)) = (et, first_up(&e)) {
            le.push((b - a) as f64);
        }
        // The two later edges (8 blocks: down, 2 blocks: up) at the same tick, to 2 ticks.
        if em.len() == 3 && ee.len() == 3 {
            edge_bad += usize::from(em[1].0.abs_diff(ee[1].0) > 2) + usize::from(em[2].0.abs_diff(ee[2].0) > 2);
        } else {
            edge_bad += 1;
        }
    }
    eprintln!("swell: lag from the target to the first +1 (ticks): mob {:.2} (n {}) ecs {:.2} (n {}); later edges more than 2 ticks apart: {edge_bad}", mean(&lm), lm.len(), mean(&le), le.len());
    assert!((mean(&lm) - mean(&le)).abs() <= 1.5);
    assert_eq!(edge_bad, 0);
}

#[test]
fn silverfish_wake_up_friends_after_a_hit() {
    if !have_data() {
        return;
    }
    let ring: Vec<(i32, i32, i32)> = (-4i32..=4).flat_map(|x| (-4i32..=4).flat_map(move |z| (1i32..=2).map(move |y| (x, y, z)))).filter(|&(x, _, z)| x.abs() >= 2 || z.abs() >= 2).collect();
    let mut r = MobRef::new(20, false);
    // No stone floor in reach (the silverfish hides in a host block it stands on: it would leave the trial).
    for x in -14..=14 {
        for z in -14..=14 {
            r.set_block(x, 0, z, "minecraft:dirt");
        }
    }
    let i = mob_add(&mut r, mob_kind("minecraft:silverfish"), Vec3::new(0.5, 1.0, 0.5));
    r.mobs[i].ai.no_action_time = 0;
    let entries: &'static [Entry] = leak([Entry::new(&silver::MODULE, 3, &silver::Params::DEFAULT)]);
    let mut h = Herd::with_slab(20, false, false);
    let e = ecs_with(&mut h, entries, 0.5, 0.5, 91);
    for _ in 0..10 {
        r.step(None);
        h.tick();
    }
    let (trials, run) = (150, 40);
    let (mut delay_m, mut delay_e, mut n_m, mut n_e) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut quiet = (0, 0);
    for k in 0..trials + 10 {
        let falling = k >= trials;
        for &(x, y, z) in &ring {
            r.set_block(x, y, z, "minecraft:infested_stone");
            h.set_block(x, y, z, "minecraft:infested_stone");
        }
        // The silverfish is put back (the `Mob` walks off after a hit) and healed.
        {
            let m = &mut r.mobs[i];
            m.position = Vec3::new(0.5, 1.0, 0.5);
            m.velocity = Vec3::ZERO;
            m.health = 8.0;
            m.nav_stop();
            m.extra.counter = 0;
        }
        r.step(None);
        h.tick();
        let from = r.events.len();
        if falling {
            // A damage without an attacker (a fall): nobody wakes.
            h.world.resource_mut::<DamageQueue>().add(e, 0.01, Cause::Fall);
        } else {
            r.hit(i, 0.01, [0.5, 1.0, 3.5]);

            h.hit(e, 0.01, (0.5, 3.5));
        }
        let (mut dm, mut de) = (None, None);
        let mut nm = 0;
        for t in 0..run {
            r.step(None);
            h.tick();
            let broken = r.events[from..].iter().filter(|ev| matches!(ev, MobEvent::BreakBlock { .. })).count();
            if broken > 0 && dm.is_none() {
                dm = Some(t);
            }
            nm = broken;
            let lvl = h.world.resource::<Level>().mw.0;
            let gone = ring.iter().filter(|&&(x, y, z)| lvl.block_name(x, y, z) != "minecraft:infested_stone").count();
            if gone > 0 && de.is_none() {
                de = Some(t);
            }
            if t == run - 1 {
                n_e.push(gone as f64);
            }
        }
        n_m.push(nm as f64);
        if falling {
            quiet = (quiet.0 + usize::from(dm.is_some()), quiet.1 + usize::from(de.is_some()));
        } else {
            delay_m.extend(dm.map(|d| d as f64));
            delay_e.extend(de.map(|d| d as f64));
        }
    }
    eprintln!(
        "silverfish: hit at +0, first block broken after (ticks) mob {:.1} (n {}) ecs {:.1} (n {}); blocks per wake-up mob {:.2} ecs {:.2}; woken by a fall: mob {} ecs {}",
        mean(&delay_m), delay_m.len(), mean(&delay_e), delay_e.len(), mean(&n_m[..trials]), mean(&n_e[..trials]), quiet.0, quiet.1
    );
    assert!((mean(&delay_m) - mean(&delay_e)).abs() <= 2.0, "delay differs");
    assert!((mean(&n_m[..trials]) - mean(&n_e[..trials])).abs() / mean(&n_m[..trials]) < 0.25, "number of blocks differs");
    assert_eq!(quiet.0 > 0, quiet.1 > 0);
}

#[test]
fn fox_faceplant_lasts_forty_ticks() {
    if !have_data() {
        return;
    }
    let mut r = MobRef::new(20, false);
    let i = mob_add(&mut r, mob_kind("minecraft:fox"), Vec3::new(0.5, 1.0, 0.5));
    let entries: &'static [Entry] = leak([Entry::new(&fox_faceplant::MODULE, 1, &fox_faceplant::Params::DEFAULT)]);
    let mut h = Herd::with_slab(20, false, false);
    let e = ecs_with(&mut h, entries, 0.5, 0.5, 5);
    for _ in 0..20 {
        r.step(None);
        h.tick();
    }
    let (mut dm, mut de) = (Vec::new(), Vec::new());
    for k in 0..6 {
        for _ in 0..k {
            r.step(None);
            h.tick();
        }
        r.mobs[i].extra.ints[0] |= 64;
        fs::set(&mut h.world, e, fs::FACEPLANTED, true);
        wake(&mut h.world, e, &fox_faceplant::MODULE);
        let (mut a, mut b) = (0, 0);
        for _ in 0..80 {
            r.step(None);
            h.tick();
            a += usize::from(r.names(i).contains(&"Faceplant"));
            b += usize::from(h.world.get::<GoalSet>(e).unwrap().is_running(&fox_faceplant::MODULE));
        }
        dm.push(a);
        de.push(b);
        eprintln!("faceplant start offset {k}: ticks running mob {a} ecs {b}; flag after: mob {} ecs {}", r.mobs[i].extra.ints[0] & 64, fs::has(&h.world, e, fs::FACEPLANTED) as u8);
    }
    for (a, b) in dm.iter().zip(&de) {
        assert!(a.abs_diff(*b) <= 2, "{dm:?} vs {de:?}");
    }
}
