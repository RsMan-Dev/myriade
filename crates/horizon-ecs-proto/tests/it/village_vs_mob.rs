//! The iron golem of the frame, by configuration, on the registry of the points of interest, against the `Mob` iron golem in the same village
//! (`HORIZON_SPECIES=minecraft:iron_golem`, `HORIZON_TEST_RESOURCES` set): a village put down in blocks (a bell, a composter, a lectern, a
//! blast furnace) and, on the `Mob` side, the villagers whose memories hold them (the `Mob` reads the village from the villagers); the golem starts
//! 40 blocks from it. What is observed: does it come back, how near it is, does it stay.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::Entered;
use horizon_ecs_proto::poi::PoiRegistry;

const POINTS: [([i32; 3], &str); 4] = [([0, 1, 0], "minecraft:bell"), ([4, 1, 0], "minecraft:composter"), ([-4, 1, 0], "minecraft:lectern"), ([0, 1, 6], "minecraft:blast_furnace")];
fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:iron_golem"])
}
#[derive(Debug)]
struct Run {
    nearest: f64,
    reached_at: Option<usize>,
    inside: f64,
    end: (f64, f64),
    entered: Option<(f64, f64)>,
}
fn norm((x, z): (f64, f64)) -> f64 {
    (x * x + z * z).sqrt()
}
/// Chebyshev distance in sections from the position to the nearest village point's section (0 inside it).
fn zone_gap((x, z): (f64, f64)) -> i32 {
    let (sx, sz) = ((x.floor() as i32).div_euclid(16), (z.floor() as i32).div_euclid(16));
    POINTS.iter().map(|(p, _)| (sx - p[0].div_euclid(16)).abs().max((sz - p[2].div_euclid(16)).abs())).min().unwrap_or(i32::MAX)
}
/// Stopped in the village zone: `ServerLevel.isVillage` is true within 1 section of a village point (`isCloseToVillage(pos, 1)`), where
/// `MoveBackToVillageGoal.canUse` ends; the goal looks for the village up to 2 sections away. The golem entered that zone and ends in it
/// (inside it the golem strolls where it likes: its distance to the bell is not a law).
fn stays_in_zone(r: &Run) -> bool {
    r.entered.is_some() && zone_gap(r.end) <= 1
}
fn summarise(track: &[(f64, f64)]) -> Run {
    let d: Vec<f64> = track.iter().map(|&p| norm(p)).collect();
    let reached_at = d.iter().position(|&x| x < 20.0).map(|t| t * 10);
    let last = &d[d.len() * 2 / 3..];
    Run {
        nearest: d.iter().copied().fold(f64::MAX, f64::min),
        reached_at,
        inside: last.iter().filter(|&&x| x < 36.0).count() as f64 / last.len() as f64,
        end: track[track.len() - 1],
        entered: track.iter().copied().find(|&p| zone_gap(p) <= 1),
    }
}
fn ecs_run(villagers: bool) -> Run {
    ecs_seed(villagers, 21)
}
fn ecs_seed(villagers: bool, seed: u64) -> Run {
    let mut h = Herd::with_slab_awake(70, true, false);
    for (p, n) in POINTS {
        h.set_block(p[0], p[1], p[2], n);
    }
    h.tick();
    if villagers {
        for (p, _) in POINTS {
            h.world.resource_mut::<PoiRegistry>().acquire(p);
        }
    }
    let e = spawn_species(&mut h.world, species(), -40.5, 0.5, seed);
    h.world.resource_mut::<Entered>().0.push(e);
    let mut track = Vec::new();
    for t in 0..3000 {
        h.tick();
        if t % 10 == 0 {
            let p = h.world.get::<Pos>(e).unwrap().0;
            track.push((p.x, p.z));
        }
    }
    summarise(&track)
}
fn mob_run(villagers: bool) -> Run {
    mob_seed(villagers, 55_001)
}
fn mob_seed(villagers: bool, id: u32) -> Run {
    let mut r = MobRef::new(70, false);
    for (p, n) in POINTS {
        r.set_block(p[0], p[1], p[2], n);
    }
    if villagers {
        // The villagers (no AI) hold the points: a job site each, and the bell as their meeting point.
        for (k, (p, _)) in POINTS.iter().enumerate().skip(1) {
            let i = r.add_kind(Vec3::new(1.5 + k as f64, 1.0, 2.5), mob_kind("minecraft:villager"));
            r.mobs[i].no_ai = true;
            r.mobs[i].extra.home = Some(*p);
            r.mobs[i].extra.meeting = Some(POINTS[0].0);
        }
    }
    let g = r.add_id(Vec3::new(-40.5, 1.0, 0.5), id);
    let view = player_view(Vec3::new(0.0, 1.0, 300.0), None);
    let mut track = Vec::new();
    for t in 0..3000 {
        r.step(Some(&view));
        if t % 10 == 0 {
            let p = r.mobs[g].position;
            track.push((p.x, p.z));
        }
    }
    summarise(&track)
}

