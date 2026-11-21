//! The creaking by configuration (`species/creaking.rs`, `stare_freeze`, the generic combat) against real `Mob` creakings
//! (`HORIZON_TEST_RESOURCES` set, process species `minecraft:creaking`). One creaking per run, `N` runs with ids far apart, the
//! player's gaze scripted tick by tick: roused by a stare, still while looked at, hunting while not, one blow every 40 ticks.
//! The `Mob` asks the stare every tick and reads its player from a sensor scanned every 20 ticks; the ECS asks it by event.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{creaking, MobEvent};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{behaviours::stare_freeze, cow::*, goals::*, hostile::PlayerState};

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:creaking"])
}
const N: usize = 8;
fn far_id(j: usize) -> u32 {
    1 + (j as u32 + 1) * 7919 % 100_003
}
/// What a run records, a row per tick.
#[derive(Default)]
struct Trace {
    roused: Vec<bool>,
    still: Vec<bool>,
    at: Vec<(f64, f64)>,
    hits: Vec<i64>,
}
/// The view vector from the player's eyes at `from` to the creaking's middle at `to`, or away from it.
fn gaze(from: Vec3, to: Vec3, at: bool) -> [f64; 3] {
    let d = [to.x - from.x, to.y + 0.9 - (from.y + 1.62), to.z - from.z];
    let n = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let s = if at { 1.0 } else { -1.0 };
    [s * d[0] / n, s * d[1] / n, s * d[2] / n]
}
/// `looks(t)`: whether the player looks at the creaking at tick t.
fn mob_run(j: usize, start: Vec3, player: Vec3, ticks: i64, looks: &dyn Fn(i64) -> bool) -> Trace {
    let mut r = MobRef::new(60, false);
    let i = r.add_id(start, far_id(j));
    let mut tr = Trace::default();
    for t in 1..=ticks {
        let mut p = player_view(player, None);
        p.look = gaze(player, start, looks(t));
        let from = r.events.len();
        r.step(Some(&p));
        let m = &r.mobs[i];
        tr.roused.push(creaking::is_active(m));
        tr.still.push(!creaking::can_move(m));
        tr.at.push((m.position.x, m.position.z));
        tr.hits.extend(r.events[from..].iter().filter(|e| matches!(e, MobEvent::AttackPlayer(..))).map(|_| t));
    }
    tr
}
fn ecs_run(j: usize, start: Vec3, player: Vec3, ticks: i64, looks: &dyn Fn(i64) -> bool) -> Trace {
    let mut h = Herd::with_slab_awake(60, true, false);
    h.world.resource_mut::<PlayerState>().immortal = true;
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:creaking").unwrap();
    let e = spawn_species(&mut h.world, &sp, start.x, start.z, ((j as u64) + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    let mut tr = Trace::default();
    let mut seen = 0;
    for t in 1..=ticks {
        h.world.resource_mut::<Online>().set(Player { id: 1, x: player.x, y: player.y, z: player.z, main: 0, off: 0, ..Default::default() });
        h.world.resource_mut::<PlayerState>().set_gaze(1, gaze(player, start, looks(t)));
        h.tick();
        tr.roused.push(stare_freeze::is_roused(&h.world, e));
        tr.still.push(!stare_freeze::can_move(&h.world, e));
        let p = h.world.get::<Pos>(e).unwrap().0;
        tr.at.push((p.x, p.z));
        let hits = h.world.resource::<PlayerState>().hits.clone();
        tr.hits.extend(hits[seen..].iter().map(|_| t));
        seen = hits.len();
    }
    tr
}
fn first(v: &[bool]) -> Option<i64> {
    v.iter().position(|&b| b).map(|i| i as i64 + 1)
}
fn dist(a: (f64, f64), b: Vec3) -> f64 {
    ((a.0 - b.x).powi(2) + (a.1 - b.z).powi(2)).sqrt()
}
fn moved(tr: &Trace, from: usize, to: usize) -> f64 {
    let (a, b) = (tr.at[from - 1], tr.at[to - 1]);
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}
fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len().max(1) as f64
}

/// Stare from 8 blocks for 40 ticks, then look away until 100, then stare again: roused, still, then hunting, then still.
#[test]
fn a_stare_rouses_and_stills_it_and_looking_away_frees_it_as_a_mob_does() {
    if skip() {
        return;
    }
    let (start, player) = (Vec3::new(0.5, 1.0, 0.5), Vec3::new(0.5, 1.0, 8.5));
    let looks = |t: i64| !(41..=100).contains(&t);
    let (mut rows, mut cols) = (Vec::new(), Vec::new());
    for j in 0..N {
        let (m, e) = (mob_run(j, start, player, 150, &looks), ecs_run(j, start, player, 150, &looks));
        rows.push(m);
        cols.push(e);
    }
    let col = |f: &dyn Fn(&Trace) -> f64, v: &[Trace]| mean(&v.iter().map(f).collect::<Vec<_>>());
    for (name, f) in [
        ("roused at", &(|t: &Trace| first(&t.roused).unwrap_or(-1) as f64) as &dyn Fn(&Trace) -> f64),
        ("still at", &|t: &Trace| first(&t.still).unwrap_or(-1) as f64),
        ("displacement 1..40 (stared)", &|t: &Trace| moved(t, 1, 40)),
        ("distance to the player at 70", &|t: &Trace| dist(t.at[69], player)),
        ("distance to the player at 100", &|t: &Trace| dist(t.at[99], player)),
        ("still at 105", &|t: &Trace| f64::from(t.still[104])),
        ("displacement 110..150 (stared)", &|t: &Trace| moved(t, 110, 150)),
    ] {
        eprintln!("{name}: mob {:.2} ecs {:.2} ({N} runs)", col(f, &rows), col(f, &cols));
    }
    for t in &cols {
        assert!(first(&t.roused).is_some_and(|x| x <= 3), "the stare rouses it at once: {:?}", first(&t.roused));
        assert!(t.still[2] && moved(t, 1, 40) < 0.01, "looked at, it stands still: {}", moved(t, 1, 40));
        assert!(!t.still[60] && dist(t.at[99], player) < dist(t.at[40], player) - 2.0, "looked away from, it walks to the player");
        assert!(t.still[119] && moved(t, 110, 150) < 0.01, "looked at again, it stands still");
    }
    for t in &rows {
        assert!(first(&t.roused).is_some(), "the Mob creaking is roused");
    }
}

/// A creaking at the player's side strikes every 40 ticks while he looks away and not while he looks at it.
#[test]
fn it_strikes_every_forty_ticks_and_never_while_looked_at() {
    if skip() {
        return;
    }
    let (start, player) = (Vec3::new(0.5, 1.0, 0.5), Vec3::new(0.5, 1.0, 2.5));
    let looks = |t: i64| t <= 10 || (131..=170).contains(&t);
    let (mut m_hits, mut e_hits) = (Vec::new(), Vec::new());
    for j in 0..N {
        let (m, e) = (mob_run(j, start, player, 260, &looks), ecs_run(j, start, player, 260, &looks));
        eprintln!("run {j}: mob hits {:?} | ecs hits {:?}", m.hits, e.hits);
        m_hits.push(m.hits);
        e_hits.push(e.hits);
    }
    let gaps = |v: &[Vec<i64>]| v.iter().flat_map(|h| h.windows(2).map(|w| (w[1] - w[0]) as f64)).collect::<Vec<_>>();
    let (mg, eg) = (gaps(&m_hits), gaps(&e_hits));
    let count = |v: &[Vec<i64>]| v.iter().map(Vec::len).sum::<usize>() as f64 / N as f64;
    eprintln!("hits per run: mob {:.2} ecs {:.2}; gap: mob {:.1} (min {:.0}) ecs {:.1} (min {:.0})", count(&m_hits), count(&e_hits), mean(&mg), mg.iter().cloned().fold(f64::MAX, f64::min), mean(&eg), eg.iter().cloned().fold(f64::MAX, f64::min));
    for h in &e_hits {
        assert!(h.iter().all(|&t| !(131..=170).contains(&t) && t > 10), "no blow while looked at: {h:?}");
        assert!(h.len() >= 3, "it strikes while the player looks away: {h:?}");
    }
    assert!(eg.iter().all(|&g| g >= 40.0), "the cooldown is 40: {eg:?}");
}
