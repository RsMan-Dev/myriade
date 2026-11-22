//! Every species of the table, in one process, against a real `Mob` in the same flat scene: 200 ticks, seeds 1 to 3, alone and with a
//! player standing 4 blocks away (the same `PlayerView` on the `Mob` side, the same `Player` in `Online` on the ECS side). Nothing stops
//! at the first gap: each (species, scenario, seed, field) that differs is a line of the journal (`DIVERGENCE_LOG`, TSV), a clean triple
//! is `ok`, a species without its own `MobType` is `no-reference`, a panic is `panic`. The test asserts nothing on the gaps.
//! `BENCH_SPECIES` (comma-separated, with or without `minecraft:`) plays only those species; `BENCH_TRACE=1` adds the 5 ticks around each
//! first gap as `trace` lines.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::Mob;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{bridge::host_read, cow::*, goals::*, targeting::Target};
use std::{fs::File, io::Write, panic::AssertUnwindSafe, path::PathBuf};

const TICKS: usize = 200;
const SEEDS: [u64; 3] = [1, 2, 3];
const SCENARIOS: [(&str, bool); 2] = [("alone", false), ("player", true)];
const FIELDS: [&str; 12] = ["x", "y", "z", "vx", "vy", "vz", "body_yaw", "health", "alive", "target", "yaw", "head_yaw"];
const FIELD_COUNT: usize = FIELDS.len();
pub(super) const ALIVE: usize = 8;
/// The first angle field: `yaw` and `head_yaw` (the last two) are compared modulo 360.
const YAW: usize = 10;
pub(super) type Row = [f64; FIELD_COUNT];

fn flag(b: bool) -> f64 {
    f64::from(u8::from(b))
}

/// The fields of a `Mob`, in `FIELDS` order.
pub(super) fn read_mob(m: &Mob) -> Row {
    let (p, v) = (m.position, m.velocity);
    [
        p.x,
        p.y,
        p.z,
        v.x,
        v.y,
        v.z,
        f64::from(m.body_yaw),
        f64::from(mob_health(m)),
        flag(m.health > 0.0 && !m.removed),
        flag(m.ai.target.is_some()),
        f64::from(m.yaw),
        f64::from(m.head_yaw),
    ]
}

/// The same fields of an entity: a missing component reads NaN (a gap on that side), a sleeping or gone entity reads as dead.
pub(super) fn read_ecs(w: &World, e: Entity) -> Row {
    let p = w.get::<Pos>(e).map_or([f64::NAN; 3], |p| [p.0.x, p.0.y, p.0.z]);
    let v = w.get::<Vel>(e).map_or([f64::NAN; 3], |v| [v.0.x, v.0.y, v.0.z]);
    let head = host_read::head_body_yaw(w, e);
    let health = w.get::<Health>(e).map_or(0.0, |h| f64::from(h.0));
    let yaw = w.get::<Yaw>(e).map_or(f64::NAN, |y| f64::from(y.0));
    [
        p[0],
        p[1],
        p[2],
        v[0],
        v[1],
        v[2],
        head.filter(|(y, _)| !y.is_nan()).map_or(yaw, |(_, b)| f64::from(b)),
        health,
        flag(host_read::alive(w, e)),
        flag(w.get::<Target>(e).is_some()),
        yaw,
        head.filter(|(y, _)| !y.is_nan()).map_or(yaw, |(y, _)| f64::from(y)),
    ]
}

/// The first gap of a field, how many ticks it differs and the widest gap (a missing side is infinite, an angle wraps at 360).
#[derive(Clone, Copy, Default)]
struct Gap {
    first: Option<(usize, f64, f64)>,
    ticks: usize,
    max: f64,
}

impl Gap {
    fn observe(&mut self, t: usize, mob: f64, ecs: f64, angle: bool) {
        if mob.is_nan() && ecs.is_nan() {
            return;
        }
        let diff = (mob - ecs).abs();
        let gap = if diff.is_nan() {
            f64::INFINITY
        } else if angle {
            let r = diff.rem_euclid(360.0);
            r.min(360.0 - r)
        } else {
            diff
        };
        if gap <= 1e-9 {
            return;
        }
        self.first.get_or_insert((t, mob, ecs));
        self.ticks += 1;
        self.max = self.max.max(gap);
    }
}

