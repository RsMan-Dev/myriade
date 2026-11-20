//! Fox (`HORIZON_SPECIES=minecraft:fox`), real `Mob`s against the ECS fox on the same scene: the fall asleep by day and the wake
//! when a cow comes (as `lot2_vs_mob::fox_sleep_delay_and_wake`), and the walk under a roof in a storm (the `Mob`'s `thundering`,
//! the ECS `Storm` resource). `--nocapture` prints. Not written yet, see the notes: the jump on prey, the flight from an untrusted
//! player and the defence of a trusted player (no harness helper for a player's trust in this file).
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{self, Mob};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    behaviours::{Entry, fox_sleep, shelter},
    cow::*,
    goals::*,
};
use std::sync::Arc;

fn leak<T>(v: T) -> &'static T {
    Box::leak(Box::new(v))
}
fn ecs_with(h: &mut Herd, entries: &'static [Entry], x: f64, z: f64, seed: u64, phase: u64) -> Entity {
    let mut sp = *species();
    sp.goals = horizon_ecs_proto::species::Config { entries };
    set_lazy_look(&mut h.world, false);
    // Well-mixed seeds only (the ECS generator's first draws of a small seed are small, PIEGES 46).
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
    if sp != "minecraft:fox" {
        eprintln!("SKIP fox_vs_mob: written for HORIZON_SPECIES=minecraft:fox, not {sp}");
        return false;
    }
    true
}
fn spot(j: usize, step: f64, row: usize) -> Vec3 {
    Vec3::new(-20.0 + step * (j % row) as f64 + 0.5, 1.0, -20.0 + step * (j / row) as f64 + 0.5)
}
/// A roof block over a column of the `Mob`'s world, relit (as `lot2_vs_mob::roof`).
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

/// A fox under a roof, by day, alone: the tick it falls asleep, and the tick it wakes when a cow stands 5 blocks away from tick 400.
#[test]
fn fox_sleep_delay_and_wake() {
    if horizon_ecs_proto::cow::skip_unless_default_species("fox_sleep_delay_and_wake") {
        return;
    }
    if !on() {
        return;
    }
    let (mut ms, mut es, mut mw, mut ew) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for batch in 0..10 {
        let n = 16;
        let mut r = MobRef::new(24, false);
        // Day for the Mob (its world is leaked; nothing reads it while this thread writes the flag).
        unsafe {
            let w = r.w as *const horizon_client_lib::mobs::MobWorld as *mut horizon_client_lib::mobs::MobWorld;
            (*w).daylight = true;
        }
        let mut h = Herd::with_slab(24, false, false);
        h.world.insert_resource(horizon_ecs_proto::hostile::Daylight(true));
        let entries: &'static [Entry] = leak([Entry::new(&fox_sleep::MODULE, 7, &fox_sleep::Params::DEFAULT)]);
        let (mut ids, mut fe) = (Vec::new(), Vec::new());
        for j in 0..n {
            let p = spot(j, 12.0, 4);
            let i = mob_add(&mut r, mob_kind("minecraft:fox"), p, j + 50 * batch);
            ids.push(i);
            fe.push(ecs_with(&mut h, entries, p.x, p.z, 1500 + (j + 50 * batch) as u64, u64::from(far_id(j + 50 * batch))));
            roof(&r, p.x.floor() as i32, p.z.floor() as i32);
            h.set_block(p.x.floor() as i32, 6, p.z.floor() as i32, "minecraft:stone");
        }
        let (mut sm, mut se) = (vec![None; n], vec![None; n]);
        let (mut wm, mut we) = (vec![None; n], vec![None; n]);
        for t in 0..900 {
            if t == 400 {
                for j in 0..n {
                    let p = spot(j, 12.0, 4);
                    let k = r.kind.clone();
                    let c = mob_add(&mut r, k, Vec3::new(p.x + 5.0, 1.0, p.z), 1000 + j + 50 * batch);
                    r.mobs[c].ai.goals.clear();
                    // A zombie body for the ECS side: a cow would be "its kind", no alarm.
                    let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:zombie").unwrap();
                    sp.goals = horizon_ecs_proto::species::Config { entries: &[] };
                    let ce = spawn_species(&mut h.world, &sp, p.x + 5.0, p.z, 9000 + (j + 50 * batch) as u64);
                    h.world.get_mut::<ActiveUntil>(ce).unwrap().0 = u64::MAX;
                    h.world.resource_mut::<Entered>().0.push(ce);
                }
            }
            r.step(None);
            h.tick();
            for j in 0..n {
                let m_sleeps = r.mobs[ids[j]].ai.goals.running_names().iter().any(|x| x.contains("Sleep"));
                let e_sleeps = h.world.get::<fox_sleep::Sleeping>(fe[j]).is_some();
                if m_sleeps && sm[j].is_none() {
                    sm[j] = Some(t);
                }
                if e_sleeps && se[j].is_none() {
                    se[j] = Some(t);
                }
                if t >= 400 {
                    if !m_sleeps && sm[j].is_some() && wm[j].is_none() {
                        wm[j] = Some(t);
                    }
                    if !e_sleeps && se[j].is_some() && we[j].is_none() {
                        we[j] = Some(t);
                    }
                }
            }
        }
        ms.extend(sm);
        es.extend(se);
        mw.extend(wm);
        ew.extend(we);
    }
    let (a, b, c, d) = (mean_of(&ms), mean_of(&es), mean_of(&mw), mean_of(&ew));
    eprintln!("fox_sleep: asleep (count, mean tick) mob {a:?} ecs {b:?}; wakes after the cow came (count, mean tick) mob {c:?} ecs {d:?}");
    assert!(a.0 >= 100 && b.0 >= 100, "{a:?} {b:?}");
    assert!((a.1 - b.1).abs() < 30.0, "asleep {a:?} vs {b:?}");
    assert!(c.0 >= 100 && d.0 >= 100, "{c:?} {d:?}");
}

