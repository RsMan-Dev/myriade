//! The lot 7 (frog and goat long jumps and ram, camel postures, nautilus, dolphin) against real `Mob`s of the species
//! (`mob_kind`): the laws of first occurrence and of durations, N draws per side, with ids far apart (PIEGES 46: the
//! Java generator's first draw is correlated between neighbouring seeds). `--nocapture` prints. Cow body only.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{self, Mob, MobEvent, Who as MWho};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{behaviours::Entry, cow::*, goals::*, targeting::*};
use std::sync::Arc;

fn leak<T>(v: T) -> &'static T {
    Box::leak(Box::new(v))
}
fn ecs_with(h: &mut Herd, entries: &'static [Entry], x: f64, z: f64, seed: u64, phase: u64) -> Entity {
    let mut sp = *species();
    sp.goals = horizon_ecs_proto::species::Config { entries };
    set_lazy_look(&mut h.world, false);
    // The ECS generator is a xorshift whose seed is `| 1` (900 and 901 are one seed) and whose first draws of a small seed
    // are small: well-mixed seeds only (PIEGES 46).
    let seed = (seed + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let e = spawn_species(&mut h.world, &sp, x, z, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.get_mut::<Phase>(e).unwrap().0 = phase;
    h.world.resource_mut::<Entered>().0.push(e);
    e
}
/// Ids far apart: the Mob's seed comes from its id.
fn far_id(j: usize) -> u32 {
    1 + (j as u32) * 7919 % 100_003
}
fn mob_add(r: &mut MobRef, kind: Arc<mobs::MobType>, at: Vec3, j: usize) -> usize {
    let mut m = Mob::new(far_id(j), kind, at, 0.0);
    m.persistent = true;
    m.ai.no_action_time = 100;
    r.mobs.push(m);
    r.searches.push(0);
    r.mobs.len() - 1
}
fn on() -> bool {
    if !have_data() {
        return false;
    }
    let sp = std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| "minecraft:cow".into());
    if sp != "minecraft:cow" {
        horizon_ecs_proto::cow::skip_note("lot4_vs_mob", &format!("written for HORIZON_SPECIES=minecraft:cow, not {sp}"));
        return false;
    }
    true
}
fn spot(j: usize, step: f64, row: usize) -> Vec3 {
    Vec3::new(-20.0 + step * (j % row) as f64 + 0.5, 1.0, -20.0 + step * (j / row) as f64 + 0.5)
}
fn near(j: usize) -> Vec3 {
    Vec3::new(-0.5 + 1.2 * (j % 8) as f64, 1.0, -3.5 + 1.2 * (j / 8) as f64)
}
/// A roof block over a column of the `Mob`'s world, with the light engine run on it (a raw `set_block` leaves the sky light as it
/// was: the `Mob` still sees the sky; the ECS's `BlockChanges` relights).
fn roof(r: &MobRef, x: i32, z: i32) {
    r.set_block(x, 6, z, "minecraft:stone");
    // SAFETY: as `MobRef::set_block`.
    unsafe {
        horizon_client_lib::mobs::world::relight_test_world(&mut *r.level.blocks.0, &[(x, 6, z)]);
    }
}
fn mean_of(v: &[Option<usize>]) -> (usize, f64) {
    let f: Vec<f64> = v.iter().filter_map(|x| x.map(|t| t as f64)).collect();
    (f.len(), f.iter().sum::<f64>() / f.len().max(1) as f64)
}

/// A block with properties in the world of the `Mob` (`MobRef`) and in the one of the ECS (the herd's level): the ECS can only
/// write names through `BlockChanges`, the controls that need a ripe bush write the state directly.
fn set_state(r: &MobRef, h: &Herd, x: i32, y: i32, z: i32, name: &str, props: &[(&str, &str)]) {
    let reg = r.w.registry;
    let base = reg.default_state(name).unwrap();
    let mut st = reg.state(base).unwrap().clone();
    for (k, v) in props {
        st.properties.insert((*k).into(), (*v).into());
    }
    let id = reg.id(&st).unwrap();
    for blocks in [r.level.blocks.0, h.world.resource::<Level>().blocks.0] {
        // SAFETY: the test thread owns the two worlds while it builds the scene (no reference to their blocks is alive).
        unsafe {
            let _ = (&mut *blocks).set(x, y, z, id);
        }
    }
}


/// The species of the registry with their `Mob`: the entry (name) and `Species` with all its goals.
fn ecs_named(h: &mut Herd, name: &str, entries: Option<&'static [Entry]>, x: f64, z: f64, seed: u64, phase: u64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let mut sp = *table.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name} is not in the registry"));
    if let Some(e) = entries {
        sp.goals = horizon_ecs_proto::species::Config { entries: e };
    }
    set_lazy_look(&mut h.world, false);
    let seed = (seed + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    // `spawn_armed` admits the animal to the zone: with no player near it is born asleep; the benches keep it awake as `ecs_with` does.
    let e = spawn_species(&mut h.world, &sp, x, z, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.get_mut::<Phase>(e).unwrap().0 = phase;
    h.world.resource_mut::<Entered>().0.push(e);
    // The kernel's stroll timer is armed for the animals of the build only: a spawned one is armed by hand (as `blocked` does).
    if h.world.get::<Stroll>(e).is_some() && h.world.get::<GoalSet>(e).is_some_and(|g| g.entries().iter().any(|en| en.module.name == "stroll")) {
        horizon_ecs_proto::behaviours::stroll::arm_now(&mut h.world, e);
    }
    e
}


fn ps_immortal(h: &mut Herd) {
    h.world.resource_mut::<horizon_ecs_proto::hostile::PlayerState>().immortal = true;
}



/// Day or night in both worlds (the `Mob`'s world is a leaked static: written through a raw pointer, as the blocks are).
fn set_day(r: &MobRef, h: &mut Herd, day: bool) {
    // SAFETY: the test thread owns the world while it builds the scene; no reference to the field is alive.
    unsafe {
        (*(r.w as *const mobs::MobWorld as *mut mobs::MobWorld)).daylight = day;
    }
    h.world.resource_mut::<horizon_ecs_proto::hostile::Daylight>().0 = day;
}

/// Counts the airborne episodes of at least `min` ticks of a position trace where `air` says the animal is off the ground.
fn episodes(air: &[bool], min: usize) -> (usize, Option<usize>) {
    let (mut n, mut run, mut first) = (0, 0, None);
    for (t, &a) in air.iter().enumerate() {
        if a {
            run += 1;
            if run == min {
                n += 1;
                first.get_or_insert(t);
            }
        } else {
            run = 0;
        }
    }
    (n, first)
}

/// `LongJump` of the frog (`frog.rs:826-860`: with a preference and no preferred block `candidate()` empties the list in one call, so ONE try per
/// start): does a real frog jump, and how often? Airborne episodes of 12 ticks or more in 2400 ticks, 6 draws, each side alone on flat ground.
#[test]
fn frog_long_jumps_on_flat_ground() {
    if !on() {
        return;
    }
    let (mut nm, mut ne, mut fm, mut fe) = (0usize, 0usize, Vec::new(), Vec::new());
    for j in 0..6 {
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab(24, false, false);
        let i = mob_add(&mut r, mob_kind("minecraft:frog"), Vec3::new(0.5, 1.0, 0.5), j + 1);
        r.mobs[i].ai.no_action_time = 0;
        let e = ecs_named(&mut h, "minecraft:frog", None, 0.5, 0.5, 21_000 + j as u64, u64::from(far_id(j + 1)));
        let (mut am, mut ae) = (Vec::new(), Vec::new());
        for _ in 0..2400 {
            r.step(None);
            h.tick();
            am.push(!r.mobs[i].on_ground);
            ae.push(!h.world.get::<Contact>(e).map_or(true, |c| c.on_ground));
        }
        let (a, f) = episodes(&am, 12);
        let (b, g) = episodes(&ae, 12);
        nm += a;
        ne += b;
        fm.extend(f);
        fe.extend(g);
        eprintln!("frog #{j}: long jumps mob {a} (first {f:?}) ecs {b} (first {g:?})");
    }
    eprintln!("frog: long jumps in 6 x 2400 ticks: mob {nm} ecs {ne}; first tick mean mob {:.0} ecs {:.0}", fm.iter().sum::<usize>() as f64 / fm.len().max(1) as f64, fe.iter().sum::<usize>() as f64 / fe.len().max(1) as f64);
}


/// A trench (x 2 and 3, all z) across the flat ground, in both worlds: dry (`water = false`) or water with lily pads on it (`true`).
fn trench(r: &MobRef, h: &mut Herd, water: bool) {
    for x in 2..=3 {
        for z in -12..=12 {
            // Water: a pool one block deep; dry: a pit five blocks deep (a fall a walker does not take).
            for y in if water { 0..=0 } else { -4..=0 } {
                let below = if water { "minecraft:water" } else { "minecraft:air" };
                r.set_block(x, y, z, below);
                h.set_block(x, y, z, below);
            }
            if water {
                r.set_block(x, 1, z, "minecraft:lily_pad");
                h.set_block(x, 1, z, "minecraft:lily_pad");
            }
        }
    }
}

/// The frog where the `Mob`'s long jump can start: a trench it cannot walk across (dry, or water with lily pads, the preferred blocks), by
/// day and by night. Airborne episodes in 6 x 2400 ticks.
#[test]
fn frog_long_jumps_across_a_trench() {
    if !on() {
        return;
    }
    for (water, day) in [(false, true), (false, false), (true, true), (true, false)] {
        let (mut nm, mut ne) = (0usize, 0usize);
        for j in 0..6 {
            let mut r = MobRef::new(24, false);
            let mut h = Herd::with_slab(24, false, false);
            trench(&r, &mut h, water);
            set_day(&r, &mut h, day);
            let i = mob_add(&mut r, mob_kind("minecraft:frog"), Vec3::new(0.5, 1.0, 0.5), j + 1);
            r.mobs[i].ai.no_action_time = 0;
            let e = ecs_named(&mut h, "minecraft:frog", None, 0.5, 0.5, 25_000 + j as u64, u64::from(far_id(j + 1)));
            let (mut am, mut ae) = (Vec::new(), Vec::new());
            for _ in 0..2400 {
                r.step(None);
                h.tick();
                am.push(!r.mobs[i].on_ground);
                ae.push(!h.world.get::<Contact>(e).map_or(true, |c| c.on_ground));
            }
            nm += episodes(&am, 12).0;
            ne += episodes(&ae, 12).0;
        }
        eprintln!("frog trench water={water} day={day}: long jumps in 6 x 2400 ticks: mob {nm} ecs {ne}");
    }
}

/// The goat: a cow 8 blocks away, 12000 ticks, 6 draws: the charges of the ram (a horizontal step of more than 0.4 blocks in a tick) on each side.
#[test]
fn goat_rams() {
    if !on() {
        return;
    }
    let (mut nm, mut ne, mut fm, mut fe, mut jm, mut je) = (0usize, 0usize, Vec::new(), Vec::new(), 0usize, 0usize);
    for j in 0..6 {
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab(24, false, false);
        let i = mob_add(&mut r, mob_kind("minecraft:goat"), Vec3::new(0.5, 1.0, 0.5), j + 1);
        r.mobs[i].ai.no_action_time = 0;
        let e = ecs_named(&mut h, "minecraft:goat", None, 0.5, 0.5, 22_000 + j as u64, u64::from(far_id(j + 1)));
        // The target is a cow with no goals: the ECS ram does not take players for targets (its documented fallback), the `Mob`'s does.
        let ci = mob_add(&mut r, mob_kind("minecraft:cow"), Vec3::new(8.5, 1.0, 0.5), j + 40);
        r.mobs[ci].ai.goals.clear();
        r.mobs[ci].ai.targets.clear();
        let ce = ecs_named(&mut h, "minecraft:cow", Some(&[]), 8.5, 0.5, 24_000 + j as u64, u64::from(far_id(j + 40)));
        for _ in 0..1 {
            h.world.get_mut::<horizon_ecs_proto::cow::Health>(ce).map(|mut x| x.0 = 1.0e6);
        }
        r.mobs[ci].health = 1.0e6;
        let view = player_view(Vec3::new(40.0, 1.0, 40.0), None);
        let (mut pm, mut pe) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
        let (mut sm, mut se, mut am, mut ae) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let (mut starts, mut was_ramming) = (Vec::new(), false);
        for t in 0..12000 {
            r.step(Some(&view));
            h.tick();
            let (qm, qe) = (r.mobs[i].position, h.world.get::<Pos>(e).map_or(pe, |p| p.0));
            let (gm, ge) = (r.mobs[i].on_ground, h.world.get::<Contact>(e).is_some_and(|c| c.on_ground));
            // A charge is fast on the ground; a leap is airborne.
            if xz(qm, pm) > 0.4 && gm {
                sm.push(t);
            }
            if xz(qe, pe) > 0.4 && ge {
                se.push(t);
            }
            am.push(!gm);
            ae.push(!ge);
            let ramming = h.world.get::<horizon_ecs_proto::behaviours::ram::Ramming>(e).is_some();
            if ramming && !was_ramming {
                starts.push(t);
            }
            was_ramming = ramming;
            (pm, pe) = (qm, qe);
        }
        let runs = |v: &[usize]| -> Vec<usize> { v.iter().enumerate().filter(|(k, t)| *k == 0 || **t - v[*k - 1] > 20).map(|(_, t)| *t).collect() };
        let (a, b) = (runs(&sm), runs(&se));
        let (lm, le) = (episodes(&am, 12), episodes(&ae, 12));
        nm += a.len();
        ne += b.len();
        jm += lm.0;
        je += le.0;
        fm.extend(a.first().copied());
        fe.extend(b.first().copied());
        eprintln!("goat #{j}: ecs ram goal starts {starts:?}; rams mob {a:?} ecs {b:?}; leaps mob {} (first {:?}) ecs {} (first {:?})", lm.0, lm.1, le.0, le.1);
    }
    eprintln!("goat: leaps mob {jm} ecs {je}; rams in 6 x 12000 ticks: mob {nm} ecs {ne}; first mean mob {:.0} ecs {:.0}", fm.iter().sum::<usize>() as f64 / fm.len().max(1) as f64, fe.iter().sum::<usize>() as f64 / fe.len().max(1) as f64);
}

/// The camel sits down and stands up (`RandomSitting`: sitting periods; the pose changes take 40 and 52 ticks): the sitting periods of 6 camels in
/// 6000 ticks, on each side (informative).
#[test]
fn camel_sits_and_stands() {
    if !on() {
        return;
    }
    let (mut tm, mut te) = (Vec::new(), Vec::new());
    for j in 0..6 {
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab(24, false, false);
        let i = mob_add(&mut r, mob_kind("minecraft:camel"), Vec3::new(0.5, 1.0, 0.5), j + 1);
        r.mobs[i].ai.no_action_time = 0;
        let e = ecs_named(&mut h, "minecraft:camel", None, 0.5, 0.5, 23_000 + j as u64, u64::from(far_id(j + 1)));
        let (mut sm, mut se, mut lm, mut le) = (false, false, 0usize, 0usize);
        for t in 0..6000 {
            r.step(None);
            h.tick();
            let (a, b) = (mobs::camel::sitting(&r.mobs[i]), horizon_ecs_proto::behaviours::posture::is_sitting(&h.world, e));
            if a != sm {
                if sm {
                    tm.push(t - lm);
                }
                (sm, lm) = (a, t);
            }
            if b != se {
                if se {
                    te.push(t - le);
                }
                (se, le) = (b, t);
            }
        }
    }
    let m = |v: &Vec<usize>| v.iter().sum::<usize>() as f64 / v.len().max(1) as f64;
    eprintln!("camel: sitting periods mob {} (mean {:.0}) ecs {} (mean {:.0}) ticks", tm.len(), m(&tm), te.len(), m(&te));
}

/// The species of the wave start (goat, nautilus, camel_husk, llama with its spit, dolphin with its jump) and a dolphin that went under for air
/// does not die: its `breath_air` brings the air back in a pool (`Air` from 120 to above 250 in 600 ticks).
#[test]
fn the_species_of_lot_7_start_and_the_dolphin_breathes() {
    if !on() {
        return;
    }
    for name in ["minecraft:goat", "minecraft:frog", "minecraft:camel", "minecraft:camel_husk", "minecraft:nautilus", "minecraft:llama", "minecraft:dolphin"] {
        let mut h = Herd::with_slab(24, false, false);
        let e = ecs_named(&mut h, name, None, 0.5, 0.5, 31, 3);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 5.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
        for _ in 0..60 {
            h.tick();
        }
        assert!(h.world.get_entity(e).is_ok(), "{name} starts and stays alive");
    }
    let mut h = Herd::with_slab(24, false, false);
    for x in -12..=-1 {
        for z in -12..=12 {
            for y in 1..=6 {
                h.set_block(x, y, z, "minecraft:water");
            }
        }
    }
    let e = ecs_named(&mut h, "minecraft:dolphin", None, 0.5, 0.5, 33, 3);
    h.teleport(e, Vec3::new(-6.5, 1.5, 0.5));
    h.world.get_mut::<Air>(e).unwrap().0 = 120;
    let mut best = 0;
    for _ in 0..600 {
        h.tick();
        best = best.max(h.world.get::<Air>(e).map_or(0, |a| a.0));
    }
    assert!(h.world.get_entity(e).is_ok(), "the dolphin survived 600 ticks under the surface");
    eprintln!("dolphin: best air in 600 ticks {best} (started at 120)");
    assert!(best > 250, "breath_air takes the dolphin up to breathe: best air {best}");
}

/// 300 goats on a walled slab (the 33 blocks of the farm test), 800 ticks: the share of the animals that left the slab (over the wall, or
/// below the world) is the `Mob`'s. Law over 300 animals and 3 repeats per side.
#[test]
fn goats_leave_the_walled_slab_as_often_as_the_mob() {
    if !on() {
        return;
    }
    let cnt: usize = std::env::var("GOATS").ok().and_then(|v| v.parse().ok()).unwrap_or(300);
    let side = 18_i32;
    let half = 33_i32;
    let (mut out_m, mut out_e, mut n) = (0usize, 0usize, 0usize);
    for rep in 0..3_usize {
        let mut r = MobRef::new(half, false);
        let mut h = Herd::with_slab(half, false, false);
        let view = player_view(Vec3::new(0.0, 1.0, 0.0), None);
        let mut ids = Vec::new();
        let mut es = Vec::new();
        for k in 0..cnt {
            let j = rep * 300 + k + 1;
            let at = Vec3::new(-f64::from(side) * 3.0 / 2.0 + f64::from(k as i32 % side) * 3.0 + 0.5, 1.0, -f64::from(side) * 3.0 / 2.0 + f64::from(k as i32 / side) * 3.0 + 0.5);
            ids.push(mob_add(&mut r, mob_kind("minecraft:goat"), at, j));
            r.mobs[ids[k]].ai.no_action_time = 0;
            es.push(ecs_named(&mut h, "minecraft:goat", None, at.x, at.z, 30_000 + j as u64, u64::from(far_id(j))));
        }
        let (s0m, s0e): (Vec<Vec3>, Vec<Vec3>) = (ids.iter().map(|&i| r.mobs[i].position).collect(), es.iter().map(|&e| h.world.get::<Pos>(e).unwrap().0).collect());
        let (mut nearm, mut neare, mut airm, mut aire) = (vec![false; cnt], vec![false; cnt], vec![false; cnt], vec![false; cnt]);
        let hw = f64::from(half) - 3.0;
        for t in 0..800 {
            r.step(Some(&view));
            h.tick();
            for k in 0..cnt {
                let pm = r.mobs[ids[k]].position;
                nearm[k] |= pm.x.abs() > hw || pm.z.abs() > hw;
                airm[k] |= t > 5 && !r.mobs[ids[k]].on_ground;
                if let Some(p) = h.world.get::<Pos>(es[k]) {
                    neare[k] |= p.0.x.abs() > hw || p.0.z.abs() > hw;
                }
                aire[k] |= h.world.get::<horizon_ecs_proto::behaviours::long_jump::Airborne>(es[k]).is_some();
            }
        }
        let mean = |a: Vec<f64>| a.iter().sum::<f64>() / a.len() as f64;
        eprintln!("mean displacement: Mob {:.1} ecs {:.1}; Mob ram cooldown >0 on {} animals", mean(ids.iter().enumerate().map(|(k, &i)| xz(r.mobs[i].position, s0m[k])).collect()), mean(es.iter().enumerate().map(|(k, &e)| xz(h.world.get::<Pos>(e).map_or(s0e[k], |p| p.0), s0e[k])).collect()), ids.iter().filter(|&&i| r.mobs[i].extra.memories.iter().any(|m| m.0 == "ram_cooldown_ticks")).count());
        let c = |v: &[bool]| v.iter().filter(|&&b| b).count();
        eprintln!("rep {rep}: reached within 3 of the wall Mob {} ecs {}; airborne at least once Mob {} ecs(leaps) {}", c(&nearm), c(&neare), c(&airm), c(&aire));
        let hf = f64::from(half);
        let outside = |p: Vec3| !(p.x.abs() < hf && p.z.abs() < hf && p.y > 0.99 && p.y < 1.2);
        out_m += ids.iter().filter(|&&i| outside(r.mobs[i].position)).count();
        out_e += es.iter().filter(|&&e| h.world.get::<Pos>(e).is_none_or(|p| outside(p.0))).count();
        n += cnt;
    }
    eprintln!("goats out of the slab after 800 ticks: Mob {out_m}/{n}, ecs {out_e}/{n}");
    // Measured with a player near (the Mob's stroll stops at `no_action_time` 100): Mob 90/900, ecs 83/900. Like the Mob: within 4 points (about 3 sd of the difference).
    assert!(out_e.abs_diff(out_m) * 100 <= 4 * n, "ecs goats leave the slab {out_e}/{n} against the Mob's {out_m}/{n}");
}

/// A goat 1.5 blocks from the wall (top at y 3, not walkable), the ram on cooldown on both sides (a leap needs it): does each leap onto the wall?
/// `reps` animals, 1500 ticks, in a row along the wall; counts the animals that were airborne at least once (the Mob: not on the ground; the ecs: `Airborne`).
#[test]
fn goats_beside_a_wall_leap_when_the_ram_cools_down() {
    if !on() {
        return;
    }
    let (mut jm, mut je, mut other_air) = (0usize, 0usize, 0usize);
    let n = 12;
    for j in 0..n {
        // (the Mob's stroll is off when `no_action_time` is 100 or more: a player stands near, and the counter starts at 0)
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab(24, false, false);
        let i = mob_add(&mut r, mob_kind("minecraft:goat"), Vec3::new(22.5, 1.0, 0.5), j + 1);
        r.mobs[i].ai.no_action_time = 0;
        let view = player_view(Vec3::new(0.0, 1.0, 0.0), None);
        r.mobs[i].extra.memories.push(("ram_cooldown_ticks", 1400, i64::MAX));
        let e = ecs_named(&mut h, "minecraft:goat", None, 22.5, 0.5, 40_000 + j as u64, u64::from(far_id(j + 1)));
        let now = h.world.resource::<Clock>().0;
        h.world.get_mut::<horizon_ecs_proto::behaviours::ram::RamCooldown>(e).unwrap().0 = now + 1400;
        let (mut am, mut ae, mut ee, mut am0, mut ae0) = (false, false, false, false, false);
        for t in 0..1500 {
            r.step(Some(&view));
            h.tick();
            if t > 5 {
                am |= !r.mobs[i].on_ground;
                let air = h.world.get::<horizon_ecs_proto::behaviours::long_jump::Airborne>(e).is_some();
                ae |= air;
                let odd = !air && h.world.get::<Contact>(e).is_some_and(|c| !c.on_ground);
                if odd && !ee { eprintln!("ecs #{j} airborne without Airborne at t={t} pos {:?}", h.world.get::<Pos>(e).map(|p| p.0)); }
                ee |= odd;
                if air && !ae0 { ae0 = true; eprintln!("ecs #{j} first leap t={t}"); }
                if !r.mobs[i].on_ground && !am0 { am0 = true; eprintln!("mob #{j} first air t={t} pos {:?}", r.mobs[i].position); }
            }
        }
        jm += usize::from(am);
        je += usize::from(ae);
        other_air += usize::from(ee);
    }
    eprintln!("goats beside the wall, ram cooling 1400 ticks: leaped Mob {jm}/{n}, ecs {je}/{n}; ecs airborne without a leap {other_air}/{n} (falls from the wall top)");
    assert!(jm > 0 && je > 0, "a goat beside a wall leaps onto it while the ram cools down: Mob {jm}/{n}, ecs {je}/{n}");
}
