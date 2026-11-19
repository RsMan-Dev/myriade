//! The random position service (`spots.rs`) against a real `Mob` of the process's species, in a world with grass on one half
//! (z >= 0) and plain stone on the other: the share of picks on the grass, the mean position and the number of failures
//! of each picker, over thousands of draws (the laws, not the streams).
mod common;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, spots::*};
use horizon_ecs_proto::spots::spotter;

fn stats(v: &[[f64; 3]], fails: usize, n: usize) -> (f64, f64, f64, f64) {
    let m = |f: fn(&[f64; 3]) -> f64| v.iter().map(f).sum::<f64>() / v.len().max(1) as f64;
    let grass = v.iter().filter(|p| p[2] >= 0.0).count() as f64 / v.len().max(1) as f64;
    (grass, m(|p| p[0]), m(|p| p[2]), fails as f64 / n as f64)
}

#[test]
fn the_pickers_have_the_laws_of_the_mob() {
    if !have_data() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    for x in -12..=12 {
        for z in 0..=12 {
            r.set_block(x, 0, z, "minecraft:grass_block");
            h.set_block(x, 0, z, "minecraft:grass_block");
        }
    }
    h.tick();
    let n = 30000;
    let at = Vec3::new(0.5, 1.0, 0.5);
    let i = r.add(at);
    let level = *h.world.resource::<Level>();
    let sp = *species();
    let nav = NavParams {
        malus: sp.malus,
        can_float: sp.can_float,
        can_open_doors: sp.can_open_doors,
        can_pass_doors: sp.can_pass_doors,
        can_walk_over_fences: sp.can_walk_over_fences,
        follow_range: sp.follow_range,
    };
    let from = [4.5, 1.0, 0.5];
    let to = [-4.5, 1.0, 4.5];
    let cases: [(&str, Pick); 5] = [
        ("default 10,7", Pick::Default { h: 10, v: 7 }),
        ("land 10,7", Pick::Land { h: 10, v: 7 }),
        ("default away 16,7", Pick::DefaultAway { h: 16, v: 7, from }),
        ("land away 0..16,7", Pick::LandAway { min_h: 0.0, max_h: 16.0, v: 7, from }),
        ("land towards 10,7", Pick::LandTowards { h: 10, v: 7, to }),
    ];
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    for (name, pick) in cases {
        let (mut mob, mut ecs, mut mf, mut ef) = (Vec::new(), Vec::new(), 0, 0);
        for _ in 0..n {
            let m = &mut r.mobs[i];
            let p = match pick {
                Pick::Default { h, v } => m.default_random_pos(r.w, h, v),
                Pick::Land { h, v } => m.land_random_pos(r.w, h, v),
                Pick::DefaultAway { h, v, from } => m.default_random_pos_away(r.w, h, v, from),
                Pick::LandAway { min_h, max_h, v, from } => m.land_random_pos_away_between(r.w, min_h, max_h, v, from),
                Pick::LandTowards { h, v, to } => m.land_random_pos_towards(r.w, h, v, to),
                Pick::DefaultTowards { h, v, to, max_rad } => m.default_random_pos_towards(r.w, h, v, to, max_rad),
            };
            match p {
                Some(p) => mob.push(p),
                None => mf += 1,
            }
            let mut sp2 = spotter(&level, [at.x, at.y, at.z], &nav, sp.walk, &mut rng);
            match sp2.pick(pick) {
                Some(p) => ecs.push(p),
                None => ef += 1,
            }
        }
        let (a, b) = (stats(&mob, mf, n), stats(&ecs, ef, n));
        eprintln!("{name}: mob grass {:.3} x {:.2} z {:.2} fail {:.3} | ecs grass {:.3} x {:.2} z {:.2} fail {:.3}", a.0, a.1, a.2, a.3, b.0, b.1, b.2, b.3);
        assert!((a.0 - b.0).abs() < 0.03 && (a.1 - b.1).abs() < 0.25 && (a.2 - b.2).abs() < 0.25 && (a.3 - b.3).abs() < 0.03, "{name}");
    }
}

/// What one call of the picker costs, in instructions (the exact counter of the thread): ten tries and ten walk values.
#[test]
fn the_cost_of_a_pick_is_reported() {
    if !have_data() {
        return;
    }
    let (_r, h) = flat_pair();
    let level = *h.world.resource::<Level>();
    let sp = *species();
    let nav = NavParams { malus: sp.malus, can_float: sp.can_float, can_open_doors: sp.can_open_doors, can_pass_doors: sp.can_pass_doors, can_walk_over_fences: sp.can_walk_over_fences, follow_range: sp.follow_range };
    let Some(counter) = horizon_ecs_proto::instr::Counter::new() else { return };
    let mut rng = Rng(77);
    for (name, pick) in [("land 10,7", Pick::Land { h: 10, v: 7 }), ("default 5,4", Pick::Default { h: 5, v: 4 })] {
        let a = counter.read();
        let n = 20000;
        for _ in 0..n {
            let _ = spotter(&level, [0.5, 1.0, 0.5], &nav, sp.walk, &mut rng).pick(pick);
        }
        eprintln!("{name}: {} instructions per pick", (counter.read() - a) / n);
        for wv in [WalkValue::Zero, WalkValue::Animal, WalkValue::Monster] {
            let a = counter.read();
            for _ in 0..n {
                let _ = spotter(&level, [0.5, 1.0, 0.5], &nav, wv, &mut rng).pick(pick);
            }
            eprintln!("  {wv:?}: {} per pick", (counter.read() - a) / n);
        }
    }
}
