//! The brain of the copper golem (without its transport between containers) against the real one (`HORIZON_TEST_RESOURCES`
//! set, process species `minecraft:copper_golem`): the cadence of the sensors, the panic after a hit and its length, the
//! stroll while the transport cooldown lasts, and the expiry of a memory to the tick. The `Mob` golem keeps its transport
//! cooldown from its constructor (60 to 99 ticks); with no container around its transport never starts, so the two walk
//! only while that cooldown counts down. The hazard draws are the entities' own: the controls compare means of 30 golems.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{animal_brain, Mob};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{brain::*, cow::*, goals::*};

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:copper_golem"])
}
fn mob_has(m: &Mob, name: &str) -> bool {
    animal_brain::describe(m).is_some_and(|(a, b)| format!("{a};{b}").contains(&format!("{name}=")))
}
fn mean(v: &[Option<i64>]) -> (f64, usize) {
    let n = v.iter().flatten().count();
    (v.iter().flatten().sum::<i64>() as f64 / n.max(1) as f64, n)
}
struct Pair {
    r: MobRef,
    h: Herd,
    ids: Vec<(usize, Entity)>,
}
fn pair(n: usize) -> Pair {
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab_awake(60, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == species().name).unwrap();
    let ids = (0..n)
        .map(|k| {
            let (x, z) = (-50.0 + 12.0 * (k % 9) as f64 + 0.5, -40.0 + 12.0 * (k / 9) as f64 + 0.5);
            let i = r.add_id(Vec3::new(x, 1.0, z), ((k as u32 + 1).wrapping_mul(0x9E37_79B1) >> 4) | 1);
            let e = spawn_species(&mut h.world, &sp, x, z, (k as u64 + 1 + std::env::var("SEED").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0) * 1000).wrapping_mul(0x9E37_79B9_7F4A_7C15));
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
            h.world.resource_mut::<Entered>().0.push(e);
            (i, e)
        })
        .collect();
    Pair { r, h, ids }
}
impl Pair {
    /// The transport cooldown made long on both sides: the golems stroll for as long as the control lasts.
    fn long_cooldown(&mut self, ticks: i32) {
        for &(i, e) in &self.ids {
            animal_brain::set_transport_cooldown(&mut self.r.mobs[i], ticks);
            set(&mut self.h.world, e, &TRANSPORT_ITEMS_COOLDOWN_TICKS, MemValue::Int(i64::from(ticks)), Some(ticks as u64));
        }
    }
    fn step(&mut self) {
        self.r.step(None);
        self.h.tick();
    }
}