/// The pool `pool_if_aquatic` digs for a species that breathes under water, given to `set` (the `Mob` world or the ECS one).
pub(super) fn pool(sp: &Species, mut set: impl FnMut(i32, i32, i32, &'static str)) {
    use horizon_ecs_proto::rules::locomotion::Medium;
    // A swimmer with no ground mode: the others stand on the floor.
    if sp.modes.iter().any(|(m, _)| *m == Medium::Ground) || !sp.modes.iter().any(|(m, _)| *m == Medium::Water) {
        return;
    }
    for dx in -2..=2 {
        for dz in -3..=3 {
            for y in 1..=2 {
                set(dx, y, dz, "minecraft:water");
            }
        }
    }
}

/// One run: the `Mob` and the ECS of `sp` in a fresh flat scene, `TICKS` ticks, the gaps of each field and the rows of every tick.
/// Once one side is dead the alive gap is the only line; once both are, the run stops.
fn run(sp: &Species, with_player: bool, seed: u64) -> ([Gap; FIELD_COUNT], Vec<(usize, Row, Row)>) {
    let (mut rf, mut h) = flat_pair();
    let i = rf.add_kind_id(Vec3::new(0.5, 1.0, 0.5), mob_kind(sp.name), seed as u32);
    let e = spawn_species(&mut h.world, sp, 0.5, 0.5, seed);
    // The `Mob` is laid facing 0: the ECS draws its yaw from the seed.
    if let Some(mut yaw) = h.world.get_mut::<Yaw>(e) {
        yaw.0 = 0.0;
    }
    if let Some(mut until) = h.world.get_mut::<ActiveUntil>(e) {
        until.0 = u64::MAX;
    }
    h.world.resource_mut::<Entered>().0.push(e);
    pool(sp, |x, y, z, n| rf.set_block(x, y, z, n));
    pool(sp, |x, y, z, n| h.set_block(x, y, z, n));
    let view = with_player.then(|| player_view(Vec3::new(4.5, 1.0, 0.5), None));
    if with_player {
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 4.5, y: 1.0, z: 0.5, ..Default::default() });
    }
    let mut gaps = [Gap::default(); FIELD_COUNT];
    let mut rows = Vec::new();
    for t in 1..=TICKS {
        rf.step(view.as_ref());
        h.tick();
        let (m, c) = (read_mob(&rf.mobs[i]), read_ecs(&h.world, e));
        rows.push((t, m, c));
        let (mob_alive, ecs_alive) = (m[ALIVE] == 1.0, c[ALIVE] == 1.0);
        if !mob_alive && !ecs_alive {
            break;
        }
        if mob_alive != ecs_alive {
            gaps[ALIVE].observe(t, m[ALIVE], c[ALIVE], false);
            break;
        }
        for (f, gap) in gaps.iter_mut().enumerate() {
            gap.observe(t, m[f], c[f], f >= YAW);
        }
    }
    (gaps, rows)
}

fn put(log: &mut File, line: &str) {
    writeln!(log, "{line}").expect("the divergence log cannot be written");
}

#[test]
fn all_species_vs_mob() {
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
    let trace = std::env::var("BENCH_TRACE").is_ok_and(|v| v == "1");
    let path = std::env::var_os("DIVERGENCE_LOG").map_or_else(|| std::env::temp_dir().join("all-species-divergences.tsv"), PathBuf::from);
    let mut log = File::create(&path).unwrap_or_else(|e| panic!("the divergence log {}: {e}", path.display()));
    let (mut ok, mut divergent, mut no_reference, mut panics) = (0, 0, 0, 0);
    for sp in table.iter().filter(|sp| only.is_empty() || only.iter().any(|n| n == sp.name)) {
        for (scenario, with_player) in SCENARIOS {
            for seed in SEEDS {
                let head = format!("{}\t{scenario}\t{seed}", sp.name);
                if !has_mob_kind(sp.name) {
                    put(&mut log, &format!("{head}\tno-reference"));
                    no_reference += 1;
                    continue;
                }
                match std::panic::catch_unwind(AssertUnwindSafe(|| run(sp, with_player, seed))) {
                    Err(p) => {
                        let msg = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_default();
                        put(&mut log, &format!("{head}\tpanic\t{}", msg.replace(['\t', '\n'], " ")));
                        panics += 1;
                    }
                    Ok((gaps, rows)) => {
                        let mut clean = true;
                        for (f, g) in gaps.iter().enumerate() {
                            if let Some((t, m, c)) = g.first {
                                clean = false;
                                put(&mut log, &format!("{head}\t{}\t{t}\t{m}\t{c}\t{}\t{}", FIELDS[f], g.ticks, g.max));
                                if trace {
                                    for (tt, mr, cr) in rows.iter().filter(|r| r.0 + 2 >= t && r.0 <= t + 2) {
                                        put(&mut log, &format!("trace\t{}\t{scenario}\t{seed}\t{}\t{tt}\t{}\t{}", sp.name, FIELDS[f], mr[f], cr[f]));
                                    }
                                }
                            }
                        }
                        if clean {
                            ok += 1;
                            put(&mut log, &format!("{head}\tok"));
                        } else {
                            divergent += 1;
                        }
                    }
                }
            }
        }
    }
    println!("species={} ok={ok} divergent={divergent} no_reference={no_reference} panic={panics}", table.len());
}
