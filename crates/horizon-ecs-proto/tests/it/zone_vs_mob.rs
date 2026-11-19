//! The simulated chunks (`zone.rs`) against Horizon's rule (`ticket_levels.rs`) and against a real `Mob` that the game's
//! pass skips while its chunk does not tick: the same chunks tick, a sleeping creature does not move or age, and wakes
//! with its timers where it left them.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::ticket_levels::{ChunkTickets, Levels, Sources};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, goals::*, targeting::Target, zone::Dormant};

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
    let clock = h.world.resource::<Clock>().of(e) as i64;
    ecs.love_left = h.world.get::<Love>(e).map_or(0, |l| (l.0 as i64 - clock).max(0));
    mob.health = mob_health(&r.mobs[i]);
    ecs.health = health(&h, e);
    (mob, ecs, slept)
}

/// Sleeping for `gap` ticks changes nothing for a creature but the tick it is on: the baby grows up, the love and the fire
/// run out after the same number of ticks it was ticked, as the `Mob` that was not ticked.
#[test]
fn a_creature_that_sleeps_keeps_its_timers_as_a_mob_does() {
    if horizon_ecs_proto::cow::skip_unless_small_body("a_creature_that_sleeps_keeps_its_timers_as_a_mob_does") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("a_creature_that_sleeps_keeps_its_timers_as_a_mob_does") {
        return;
    }
    if !have_data() {
        return;
    }
    let (s1, s2) = (100, 500);
    let _ = pair();
    // A monster is not a baby and is never in love: its own timers are the fire and the zombie test below.
    let monster = !species().goals.has(&horizon_ecs_proto::behaviours::breed::MODULE);
    let (mf, ef, slept) = run(700, s1, s2, What::Fire);
    if monster {
        assert!(slept);
        let close = |a: Option<i64>, b: Option<i64>| a.zip(b).is_some_and(|(a, b)| (a - b).abs() <= 1);
        assert!(close(ef.fire_out, mf.fire_out), "fire: mob {:?} ecs {:?}", mf.fire_out, ef.fire_out);
        assert_eq!(ef.health, mf.health, "health after the fire");
        return;
    }
    let (m0, e0, slept0) = run(0, s1, s2, What::Baby);
    assert!(!slept0, "a creature beside a player never sleeps");
    let (mg, eg, slept) = run(700, s1, s2, What::Baby);
    let (ml, el, _) = run(700, s1, s2, What::Love);
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
    if horizon_ecs_proto::cow::skip_unless_small_body("a_sleeping_creature_is_out_of_the_wheel_and_does_not_move") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("a_sleeping_creature_is_out_of_the_wheel_and_does_not_move") {
        return;
    }
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
    if horizon_ecs_proto::cow::skip_unless_ground_only("a_creature_walking_to_the_border_falls_asleep_there") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_small_body("a_creature_walking_to_the_border_falls_asleep_there") {
        return;
    }
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

/// A herd that strolls across the border of the zone while the player walks back and forth: at every tick the creatures of the
/// chunks that tick are awake and the others asleep, and none of them is lost or made twice.
#[test]
fn the_herd_stays_awake_or_asleep_by_its_chunk() {
    if horizon_ecs_proto::cow::skip_unless_small_body("the_herd_stays_awake_or_asleep_by_its_chunk") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("the_herd_stays_awake_or_asleep_by_its_chunk") {
        return;
    }
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(HALF, true, false);
    h.set_players(vec![(0.0, 0.0)]);
    h.set_simulation(2);
    let at: Vec<(f64, f64)> = (0..400).map(|i| (-60.0 + f64::from(i % 20) * 6.0 + 0.5, -60.0 + f64::from(i / 20) * 6.0 + 0.5)).collect();
    let all = h.populate(&at, 5);
    let (mut slept_max, mut moved) = (0, 0);
    let mut cells: Vec<u32> = all.iter().map(|&e| h.world.get::<Cell>(e).unwrap().0).collect();
    for t in 0..1500 {
        // The player walks 0..80 blocks east and back, 0.2 block a tick.
        let x = ((t % 800) as f64 * 0.2 - if t % 800 > 400 { (t % 800 - 400) as f64 * 0.4 } else { 0.0 }).max(-30.0);
        h.set_players(vec![(x, 0.0)]);
        h.tick();
        let mut asleep = 0;
        for (k, &e) in all.iter().enumerate() {
            let cell = h.world.get::<Cell>(e).expect("a creature is never lost").0;
            let sleeping = h.world.get::<Dormant>(e).is_some();
            let ticks = h.world.resource::<Index>().zone.simulated_cell(cell);
            // A creature that walked into a chunk that does not tick during this tick falls asleep at the start of the next: it
            // is the only one that may be awake out of the zone.
            assert!(sleeping == !ticks || (!sleeping && !ticks && cell != cells[k]), "tick {t}: creature {k} asleep {sleeping}, its chunk ticks {ticks}, cell {} -> {cell}", cells[k]);
            cells[k] = cell;
            asleep += usize::from(sleeping);
        }
        slept_max = slept_max.max(asleep);
        moved = h.world.resource::<Index>().zone.stats.woke;
    }
    assert!(slept_max > 100 && moved > 50, "the zone never moved over the herd: {slept_max} asleep at most, {moved} woke");
}

/// What a ring of zombies around a player did: the ticks of their blows (the hits on the player, in the world's ticks).
fn zombie_hits(gap: i64, s1: i64, s2: i64) -> (Vec<i64>, Vec<i64>, bool) {
    use horizon_ecs_proto::hostile::PlayerState;
    let mut r = MobRef::new(HALF, false);
    r.kind = mob_kind("minecraft:zombie");
    let mut h = Herd::with_slab(HALF, true, false);
    h.world.resource_mut::<PlayerState>().immortal = true;
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let (target, near, far) = ((60.0, 0.0), (50.0, 0.0), (-90.0, -90.0));
    h.set_players(vec![near]);
    h.set_simulation(6);
    h.tick();
    let spots: Vec<(f64, f64)> = (0..4).map(|i| (target.0 + 6.0 * f64::from(i).cos(), target.1 + 6.0 * f64::from(i).sin())).collect();
    for (i, s) in spots.iter().enumerate() {
        r.add(Vec3::new(s.0, 1.0, s.1));
        let e = spawn_species(&mut h.world, &table[2], s.0, s.1, 1000 + i as u64);
        h.world.entity_mut(e).insert(ActiveUntil(u64::MAX));
        h.world.resource_mut::<Entered>().0.push(e);
    }
    h.world.resource_mut::<Online>().set(Player { id: 1, x: target.0, y: 1.0, z: target.1, main: 0, off: 0, ..Default::default() });
    let (lv_near, lv_far) = (oracle(&[near], 6, 6), oracle(&[far], 6, 6));
    let pv = player_view(Vec3::new(target.0, 1.0, target.1), None);
    let (mut mob, mut ecs, mut seen, mut slept) = (Vec::new(), Vec::new(), 0, false);
    let t0 = h.world.resource::<Clock>().0 as i64;
    for t in 1..=(s1 + gap + s2) {
        let (list, lv) = if t > s1 && t <= s1 + gap { (far, &lv_far) } else { (near, &lv_near) };
        if t == 1 || t == s1 + 1 || t == s1 + gap + 1 {
            h.set_players(vec![list]);
        }
        let from = r.events.len();
        r.step_where(Some(&pv), |m| lv.entity_ticking_at(m.position.x.floor() as i32, m.position.z.floor() as i32));
        h.tick();
        for e in &r.events[from..] {
            if matches!(e, horizon_client_lib::mobs::MobEvent::AttackPlayer(..)) {
                mob.push(t);
            }
        }
        let hits = h.world.resource::<PlayerState>().hits.clone();
        for x in &hits[seen..] {
            ecs.push(x.tick as i64 - t0);
        }
        seen = hits.len();
        slept |= h.world.query_filtered::<(), bevy_ecs::prelude::With<bevy_ecs::entity_disabling::Disabled>>().iter(&h.world).count() > 0;
    }
    (mob, ecs, slept)
}

/// A zombie asleep for 707 ticks strikes again as the `Mob` that was not ticked does. Its melee check is in the WORLD's time (vanilla's
/// `lastCanUseCheck` against `getGameTime`, which the `Mob` keeps): the `Mob` asks at once when it wakes, the ecs zombie on its
/// grid, at most 20 ticks later (aligned at the wake); the cadence after is the same, and its other deadlines (sun draw, fire,
/// cooldown counters) stand still with its own time.
#[test]
fn a_zombie_that_sleeps_strikes_again_as_a_mob_does() {
    if horizon_ecs_proto::cow::skip_unless_default_species("a_zombie_that_sleeps_strikes_again_as_a_mob_does") {
        return;
    }
    if !have_data() {
        return;
    }
    let _ = pair();
    let s2 = 300;
    let mut worst = 0;
    // Asleep while hunting (the target taken, the melee goal not yet running, the zombies still on their way), for several lengths
    // and several moments, so that the grid of the check falls at every offset from the wake.
    for (s1, gap) in [(10, 707), (14, 713), (18, 721), (24, 729), (30, 737), (40, 743), (80, 707)] {
        let (_, e0, slept0) = zombie_hits(0, s1, s2);
        assert!(!slept0);
        let (mg, eg, slept) = zombie_hits(gap, s1, s2);
        assert!(slept, "the zombies sleep in the gap");
        let first_after = |v: &[i64]| v.iter().copied().find(|&t| t > s1 + gap);
        let (mf, ef) = (first_after(&mg).expect("the mob strikes again"), first_after(&eg).expect("the ecs zombies strike again"));
        eprintln!("gap {gap}: first strike after the wake: mob +{}, ecs +{}", mf - s1 - gap, ef - s1 - gap);
        worst = worst.max((ef - mf).abs());
        // The strikes before the sleep are those of a herd that never slept.
        assert_eq!(eg.iter().filter(|&&t| t <= s1).count(), e0.iter().filter(|&&t| t <= s1).count());
        let after = |v: &[i64]| v.iter().filter(|&&t| t > s1 + gap).count() as f64;
        let (ma, ea) = (after(&mg), after(&eg));
        assert!((ea / ma - 1.0).abs() < 0.25, "strikes after the wake: mob {ma}, ecs {ea}");
    }
    assert!(worst <= 3, "first strike after the wake: the ecs zombies differ from the mob's by {worst} ticks");
}

/// At the border of the zone a creature asleep is still an obstacle: an awake one that overlaps it is pushed away, as the
/// `Mob` beside a mob that is not ticked is (`Entity.push` moves both; the sleeper does not move, its velocity changes).
#[test]
fn a_sleeping_neighbour_still_pushes_an_awake_creature() {
    if horizon_ecs_proto::cow::skip_unless_small_body("a_sleeping_neighbour_still_pushes_an_awake_creature") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("a_sleeping_neighbour_still_pushes_an_awake_creature") {
        return;
    }
    if !have_data() {
        return;
    }
    let (mut r, mut h) = pair();
    // The player's chunk is 0: with a radius of 2, x < 48 ticks and x >= 48 does not.
    h.set_players(vec![(8.0, 8.0)]);
    h.set_simulation(2);
    h.tick();
    // The two boxes overlap by 25 % of the width, whatever the species.
    let w = f64::from(species().width);
    let (ax, bx, z) = (48.0 - 0.375 * w, 48.0 + 0.375 * w, 8.5);
    let (ia, ib) = (r.add(Vec3::new(ax, 1.0, z)), r.add(Vec3::new(bx, 1.0, z)));
    let (ea, eb) = (ecs_cow(&mut h, ax, 1.0, z, 3), ecs_cow(&mut h, bx, 1.0, z, 4));
    let lv = oracle(&[(8.0, 8.0)], 2, 2);
    h.tick();
    let mut asleep = false;
    for _ in 0..40 {
        r.step_where(None, |m| lv.entity_ticking_at(m.position.x.floor() as i32, m.position.z.floor() as i32));
        h.tick();
        asleep |= h.world.get::<Dormant>(eb).is_some();
    }
    assert!(asleep, "the neighbour at x = 48.3 sleeps");
    let (ma, mb) = (r.mobs[ia].position, r.mobs[ib].position);
    let (ca, cb) = (h.world.get::<Pos>(ea).unwrap().0, h.world.get::<Pos>(eb).unwrap().0);
    eprintln!("awake: mob x {:.3}, ecs x {:.3}; asleep: mob x {:.3}, ecs x {:.3}", ma.x, ca.x, mb.x, cb.x);
    assert_eq!(mb.x, bx, "the mob that is not ticked does not move");
    assert_eq!(cb.x, bx, "the creature asleep does not move");
    assert!(ma.x < ax - 0.02 && ca.x < ax - 0.02, "both awake creatures are pushed away from the sleeper");
    // (The push law agrees with the `Mob` to 0.04-0.25 block on a few ticks of overlap: `suivi/ecs-ecarts.md`.)
    assert!((ma.x - ca.x).abs() < 0.2, "mob {:.3} ecs {:.3}", ma.x, ca.x);
}

/// A creature created in a chunk that does not tick is born asleep, with no entry in the wheel; one in a chunk that ticks is not.
#[test]
fn a_creature_created_where_nothing_ticks_is_born_asleep() {
    if horizon_ecs_proto::cow::skip_unless_walker("a_creature_created_where_nothing_ticks_is_born_asleep") {
        return;
    }
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(HALF, true, false);
    h.set_players(vec![(8.0, 8.0)]);
    h.set_simulation(2);
    h.tick();
    let made = h.populate(&[(10.5, 10.5), (-90.5, 40.5), (70.5, 8.5)], 9);
    let asleep: Vec<bool> = made.iter().map(|&e| h.world.get::<Dormant>(e).is_some()).collect();
    assert_eq!(asleep, [false, true, true]);
    let wheel = h.world.resource::<Wheel>().len();
    // Only the first creature has entries.
    let own = h.world.resource::<Wheel>().owned_by(made[0]);
    assert!(own > 0 && wheel == own, "wheel {wheel}, the awake creature's {own}");
    for _ in 0..300 {
        h.tick();
    }
    assert_eq!(h.world.get::<Pos>(made[1]).unwrap().0.x, -90.5);
    // The player comes: they wake and run.
    h.set_players(vec![(-90.0, 40.0)]);
    h.tick();
    assert!(h.world.get::<Dormant>(made[1]).is_none());
    assert!(h.world.resource::<Wheel>().owned_by(made[1]) > 0);
}
