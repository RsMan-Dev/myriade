//! Every species of the table, in one process, against a real `Mob` in the same flat scene: 200 ticks, seeds 1 to 3, alone and with a
//! player standing 4 blocks away (the same `PlayerView` on the `Mob` side, the same `Player` in `Online` on the ECS side). Nothing stops
//! at the first gap: each (species, scenario, seed, field) that differs is a line of the journal (`DIVERGENCE_LOG`, TSV), a clean triple
//! is `ok`, a species without its own `MobType` is `no-reference`, a panic is `panic`. The test asserts nothing on the gaps.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::Mob;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{bridge::host_read, cow::*, goals::*, targeting::Target};
use std::{fs::File, io::Write, panic::AssertUnwindSafe, path::PathBuf};

const TICKS: usize = 200;
const SEEDS: [u64; 3] = [1, 2, 3];
const SCENARIOS: [(&str, bool); 2] = [("alone", false), ("player", true)];
const FIELDS: [&str; 10] = ["x", "y", "z", "vx", "vy", "vz", "body_yaw", "health", "alive", "target"];

fn flag(b: bool) -> f64 {
    f64::from(u8::from(b))
}

/// The fields of a `Mob`, in `FIELDS` order.
fn read_mob(m: &Mob) -> [f64; 10] {
    let (p, v) = (m.position, m.velocity);
    [p.x, p.y, p.z, v.x, v.y, v.z, f64::from(m.body_yaw), f64::from(mob_health(m)), flag(m.health > 0.0 && !m.removed), flag(m.ai.target.is_some())]
}

/// The same fields of an entity: a missing component reads NaN (a gap on that side), a sleeping or gone entity reads as dead.
fn read_ecs(w: &World, e: Entity) -> [f64; 10] {
    let p = w.get::<Pos>(e).map_or([f64::NAN; 3], |p| [p.0.x, p.0.y, p.0.z]);
    let v = w.get::<Vel>(e).map_or([f64::NAN; 3], |v| [v.0.x, v.0.y, v.0.z]);
    let body = host_read::head_body_yaw(w, e).map_or(f64::NAN, |(_, b)| f64::from(b));
    let health = w.get::<Health>(e).map_or(0.0, |h| f64::from(h.0));
    [p[0], p[1], p[2], v[0], v[1], v[2], body, health, flag(host_read::alive(w, e)), flag(w.get::<Target>(e).is_some())]
}

/// The first gap of a field, how many ticks it differs and the widest gap (a missing side is infinite).
#[derive(Clone, Copy, Default)]
struct Gap {
    first: Option<(usize, f64, f64)>,
    ticks: usize,
    max: f64,
}

impl Gap {
    fn observe(&mut self, t: usize, mob: f64, ecs: f64) {
        if mob.is_nan() && ecs.is_nan() {
            return;
        }
        let gap = if (mob - ecs).is_nan() { f64::INFINITY } else { (mob - ecs).abs() };
        if gap <= 1e-9 {
            return;
        }
        self.first.get_or_insert((t, mob, ecs));
        self.ticks += 1;
        self.max = self.max.max(gap);
    }
}

/// One run: the `Mob` and the ECS of `sp` in a fresh flat scene, `TICKS` ticks, the gaps of each field.
fn run(sp: &Species, with_player: bool, seed: u64) -> [Gap; 10] {
    let (mut rf, mut h) = flat_pair();
    let i = rf.add_kind_id(Vec3::new(0.5, 1.0, 0.5), mob_kind(sp.name), seed as u32);
    let e = spawn_species(&mut h.world, sp, 0.5, 0.5, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    let view = with_player.then(|| player_view(Vec3::new(4.5, 1.0, 0.5), None));
    if with_player {
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 4.5, y: 1.0, z: 0.5, ..Default::default() });
    }
    let mut gaps = [Gap::default(); 10];
    for t in 1..=TICKS {
        rf.step(view.as_ref());
        h.tick();
        let (m, c) = (read_mob(&rf.mobs[i]), read_ecs(&h.world, e));
        for (f, gap) in gaps.iter_mut().enumerate() {
            gap.observe(t, m[f], c[f]);
        }
    }
    gaps
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
    let path = std::env::var_os("DIVERGENCE_LOG").map_or_else(|| std::env::temp_dir().join("all-species-divergences.tsv"), PathBuf::from);
    let mut log = File::create(&path).unwrap_or_else(|e| panic!("the divergence log {}: {e}", path.display()));
    let (mut ok, mut divergent, mut no_reference, mut panics) = (0, 0, 0, 0);
    for sp in &table {
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
                    Ok(gaps) => {
                        let mut clean = true;
                        for (f, g) in gaps.iter().enumerate() {
                            if let Some((t, m, c)) = g.first {
                                clean = false;
                                put(&mut log, &format!("{head}\t{}\t{t}\t{m}\t{c}\t{}\t{}", FIELDS[f], g.ticks, g.max));
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
