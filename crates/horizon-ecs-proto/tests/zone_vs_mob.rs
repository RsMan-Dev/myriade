//! The simulated chunks (`zone.rs`) against Horizon's rule (`ticket_levels.rs`) and against a real `Mob` that the game's
//! pass skips while its chunk does not tick: the same chunks tick, a sleeping creature does not move or age, and wakes
//! with its timers where it left them.
mod common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::ticket_levels::{ChunkTickets, Levels, Sources};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, goals::*, zone::Dormant};

const HALF: i32 = 100;

fn chunk(v: f64) -> i32 {
    (v / 16.0).floor() as i32
}
/// Horizon's levels for players at `at`, with `view` and `sim` distances.
fn oracle(at: &[(f64, f64)], view: i32, sim: i32) -> Levels {
    let mut t = ChunkTickets::default();
    t.update(Some(Sources { players: at.iter().map(|p| (chunk(p.0), chunk(p.1))).collect(), view_distance: view, simulation_distance: sim, ..Default::default() }));
    t.levels()
}
fn pair() -> (MobRef, Herd) {
    (MobRef::new(HALF, false), Herd::with_slab(HALF, true, false))
}

/// The chunks that tick are the chunks of Horizon's tickets, for the view and simulation distances of a server, as the
/// players come, move and go.
#[test]
fn the_chunks_that_tick_are_those_of_the_tickets() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(HALF, true, false);
    h.set_simulation(2);
    let steps: Vec<Vec<(f64, f64)>> = vec![
        vec![(0.5, 0.5)],
        vec![(17.5, 0.5)],
        vec![(17.5, 40.5), (-70.0, -70.0)],
        vec![(-70.0, -64.5)],
        vec![(-70.0, -64.5), (90.0, 90.0), (0.0, 0.0)],
        vec![(300.0, 0.0)],
        vec![],
        vec![(-10.0, 10.0)],
    ];
    for (view, sim) in [(10, 10), (6, 6), (10, 6), (6, 10), (3, 8), (2, 2)] {
        h.set_simulation(view.min(sim));
        for list in &steps {
            h.set_players(list.clone());
            h.tick();
            let lv = oracle(list, view, sim);
            for cx in -7..=6 {
                for cz in -7..=6 {
                    assert_eq!(h.chunk_simulated(cx, cz), !list.is_empty() && lv.entity_ticking(cx, cz), "view {view} sim {sim} players {list:?} chunk ({cx},{cz})");
                }
            }
        }
    }
}

/// What the run of a creature that sleeps for `gap` ticks shows: the tick (counted from the start of the loop) the baby
/// grew up, the remaining love, the health after a fire, and where the fire went out.
#[derive(Debug, PartialEq)]
struct Seen {
    grew: Option<i64>,
    love_left: i64,
    fire_out: Option<i64>,
    health: f32,
}

#[derive(Clone, Copy)]
enum What {
    Baby,
    Love,
    Fire,
}