/// A fox on open ground in a storm: the tick it stands under a roof, the `Mob` (`thundering`) against the ECS (`Storm`), same roof.
#[test]
fn fox_in_a_storm_walks_under_a_roof_as_the_mob_does() {
    if horizon_ecs_proto::cow::skip_unless_default_species("fox_in_a_storm_walks_under_a_roof_as_the_mob_does") {
        return;
    }
    if !on() {
        return;
    }
    let (mut tm, mut te) = (Vec::new(), Vec::new());
    for batch in 0..6 {
        let mut r = MobRef::new(24, false);
        unsafe {
            let w = r.w as *const horizon_client_lib::mobs::MobWorld as *mut horizon_client_lib::mobs::MobWorld;
            (*w).thundering = true;
        }
        let mut h = Herd::with_slab(24, false, false);
        h.world.insert_resource(shelter::Storm(true));
        let entries: &'static [Entry] = leak([Entry::new(&shelter::MODULE, 6, &shelter::Params { speed: 1.25, interval: 100, blocked: 32 })]);
        // A stone roof over x 0..=8, z -4..=4 at y 8, as the same scene on both sides.
        for x in 0..=8 {
            for z in -4..=4 {
                r.set_block(x, 8, z, "minecraft:stone");
                h.set_block(x, 8, z, "minecraft:stone");
            }
        }
        r.step(None);
        h.tick();
        let at = Vec3::new(14.5, 1.0, 0.5);
        let i = mob_add(&mut r, mob_kind("minecraft:fox"), at, batch);
        let e = ecs_with(&mut h, entries, at.x, at.z, 900 + batch as u64, u64::from(far_id(batch)));
        let (mut tmob, mut tecs) = (None, None);
        for t in 0..600usize {
            r.step(None);
            h.tick();
            let mp = r.mobs[i].position;
            let (x, y, z) = (mp.x.floor() as i32, (mp.y + 1.0).floor() as i32, mp.z.floor() as i32);
            if tmob.is_none() && !r.w.can_see_sky(x, y, z) {
                tmob = Some(t);
            }
            let p = h.world.get::<Pos>(e).expect("the fox has a position").0;
            let (x, y, z) = (p.x.floor() as i32, (p.y + 1.0).floor() as i32, p.z.floor() as i32);
            if tecs.is_none() && !h.world.resource::<horizon_ecs_proto::cow::Level>().mw.0.can_see_sky(x, y, z) {
                tecs = Some(t);
            }
        }
        tm.push(tmob);
        te.push(tecs);
    }
    let (a, b) = (mean_of(&tm), mean_of(&te));
    eprintln!("fox_storm: under a roof (count, mean tick) mob {a:?} ecs {b:?}");
    assert!(a.0 == 6 && b.0 == 6, "{a:?} {b:?}");
    assert!((a.1 - b.1).abs() < 60.0, "under a roof {a:?} vs {b:?}");
}