/// 30 golems hit at tick 5: the tick `hurt_by` is first seen (a sensor scan, up to 20 ticks later), the tick the panic starts and
/// the length of the panic (100 to 120 ticks), the tick `visible_mobs` first appears, on both sides.
#[test]
fn the_sensors_and_the_panic_follow_the_mob_s_cadence() {
    if skip() {
        return;
    }
    let mut p = pair(30);
    let n = p.ids.len();
    let mut rows = vec![[None::<i64>; 4]; 2 * n];
    let mut was = vec![[false; 2]; 2 * n];
    for t in 1..=300i64 {
        if t == 5 {
            for &(i, e) in &p.ids {
                p.r.hit(i, 1.0, [0.0, 1.0, 0.0]);
                let at = p.h.world.get::<Pos>(e).unwrap().0;
                p.h.hit(e, 1.0, (at.x - 1.0, at.z));
            }
        }
        p.step();
        for (k, &(i, e)) in p.ids.iter().enumerate() {
            let probes = [
                (mob_has(&p.r.mobs[i], "visible_mobs"), mob_has(&p.r.mobs[i], "hurt_by"), mob_has(&p.r.mobs[i], "is_panicking")),
                (present(&p.h.world, e, &VISIBLE_MOBS), present(&p.h.world, e, &HURT_BY), present(&p.h.world, e, &IS_PANICKING)),
            ];
            for (side, (vis, hurt, panic)) in probes.into_iter().enumerate() {
                let r = &mut rows[side * n + k];
                let w = &mut was[side * n + k];
                if vis && r[0].is_none() {
                    r[0] = Some(t);
                }
                if hurt && r[1].is_none() {
                    r[1] = Some(t);
                }
                if panic && r[2].is_none() {
                    r[2] = Some(t);
                }
                if w[1] && !panic && r[3].is_none() {
                    r[3] = Some(t);
                }
                *w = [hurt, panic];
            }
        }
    }
    let col = |side: usize, c: usize| -> Vec<Option<i64>> { (0..n).map(|k| rows[side * n + k][c]).collect() };
    let (mv, ev) = (mean(&col(0, 0)), mean(&col(1, 0)));
    let (mh, eh) = (mean(&col(0, 1)), mean(&col(1, 1)));
    let (mp, ep) = (mean(&col(0, 2)), mean(&col(1, 2)));
    let (me, ee) = (mean(&col(0, 3)), mean(&col(1, 3)));
    eprintln!("visible_mobs first seen: mob {:.1} ({}) ecs {:.1} ({})", mv.0, mv.1, ev.0, ev.1);
    eprintln!("hurt_by first seen: mob {:.1} ({}) ecs {:.1} ({})", mh.0, mh.1, eh.0, eh.1);
    eprintln!("panic starts: mob {:.1} ({}) ecs {:.1} ({}); ends: mob {:.1} ({}) ecs {:.1} ({})", mp.0, mp.1, ep.0, ep.1, me.0, me.1, ee.0, ee.1);
    assert!(mv.1 == n && ev.1 == n, "every golem scans: mob {} ecs {}", mv.1, ev.1);
    assert!((mv.0 - ev.0).abs() < 4.0, "first scan: mob {:.1}, ecs {:.1}", mv.0, ev.0);
    assert!(mh.1 == n && eh.1 == n && (mh.0 - eh.0).abs() < 4.0, "hurt_by: mob {:.1} ({}), ecs {:.1} ({})", mh.0, mh.1, eh.0, eh.1);
    assert!(mp.1 == n && ep.1 == n && (mp.0 - ep.0).abs() < 4.0, "panic start: mob {:.1}, ecs {:.1}", mp.0, ep.0);
    assert!(me.1 == n && ee.1 == n && (me.0 - ee.0).abs() < 6.0, "panic end: mob {:.1}, ecs {:.1}", me.0, ee.0);
}

/// 30 golems left alone: the transport cooldown (60 to 99) counts down on both sides and the golems stroll or do nothing while
/// it lasts (the gate needs it): the tick the cooldown is gone, how many golems walked, and how far.
#[test]
fn the_golem_strolls_while_the_transport_cooldown_lasts_as_a_mob_does() {
    if skip() {
        return;
    }
    let mut p = pair(60);
    // The ECS herd lands in its first ticks (no path is made in the air): both sides settle before the comparison.
    for _ in 0..5 {
        p.step();
    }
    let n = p.ids.len();
    let start: Vec<(Vec3, Vec3)> = p.ids.iter().map(|&(i, e)| (p.r.mobs[i].position, p.h.world.get::<Pos>(e).unwrap().0)).collect();
    let mut gone = vec![[None::<i64>; 2]; n];
    let mut seen = vec![[false; 2]; n];
    let mut moved_at_60: Vec<(f64, f64)> = Vec::new();
    let mut targeted = vec![[false; 2]; n];
    for t in 1..=100i64 {
        p.step();
        if t == 60 {
            moved_at_60 = p.ids.iter().enumerate().map(|(k, &(i, e))| (xz(p.r.mobs[i].position, start[k].0), xz(p.h.world.get::<Pos>(e).unwrap().0, start[k].1))).collect();
        }
        for (k, &(i, e)) in p.ids.iter().enumerate() {
            if t <= 60 {
                targeted[k][0] |= mob_has(&p.r.mobs[i], "walk_target");
                targeted[k][1] |= present(&p.h.world, e, &WALK_TARGET);
            }
            let (m, c) = (mob_has(&p.r.mobs[i], "transport_items_cooldown_ticks"), present(&p.h.world, e, &TRANSPORT_ITEMS_COOLDOWN_TICKS));
            for (s, on) in [m, c].into_iter().enumerate() {
                seen[k][s] |= on;
                if seen[k][s] && !on && gone[k][s].is_none() {
                    gone[k][s] = Some(t);
                }
            }
        }
    }
    // The `Mob` restarts its transport (and sets the cooldown again) once it is gone: only the first 60 ticks, where every cooldown still lasts, compare.
    let moved = moved_at_60;
    eprintln!("walk targets set in the first 60 ticks: mob {} ecs {}", targeted.iter().filter(|x| x[0]).count(), targeted.iter().filter(|x| x[1]).count());
    let (mg, eg) = (mean(&gone.iter().map(|g| g[0]).collect::<Vec<_>>()), mean(&gone.iter().map(|g| g[1]).collect::<Vec<_>>()));
    let walked = |s: usize| moved.iter().filter(|m| if s == 0 { m.0 > 1.0 } else { m.1 > 1.0 }).count();
    let dist = |s: usize| moved.iter().map(|m| if s == 0 { m.0 } else { m.1 }).sum::<f64>() / n as f64;
    eprintln!("cooldown gone: mob {:.1} ({}) ecs {:.1} ({}); walked more than a block: mob {}/{n} ecs {}/{n}; mean distance: mob {:.2} ecs {:.2}", mg.0, mg.1, eg.0, eg.1, walked(0), walked(1), dist(0), dist(1));
    assert!(mg.1 == n && eg.1 == n && (mg.0 - eg.0).abs() < 4.0, "cooldown: mob {:.1}, ecs {:.1}", mg.0, eg.0);
    assert!(walked(1) > 0 && walked(0) > 0);
}

