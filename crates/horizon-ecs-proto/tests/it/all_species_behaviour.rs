//! Every species of the table against a real `Mob` in the same flat scene, over seeds 1 to 8, alone and with a player 4 blocks away.
//! Each side plays on its own for `TICKS` ticks or until it dies. No tick is compared: each run gives behaviour measures (path, speed,
//! hops, turns, target, end state), and a measure is `differs` when the two ranges over the seeds are disjoint and the means apart by
//! more than 25 %. `head_turn` and `body_turn` alone are `lod`: the ECS does not look without a spectator (`LazyLook`).
//! The journal (`BEHAVIOUR_LOG`, TSV) has one line per measure, a panic one line per seed. The test asserts nothing on the verdicts.
//! `BENCH_SPECIES` (comma-separated, with or without `minecraft:`) plays only those species.
use crate::all_species_vs_mob::{pool, read_ecs, read_mob, Row, ALIVE};
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, goals::*};
use std::{fs::File, io::Write, panic::AssertUnwindSafe, path::PathBuf};

const TICKS: usize = 400;
const SEEDS: [u64; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
const SCENARIOS: [(&str, bool); 2] = [("alone", false), ("player", true)];
/// The player's horizontal spot, 4 blocks from the mob (the same as in `all_species_vs_mob`).
const PLAYER: (f64, f64) = (4.5, 0.5);
const MOVING: f64 = 0.001;
const X: usize = 0;
const Y: usize = 1;
const Z: usize = 2;
const VX: usize = 3;
const VY: usize = 4;
const VZ: usize = 5;
const BODY: usize = 6;
const HEALTH: usize = 7;
const TARGET: usize = 9;
const HEAD: usize = 11;
/// The measures of one side of one run, in a fixed order; `None` is a seed without the event (`target_first`).
type Measures = Vec<(&'static str, Option<f64>)>;

/// The shortest distance between two angles in degrees.
fn wrap(d: f64) -> f64 {
    let r = d.rem_euclid(360.0);
    r.min(360.0 - r)
}

/// The behaviour measures of one side, from its rows: the first is the start, the others are the ticks.
fn measures(rows: &[Row], with_player: bool) -> Measures {
    let (first, last) = (rows[0], rows[rows.len() - 1]);
    let ticks = &rows[1..];
    let n = ticks.len() as f64;
    let speed: Vec<f64> = ticks.iter().map(|r| r[VX].hypot(r[VZ])).collect();
    let moving: Vec<f64> = speed.iter().copied().filter(|s| *s > MOVING).collect();
    let mean_moving = if moving.is_empty() { 0.0 } else { moving.iter().sum::<f64>() / moving.len() as f64 };
    let (lo, hi) = rows.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), r| (lo.min(r[Y]), hi.max(r[Y])));
    let turn = |i: usize| rows.windows(2).map(|w| wrap(w[1][i] - w[0][i])).sum::<f64>();
    let hdist = |r: &Row| (r[X] - PLAYER.0).hypot(r[Z] - PLAYER.1);
    let mut out: Measures = vec![
        ("path", Some(rows.windows(2).map(|w| (w[1][X] - w[0][X]).hypot(w[1][Z] - w[0][Z])).sum())),
        ("net", Some((last[X] - first[X]).hypot(last[Z] - first[Z]))),
        ("moving", Some(moving.len() as f64 / n)),
        ("speed", Some(mean_moving)),
        ("top_speed", Some(speed.iter().copied().fold(0.0, f64::max))),
        ("y_range", Some(hi - lo)),
        ("hops", Some(rows.windows(2).filter(|w| w[0][VY] <= 0.0 && w[1][VY] > 0.1).count() as f64)),
        ("head_turn", Some(turn(HEAD))),
        ("body_turn", Some(turn(BODY))),
        ("target_ever", Some(f64::from(u8::from(ticks.iter().any(|r| r[TARGET] == 1.0))))),
        ("target_ticks", Some(ticks.iter().filter(|r| r[TARGET] == 1.0).count() as f64 / n)),
        ("target_first", ticks.iter().position(|r| r[TARGET] == 1.0).map(|i| (i + 1) as f64)),
        ("health_end", Some(last[HEALTH])),
        ("alive_end", Some(last[ALIVE])),
    ];
    if with_player {
        out.push(("player_dist_end", Some(hdist(&last))));
        out.push(("player_dist_min", Some(rows.iter().map(hdist).fold(f64::INFINITY, f64::min))));
    }
    out
}

/// One run of `sp` in a fresh flat scene: the rows of the `Mob` and of the ECS, each side stepped only while it lives.
fn run(sp: &Species, with_player: bool, seed: u64) -> [Vec<Row>; 2] {
    let (mut rf, mut h) = flat_pair();
    let i = rf.add_kind_id(Vec3::new(0.5, 1.0, 0.5), mob_kind(sp.name), seed as u32);
    let e = spawn_species(&mut h.world, sp, 0.5, 0.5, seed);
    if let Some(mut yaw) = h.world.get_mut::<Yaw>(e) {
        yaw.0 = 0.0;
    }
    // A player within 32 blocks keeps the animal active without end (the ring); alone, it keeps its spawn deadline (`noActionTime` counts up).
    if let Some(mut until) = h.world.get_mut::<ActiveUntil>(e).filter(|_| with_player) {
        until.0 = u64::MAX;
    }
    h.world.resource_mut::<Entered>().0.push(e);
    // The first stroll attempt is armed as `Herd::populate` arms it (a spawn alone does not arm it).
    if h.world.get::<Stroll>(e).is_some() {
        let at = now_of(&h.world, e) + 1 + seed % 120;
        arm_at(&mut h.world, e, at, Due::Stroll);
        h.world.get_mut::<Stroll>(e).unwrap().scheduled = true;
    }
    pool(sp, |x, y, z, n| rf.set_block(x, y, z, n));
    pool(sp, |x, y, z, n| h.set_block(x, y, z, n));
    let view = with_player.then(|| player_view(Vec3::new(4.5, 1.0, 0.5), None));
    if with_player {
        h.set_online(&[PLAYER]);
    }
    let mut rows = [vec![read_mob(&rf.mobs[i])], vec![read_ecs(&h.world, e)]];
    let (mut mob_on, mut ecs_on) = (true, true);
    for _ in 0..TICKS {
        if mob_on {
            rf.step(view.as_ref());
            rows[0].push(read_mob(&rf.mobs[i]));
            mob_on = rows[0].last().is_some_and(|r| r[ALIVE] == 1.0);
        }
        if ecs_on {
            h.tick();
            rows[1].push(read_ecs(&h.world, e));
            ecs_on = rows[1].last().is_some_and(|r| r[ALIVE] == 1.0);
        }
        if !mob_on && !ecs_on {
            break;
        }
    }
    rows
}

