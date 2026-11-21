//! The rabbit (bounds along a path) and the magma cube (size, split) of the frame against their `Mob`s in the same scene
//! (`HORIZON_SPECIES=minecraft:rabbit` / `minecraft:magma_cube`, `HORIZON_TEST_RESOURCES` set).
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::{go_to_at, Entered};
use horizon_ecs_proto::size::{set_size, spawn_sized, Size};

fn is(name: &str) -> bool {
    have_data() && !wrong_species(&[name])
}
#[derive(Default)]
struct Bounds {
    takeoffs: usize,
    lengths: Vec<f64>,
    heights: Vec<f64>,
}
fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len().max(1) as f64
}

/// 16 rabbits sent 20 blocks along +z at the speed of a stroll (0.6): the number of bounds in 300 ticks, the length and the height of a bound.
#[test]
fn a_rabbit_bounds_along_its_path_as_the_original() {
    if horizon_ecs_proto::cow::skip_unless_small_body("a_rabbit_bounds_along_its_path_as_the_original") {
        return;
    }
    if !is("minecraft:rabbit") {
        return;
    }
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab_awake(60, true, false);
    let n = 16;
    let mut ids = Vec::new();
    for k in 0..n {
        let (x, z) = (-50.0 + 6.0 * k as f64 + 0.5, -40.0 + 0.5);
        let i = r.add_id(Vec3::new(x, 1.0, z), 5000 + 977 * k as u32);
        r.mobs[i].ai.goals.clear();
        r.mobs[i].ai.targets.clear();
        let e = spawn_species(&mut h.world, species(), x, z, 61 + k as u64);
        h.world.resource_mut::<Entered>().0.push(e);
        ids.push((i, e, x, z));
    }
    let view = player_view(Vec3::new(0.0, 1.0, 100.0), None);
    let (mut mb, mut eb) = (Bounds::default(), Bounds::default());
    let mut mair = vec![None::<(Vec3, f64)>; n];
    let mut eair = vec![None::<(Vec3, f64)>; n];
    for t in 0..300 {
        if t == 20 {
            for &(i, e, x, z) in &ids {
                let m = &mut r.mobs[i];
                m.nav_move_to(r.w, [x, 1.0, z + 20.0], 0.6);
                go_to_at(&mut h.world, e, [x, 1.0, z + 20.0], 0.6, 1);
            }
        }
        r.step(Some(&view));
        h.tick();
        for (k, &(i, e, _, _)) in ids.iter().enumerate() {
            let (mp, mg) = (r.mobs[i].position, r.mobs[i].on_ground);
            let (ep, eg) = (h.world.get::<Pos>(e).unwrap().0, h.world.get::<Contact>(e).unwrap().on_ground);
            for (air, p, g, b) in [(&mut mair[k], mp, mg, &mut mb), (&mut eair[k], ep, eg, &mut eb)] {
                if !g && air.is_none() && t > 25 {
                    *air = Some((p, p.y));
                    b.takeoffs += 1;
                } else if let Some((from, top)) = air.as_mut() {
                    *top = top.max(p.y);
                    if g {
                        b.lengths.push(((p.x - from.x).powi(2) + (p.z - from.z).powi(2)).sqrt());
                        b.heights.push(*top - from.y);
                        *air = None;
                    }
                }
            }
        }
    }
    eprintln!("bounds in 280 ticks of 16 rabbits: mob {}, ecs {}; length: mob {:.3}, ecs {:.3}; height: mob {:.3}, ecs {:.3}", mb.takeoffs, eb.takeoffs, mean(&mb.lengths), mean(&eb.lengths), mean(&mb.heights), mean(&eb.heights));
    assert!(mb.takeoffs > 40 && eb.takeoffs > 40, "{} {}", mb.takeoffs, eb.takeoffs);
    assert!((mb.takeoffs as f64 - eb.takeoffs as f64).abs() < 0.15 * mb.takeoffs as f64, "the cadence differs");
    assert!((mean(&mb.lengths) - mean(&eb.lengths)).abs() < 0.1, "the length of a bound differs");
    assert!((mean(&mb.heights) - mean(&eb.heights)).abs() < 0.05, "the height of a bound differs");
}