/// A memory written with a time to live of 5 is present for the tick it is written and 5 more, and forgotten at the start of the
/// next (`Brain.forgetOutdatedMemories` before the sensors); rewritten before, it keeps the later deadline.
#[test]
fn a_memory_expires_to_the_tick() {
    if skip() {
        return;
    }
    let mut p = pair(1);
    let e = p.ids[0].1;
    p.h.tick();
    set(&mut p.h.world, e, &LOOK_TARGET, MemValue::Look(Tracker::Block([0, 1, 0])), Some(5));
    let mut pattern = String::new();
    for _ in 0..8 {
        p.h.tick();
        pattern.push(if present(&p.h.world, e, &LOOK_TARGET) { '1' } else { '0' });
    }
    assert_eq!(pattern, "11111000", "present for five ticks after the one it was written in");
}

/// 60 golems with a long transport cooldown, 100 to 400 ticks (the start of the two sides is settled): how many walks they begin
/// and the share of the ticks they spend walking (a walk target present), on both sides.
#[test]
fn the_golem_strolls_as_much_as_a_mob_does() {
    if skip() {
        return;
    }
    let mut p = pair(60);
    p.long_cooldown(2000);
    let n = p.ids.len();
    let (mut starts, mut ticks) = ([0usize; 2], [0usize; 2]);
    let mut was = vec![[false; 2]; n];
    for t in 1..=400 {
        p.step();
        if t < 100 {
            continue;
        }
        for (k, &(i, e)) in p.ids.iter().enumerate() {
            let on = [mob_has(&p.r.mobs[i], "walk_target"), present(&p.h.world, e, &WALK_TARGET)];
            for s in 0..2 {
                starts[s] += usize::from(on[s] && !was[k][s]);
                ticks[s] += usize::from(on[s]);
            }
            was[k] = on;
        }
    }
    eprintln!("walks begun: mob {} ecs {}; golem-ticks with a walk target: mob {} ecs {}", starts[0], starts[1], ticks[0], ticks[1]);
    let near = |a: usize, b: usize| (a as f64 - b as f64).abs() <= 0.15 * a.max(b) as f64;
    assert!(near(starts[0], starts[1]), "walks begun: mob {}, ecs {}", starts[0], starts[1]);
    assert!(near(ticks[0], ticks[1]), "ticks walking: mob {}, ecs {}", ticks[0], ticks[1]);
}