/// One creature at (60, 0), a player beside it for `s1` ticks, then far away for `gap`, then back for `s2`.
/// Returns what the `Mob` and the ecs creature showed.
fn run(gap: i64, s1: i64, s2: i64, what: What) -> (Seen, Seen, bool) {
    let (mut r, mut h) = pair();
    h.set_simulation(6);
    let (x, z) = (60.5, 0.5);
    let near = (50.0, 0.0);
    let far = (-90.0, -90.0);
    h.set_players(vec![near]);
    h.tick();
    let i = r.add(Vec3::new(x, 1.0, z));
    let e = ecs_cow(&mut h, x, 1.0, z, 91);
    match what {
        // A baby that grows in 400 ticks; a love of 5000; a fire of 160 (a baby is never in love, a hurt ends love).
        What::Baby => {
            r.mobs[i].extra.age = -400;
            h.make_baby(e, 400);
        }
        What::Love => {
            let now = h.world.resource::<Clock>().0;
            r.mobs[i].extra.in_love = 5000;
            h.world.entity_mut(e).insert(Love(now + 5000));
        }
        What::Fire => {
            // (Enough health to live through the fire, whatever the species.)
            r.mobs[i].health = 100.0;
            h.world.get_mut::<Health>(e).unwrap().0 = 100.0;
            r.mobs[i].fire_ticks = 160;
            ignite(&mut h.world, e, 160);
        }
    }
    let (mut mob, mut ecs) = (Seen { grew: None, love_left: 0, fire_out: None, health: 0.0 }, Seen { grew: None, love_left: 0, fire_out: None, health: 0.0 });
    let mut slept = false;
    let (lv_near, lv_far) = (oracle(&[near], 6, 6), oracle(&[far], 6, 6));
    for t in 1..=(s1 + gap + s2) {
        let (list, lv) = if t > s1 && t <= s1 + gap { (far, &lv_far) } else { (near, &lv_near) };
        if t == 1 || t == s1 + 1 || t == s1 + gap + 1 {
            h.set_players(vec![list]);
        }
        let pv = player_view(Vec3::new(list.0, 1.0, list.1), None);
        r.step_where(Some(&pv), |m| lv.entity_ticking_at(m.position.x.floor() as i32, m.position.z.floor() as i32));
        h.tick();
        slept |= h.world.get::<Dormant>(e).is_some();
        if mob.grew.is_none() && r.mobs[i].extra.age >= 0 {
            mob.grew = Some(t);
        }
        if ecs.grew.is_none() && h.world.get::<GrowAt>(e).is_none() {
            ecs.grew = Some(t);
        }
        if mob.fire_out.is_none() && r.mobs[i].fire_ticks <= 0 {
            mob.fire_out = Some(t);
        }
        if ecs.fire_out.is_none() && h.world.get::<Burning>(e).is_none() {
            ecs.fire_out = Some(t);
        }
    }
    eprintln!("end: dormant {} burning {:?} grow {:?} clock {} pos {:?}", h.world.get::<Dormant>(e).is_some(), h.world.get::<Burning>(e).map(|b| b.until), h.world.get::<GrowAt>(e).map(|g| g.0), h.world.resource::<Clock>().0, h.world.get::<Pos>(e).map(|p| p.0));
    mob.love_left = i64::from(r.mobs[i].extra.in_love.max(0));
    let clock = h.world.resource::<Clock>().0 as i64;
    ecs.love_left = h.world.get::<Love>(e).map_or(0, |l| (l.0 as i64 - clock).max(0));
    mob.health = mob_health(&r.mobs[i]);
    ecs.health = health(&h, e);
    (mob, ecs, slept)
}

/// Sleeping for `gap` ticks changes nothing for a creature but the tick it is on: the baby grows up, the love and the fire
/// run out after the same number of ticks it was ticked, as the `Mob` that was not ticked.
#[test]
fn a_creature_that_sleeps_keeps_its_timers_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let (s1, s2) = (100, 500);
    let (m0, e0, slept0) = run(0, s1, s2, What::Baby);
    assert!(!slept0, "a creature beside a player never sleeps");
    let (mg, eg, slept) = run(700, s1, s2, What::Baby);
    let (ml, el, _) = run(700, s1, s2, What::Love);
    let (mf, ef, _) = run(700, s1, s2, What::Fire);
    eprintln!("baby, no gap: mob {m0:?} ecs {e0:?}\nbaby, gap 700: mob {mg:?} ecs {eg:?}\nlove: mob {ml:?} ecs {el:?}\nfire: mob {mf:?} ecs {ef:?}");
    assert!(slept, "the creature must have slept");
    // The Mob after the gap equals the Mob with no gap, shifted by the gap.
    assert_eq!(mg.grew.map(|t| t - 700), m0.grew);
    // The ecs creature against the Mob (within a tick: the two count a deadline on either side of the tick).
    let close = |a: Option<i64>, b: Option<i64>| a.zip(b).is_some_and(|(a, b)| (a - b).abs() <= 1);
    assert!(close(eg.grew, mg.grew), "baby: mob {:?} ecs {:?}", mg.grew, eg.grew);
    assert!(ml.love_left == 4400 && (el.love_left - ml.love_left).abs() <= 1, "love: mob {} ecs {}", ml.love_left, el.love_left);
    assert!(close(ef.fire_out, mf.fire_out), "fire: mob {:?} ecs {:?}", mf.fire_out, ef.fire_out);
    assert_eq!(ef.health, mf.health, "health after the fire");
}

/// A sleeping creature has no wheel entry, no marker and no work; it does not move; it wakes with them again.
#[test]
fn a_sleeping_creature_is_out_of_the_wheel_and_does_not_move() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(HALF, true, false);
    h.set_players(vec![(50.0, 0.0)]);
    h.set_simulation(6);
    h.tick();
    let e = ecs_cow(&mut h, 60.5, 1.0, 0.5, 5);
    h.make_baby(e, 3000);
    for _ in 0..60 {
        h.tick();
    }
    let with = h.world.resource::<Wheel>().len();
    assert!(with > 0);
    h.set_players(vec![(-90.0, -90.0)]);
    h.tick();
    assert!(h.world.get::<Dormant>(e).is_some());
    assert_eq!(h.world.resource::<Wheel>().len(), 0, "a sleeping creature holds no wheel entry");
    let at = h.world.get::<Pos>(e).unwrap().0;
    let rng = h.world.get::<Rng>(e).unwrap().0;
    for _ in 0..2000 {
        h.tick();
    }
    assert_eq!(h.world.get::<Pos>(e).unwrap().0, at);
    assert_eq!(h.world.get::<Rng>(e).unwrap().0, rng, "no draw while asleep");
    h.set_players(vec![(50.0, 0.0)]);
    h.tick();
    assert!(h.world.get::<Dormant>(e).is_none());
    assert_eq!(h.world.resource::<Wheel>().len(), with, "its entries are back");
}