fn put(log: &mut File, line: &str) {
    writeln!(log, "{line}").expect("the behaviour log cannot be written");
}

/// Min, max and mean of the values of a side over the seeds; `None` without any.
fn stats(v: &[f64]) -> Option<(f64, f64, f64)> {
    if v.is_empty() {
        return None;
    }
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    Some((mean, v.iter().copied().fold(f64::INFINITY, f64::min), v.iter().copied().fold(f64::NEG_INFINITY, f64::max)))
}

fn cols(s: Option<(f64, f64, f64)>) -> String {
    match s {
        Some((mean, lo, hi)) => format!("{mean}\t{lo}\t{hi}"),
        None => "-\t-\t-".to_string(),
    }
}

/// `differs` when the ranges are disjoint and the means apart by more than 25 % of the larger (and 0.01); `by_count` adds the
/// rule of 4 seeds of difference in the number of seeds that have the value.
fn differs(mob: &[f64], ecs: &[f64], by_count: bool) -> bool {
    let count_gap = by_count && (mob.len() as i64 - ecs.len() as i64).abs() >= 4;
    let (Some(a), Some(b)) = (stats(mob), stats(ecs)) else {
        return count_gap;
    };
    let gap = (a.0 - b.0).abs();
    let disjoint = a.2 < b.1 || b.2 < a.1;
    count_gap || (disjoint && gap > 0.25 * a.0.abs().max(b.0.abs()) && gap > 0.01)
}

#[test]
fn all_species_behaviour() {
    if !have_data() {
        return;
    }
    // The first world sets the species table and the `MobType`s of the process.
    let _ = flat_pair();
    let table = species_table().0;
    let only: Vec<String> = std::env::var("BENCH_SPECIES")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| if s.contains(':') { s.to_string() } else { format!("minecraft:{s}") })
        .collect();
    let path = std::env::var_os("BEHAVIOUR_LOG").map_or_else(|| std::env::temp_dir().join("all-species-behaviour.tsv"), PathBuf::from);
    let mut log = File::create(&path).unwrap_or_else(|e| panic!("the behaviour log {}: {e}", path.display()));
    put(&mut log, "species\tscenario\tmetric\tverdict\tmob_mean\tmob_min\tmob_max\tecs_mean\tecs_min\tecs_max");
    let (mut same, mut differing, mut no_reference, mut panics) = (0, 0, 0, 0);
    let mut listed = Vec::new();
    for sp in table.iter().filter(|sp| only.is_empty() || only.iter().any(|n| n == sp.name)) {
        if !has_mob_kind(sp.name) {
            put(&mut log, &format!("{}\t-\t-\tno-reference", sp.name));
            no_reference += 1;
            continue;
        }
        let (mut bad, mut panicked) = (Vec::new(), false);
        for (scenario, with_player) in SCENARIOS {
            // Per side, one list of measures per seed that did not panic.
            let mut runs: [Vec<Measures>; 2] = [Vec::new(), Vec::new()];
            for seed in SEEDS {
                match std::panic::catch_unwind(AssertUnwindSafe(|| run(sp, with_player, seed))) {
                    Err(p) => {
                        let msg = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_default();
                        put(&mut log, &format!("{}\t{scenario}\tseed {seed}\tpanic\t{}", sp.name, msg.replace(['\t', '\n'], " ")));
                        panicked = true;
                    }
                    Ok(sides) => {
                        for (side, rows) in sides.iter().enumerate() {
                            runs[side].push(measures(rows, with_player));
                        }
                    }
                }
            }
            let Some(first) = runs[0].first() else {
                continue;
            };
            for m in 0..first.len() {
                let name = first[m].0;
                let vals = |side: usize| -> Vec<f64> { runs[side].iter().filter_map(|ms| ms[m].1).collect() };
                let (mob, ecs) = (vals(0), vals(1));
                let verdict = if scenario == "alone" && (name == "head_turn" || name == "body_turn") {
                    "lod"
                } else if differs(&mob, &ecs, name == "target_first") {
                    bad.push(format!("{scenario}/{name}"));
                    "differs"
                } else {
                    "same"
                };
                put(&mut log, &format!("{}\t{scenario}\t{name}\t{verdict}\t{}\t{}", sp.name, cols(stats(&mob)), cols(stats(&ecs))));
            }
        }
        if !bad.is_empty() {
            differing += 1;
            listed.push(format!("{}: {}", sp.name, bad.join(", ")));
        }
        if panicked {
            panics += 1;
        }
        if bad.is_empty() && !panicked {
            same += 1;
        }
    }
    println!("species={} same={same} differs={differing} no_reference={no_reference} panic={panics}", table.len());
    for line in &listed {
        println!("differs {line}");
    }
}
