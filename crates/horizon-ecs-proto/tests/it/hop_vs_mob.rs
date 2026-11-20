//! The hop mode of the frame (the slime, by configuration) against the `Mob` slime in the same scene (`HORIZON_SPECIES=minecraft:slime`,
//! `HORIZON_TEST_RESOURCES` set): the shape of one bound (its height and its distance), the cadence of the bounds, and the cost of a slime at rest.
use crate::common;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::Entered;
use horizon_ecs_proto::locomotion::hop::Hop;

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:slime"])
}
#[derive(Default, Debug)]
struct Bounds {
    takeoffs: Vec<usize>,
    distances: Vec<f64>,
    apexes: Vec<f64>,
}
fn stats(b: &Bounds) -> (f64, f64, f64) {
    let gaps: Vec<f64> = b.takeoffs.windows(2).map(|w| (w[1] - w[0]) as f64).collect();
    let m = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
    (m(&gaps), m(&b.distances), m(&b.apexes))
}

/// 24 slimes that head for +z (the direction imposed on both sides) hop for 600 ticks: the mean wait between two takeoffs, the mean length of a
/// bound and its mean height agree.
#[test]
fn a_bound_has_the_shape_and_the_cadence_of_the_original() {
    if skip() {
        return;
    }
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab(60, true, false);
    let n = 24;
    let mut ids = Vec::new();
    for k in 0..n {
        let (x, z) = (-50.0 + 4.0 * (k % 12) as f64 + 0.5, -40.0 + 30.0 * (k / 12) as f64 + 0.5);
        let i = r.add_id(Vec3::new(x, 1.0, z), 9000 + 977 * k as u32);
        r.mobs[i].ai.goals.clear();
        r.mobs[i].ai.targets.clear();
        let e = spawn_species(&mut h.world, species(), x, z, 41 + k as u64);
        h.world.resource_mut::<Entered>().0.push(e);
        h.world.get_mut::<Hop>(e).unwrap().redirect = i32::MAX;
        ids.push((i, e));
    }
    let (mut mb, mut eb) = (Bounds::default(), Bounds::default());
    let mut mob_air = vec![(false, Vec3::ZERO, 0.0_f64); n];
    let mut ecs_air = vec![(false, Vec3::ZERO, 0.0_f64); n];
    let view = player_view(Vec3::new(0.0, 1.0, 100.0), None);
    for t in 0..600 {
        for &(i, _) in &ids {
            let m = &mut r.mobs[i];
            m.extra.vector[0] = 0.0;
            m.extra.flag = false;
            m.ai.mov.speed_modifier = 1.0;
            m.ai.mov.operation = horizon_client_lib::mobs::control::Operation::MoveTo;
        }
        r.step(Some(&view));
        h.tick();
        for (k, &(i, e)) in ids.iter().enumerate() {
            let (mp, mg) = (r.mobs[i].position, r.mobs[i].on_ground);
            let (ep, eg) = (h.world.get::<Pos>(e).unwrap().0, h.world.get::<Contact>(e).unwrap().on_ground);
            for (air, p, g, b) in [(&mut mob_air[k], mp, mg, &mut mb), (&mut ecs_air[k], ep, eg, &mut eb)] {
                if !g && !air.0 && t > 20 {
                    *air = (true, p, p.y);
                    b.takeoffs.push(t);
                } else if air.0 {
                    air.2 = air.2.max(p.y);
                    if g {
                        b.distances.push(((p.x - air.1.x).powi(2) + (p.z - air.1.z).powi(2)).sqrt());
                        b.apexes.push(air.2 - air.1.y);
                        *air = (false, Vec3::ZERO, 0.0);
                    }
                }
            }
        }
    }
    // The takeoffs of all the slimes are mixed in the lists: only the per-slime gap counts, so the gap is taken as the total over the count.
    let rate = |b: &Bounds| 600.0 * n as f64 / b.takeoffs.len().max(1) as f64;
    let ((_, md, ma), (_, ed, ea)) = (stats(&mb), stats(&eb));
    eprintln!("ticks between two takeoffs of one slime: mob {:.1} ({} bounds), ecs {:.1} ({} bounds)", rate(&mb), mb.takeoffs.len(), rate(&eb), eb.takeoffs.len());
    eprintln!("bound length: mob {md:.3}, ecs {ed:.3}; height: mob {ma:.3}, ecs {ea:.3}");
    assert!((rate(&mb) - rate(&eb)).abs() < 0.1 * rate(&mb), "the cadence differs");
    assert!((md - ed).abs() < 0.1 * md.max(0.1), "the length of a bound differs");
    assert!((ma - ea).abs() < 0.05, "the height of a bound differs");
}

/// A slime between two bounds costs nothing: no visit of the hop system while it waits.
#[test]
fn a_slime_waiting_between_bounds_is_not_visited() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab(60, true, false);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 41);
    h.world.resource_mut::<Entered>().0.push(e);
    let mut waiting = 0;
    let mut total = 0;
    for _ in 0..300 {
        h.tick();
        total += 1;
        if h.world.get::<horizon_ecs_proto::locomotion::hop::Hopping>(e).is_none() && h.world.get::<Moving>(e).is_none() {
            waiting += 1;
        }
    }
    eprintln!("ticks without a visit: {waiting} of {total}");
    assert!(waiting * 100 > total * 40, "the slime is visited when it only waits: {waiting} of {total}");
}