/// A creature that walks out of the simulated chunks falls asleep where it stands, and one that comes back with a player is
/// woken.
#[test]
fn a_creature_walking_to_the_border_falls_asleep_there() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(HALF, true, false);
    // The player's chunk is 0: with a radius of 2 the chunks -2..=2 tick, so x < 48.
    h.set_players(vec![(8.0, 8.0)]);
    h.set_simulation(2);
    h.tick();
    let e = ecs_cow(&mut h, 40.5, 1.0, 8.5, 6);
    let mut at = Vec::new();
    for t in 0..80 {
        h.world.get_mut::<Vel>(e).unwrap().0.x = 0.12;
        h.world.entity_mut(e).insert(Moving);
        h.tick();
        at.push((t, h.world.get::<Pos>(e).unwrap().0.x, h.world.get::<Dormant>(e).is_some()));
        if at.last().unwrap().2 {
            break;
        }
    }
    let (t, x, asleep) = *at.last().unwrap();
    assert!(asleep, "never slept: {at:?}");
    assert!((48.0..49.5).contains(&x), "it slept at x = {x} (tick {t}), not at the border (48)");
    for _ in 0..50 {
        h.tick();
    }
    assert_eq!(h.world.get::<Pos>(e).unwrap().0.x, x);
    // A player walks to it: the chunk ticks, the creature wakes.
    h.set_players(vec![(40.0, 8.0)]);
    h.tick();
    assert!(h.world.get::<Dormant>(e).is_none());
}

/// The egg timer of a hen (the process's species must be the chicken) counts only the ticks the hen was ticked.
#[test]
fn a_hen_that_sleeps_lays_after_the_same_ticks_it_was_ticked() {
    if !have_data() {
        return;
    }
    let _ = pair();
    if species().name != "minecraft:chicken" {
        return;
    }
    let lay = |gap: i64| {
        let (mut r, mut h) = pair();
        h.set_simulation(6);
        let (near, far) = ((50.0, 0.0), (-90.0, -90.0));
        h.set_players(vec![near]);
        h.tick();
        let i = r.add(Vec3::new(60.5, 1.0, 0.5));
        let e = ecs_cow(&mut h, 60.5, 1.0, 0.5, 33);
        r.mobs[i].extra.egg_time = 6000;
        horizon_ecs_proto::farm::set_egg(&mut h.world, e, 6000);
        let (lv_near, lv_far) = (oracle(&[near], 6, 6), oracle(&[far], 6, 6));
        let (mut seen, mut m, mut c) = (0, None, None);
        for t in 1..=(100 + gap + 6000) {
            let (list, lv) = if t > 100 && t <= 100 + gap { (far, &lv_far) } else { (near, &lv_near) };
            if t == 101 || t == 101 + gap {
                h.set_players(vec![list]);
            }
            let pv = player_view(Vec3::new(list.0, 1.0, list.1), None);
            r.step_where(Some(&pv), |mb| lv.entity_ticking_at(mb.position.x.floor() as i32, mb.position.z.floor() as i32));
            h.tick();
            for ev in &r.events[seen..] {
                if matches!(ev, horizon_client_lib::mobs::MobEvent::LayEgg { .. }) && m.is_none() {
                    m = Some(t);
                }
            }
            seen = r.events.len();
            if c.is_none() && h.world.resource::<horizon_ecs_proto::farm::LaidEggs>().0.iter().any(|x| x.0 == e) {
                c = Some(t);
            }
        }
        (m, c)
    };
    let (m0, c0) = lay(0);
    let (mg, cg) = lay(700);
    eprintln!("egg: no gap mob {m0:?} ecs {c0:?}; gap 700 mob {mg:?} ecs {cg:?}");
    let (mg, cg) = (mg.expect("mob laid"), cg.expect("ecs laid"));
    assert_eq!(m0.map(|t| t + 700), Some(mg));
    assert!((mg - cg).abs() <= 1, "mob {mg}, ecs {cg}");
}