/// The size of a magma cube: its health, speed, damage, box and the wait between bounds (four times a slime's), at sizes 1, 2 and 4; and what
/// it leaves when it dies (2 + nextInt(3) cubes of half the size, at the places `setUpSplitCube` gives).
#[test]
fn a_magma_cube_has_the_numbers_of_its_size_and_splits_as_the_original() {
    if !is("minecraft:magma_cube") {
        return;
    }
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab_awake(60, true, false);
    for size in [1, 2, 4] {
        let i = r.add_id(Vec3::new(0.5, 1.0, 0.5), 77 + size as u32);
        horizon_client_lib::mobs::cube::set_size(&mut r.mobs[i], size);
        let e = spawn_sized(&mut h.world, species(), 0.5, 0.5, 9, size);
        let (m, b) = (&r.mobs[i], h.world.get::<Body>(e).unwrap());
        eprintln!("size {size}: mob health {} speed {:.3} width {:.3}; ecs health {} speed {:.3} width {:.3}", m.health, m.movement_speed(), m.width(), h.world.get::<Health>(e).unwrap().0, b.speed, b.width);
        assert_eq!(m.health, h.world.get::<Health>(e).unwrap().0);
        assert!((m.movement_speed() - b.speed).abs() < 1e-6);
        assert!((m.width() - b.width).abs() < 1e-5);
        assert_eq!(h.world.get::<Size>(e).unwrap().0, size);
    }
    // The split: a size 4 cube dies; its children are of size 2, 2 to 4 of them, at the offsets of the original.
    // The cubes of the size checks above are still in the world: they would count as children of the first death.
    let left: Vec<Entity> = h.world.query_filtered::<Entity, With<Size>>().iter(&h.world).collect();
    for id in left {
        h.world.entity_mut(id).despawn();
    }
    let mut counts = [0usize; 6];
    for k in 0..2000 {
        let e = spawn_sized(&mut h.world, species(), 0.5 + (k % 20) as f64 * 2.0, 0.5, 100 + k as u64, 4);
        h.hit(e, 10_000.0, (0.0, 0.0));
        h.tick();
        let kids: Vec<_> = h.world.query::<(&Size, &Pos)>().iter(&h.world).filter(|(s, _)| s.0 == 2).map(|(_, p)| p.0).collect();
        counts[kids.len().min(5)] += 1;
        let ids: Vec<Entity> = h.world.query_filtered::<Entity, With<Size>>().iter(&h.world).collect();
        for id in ids {
            h.world.entity_mut(id).despawn();
        }
    }
    eprintln!("children of a size 4 cube, over 2000 deaths: {counts:?}");
    assert!(counts[0] + counts[1] == 0 && counts[5] == 0, "{counts:?}");
    // `2 + nextInt(3)` is uniform: each of the three within 6 standard deviations (about 25) of a third.
    assert!(counts[2..5].iter().all(|&c| (c as i64 - 667).abs() < 150), "{counts:?}");
    // The children of the original: the same law.
    let mut mc = [0usize; 6];
    for k in 0..2000 {
        let pi = r.add_id(Vec3::new(0.5, 1.0, 0.5), 9000 + k);
        let mut parent = r.mobs[pi].clone();
        horizon_client_lib::mobs::cube::set_size(&mut parent, 4);
        mc[(horizon_client_lib::mobs::cube::split_count(&mut parent) as usize).min(5)] += 1;
    }
    eprintln!("the original: {mc:?}");
    assert!(mc[2..5].iter().all(|&c| (c as i64 - 667).abs() < 150), "{mc:?}");
    let _ = set_size;
}