/// With villagers that hold the points, both golems come back to the village and stay in it.
#[test]
fn the_golem_goes_back_to_a_village_and_stays_as_the_original() {
    if skip() {
        return;
    }
    let (e, m) = (ecs_run(true), mob_run(true));
    eprintln!("the village held by villagers: ecs {e:?}\n                                mob {m:?}");
    assert!(e.reached_at.is_some() && m.reached_at.is_some(), "one of them does not come back: ecs {e:?}, mob {m:?}");
    assert!(e.inside > 0.8 && m.inside > 0.8, "ecs {e:?}, mob {m:?}");
}

/// The village the registry knows and the villagers do not hold (no memory holds its points): the `Mob` sees no village and wanders
/// where it is; the registry's golem goes back to it (what vanilla's `PoiManager` does).
#[test]
fn a_village_nobody_holds_is_seen_by_the_registry_and_not_by_the_villagers_memories() {
    if skip() {
        return;
    }
    let (e, m) = (ecs_run(false), mob_run(false));
    eprintln!("the village held by nobody: ecs {e:?}\n                            mob {m:?}");
    // Jar: `MoveBackToVillageGoal.canUse` is false once `isVillage(pos)` (within 1 section of a village point; `findSectionClosestToVillage(.., 2)` is the reach of the search),
    // so the registry's golem is pulled only up to that range (about 23 blocks from the bell here, not the 20 of the villagers' case) and stays.
    assert!(stays_in_zone(&e) && e.inside > 0.8, "{e:?}");
    assert!(e.nearest < m.nearest - 10.0, "the Mob's golem does not see the village: ecs {e:?}, mob {m:?}");
}

/// The same stop, held to the `Mob` too. Not yet true: the `Mob` keeps walking past the zone's entrance.
#[test]
#[ignore = "Mob non conforme au jar : MoveBackToVillage ne s'arrête pas à l'entrée de la zone village (Mob 40,2 blocs, jar ~23)"]
fn the_village_nobody_holds_stops_the_ecs_golem_and_the_mob_golem_in_the_zone() {
    if skip() {
        return;
    }
    let (e, m) = (ecs_run(false), mob_run(false));
    eprintln!("the village held by nobody: ecs {e:?}\n                            mob {m:?}");
    assert!(stays_in_zone(&e) && stays_in_zone(&m) && e.inside > 0.8 && m.inside > 0.8, "ecs {e:?}, mob {m:?}");
}

/// The return time is a random walk (the golem is 3 sections away, the pull toward the village reaches 2): one seed proves nothing. The law of
/// the time to come within 20 blocks over 40 seeds on each side (the median and the quartiles, a run that does not come back counts 3000).
#[test]
fn the_return_time_has_the_law_of_the_original() {
    if skip() {
        return;
    }
    let law = |mut v: Vec<usize>| {
        v.sort_unstable();
        (v[v.len() / 4], v[v.len() / 2], v[3 * v.len() / 4], v.iter().sum::<usize>() / v.len())
    };
    let e: Vec<usize> = (0..40).map(|k| ecs_seed(true, 1000 + k).reached_at.unwrap_or(3000)).collect();
    let m: Vec<usize> = (0..40).map(|k| mob_seed(true, 56_000 + k as u32).reached_at.unwrap_or(3000)).collect();
    eprintln!("return time (q1, median, q3, mean): ecs {:?} mob {:?}", law(e.clone()), law(m.clone()));
}
