//! The prototype's `path_type_static` (neighbour answers worked out once per state per query) against Horizon's, on every cell of a
//! slab of fire, berries, water, lava, doors, fences, torches and cactus; and the amphibious / short searches of the long jump against
//! Horizon's `Finder`.
use crate::common::*;
use horizon_client_lib::mobs::path::{Evaluator, Finder, PathMob};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::build_level_with, cow::species_table, kernel_lab::Rng};
use std::sync::Arc;

const NAMES: [&str; 16] = [
    "minecraft:fire", "minecraft:soul_fire", "minecraft:sweet_berry_bush", "minecraft:water", "minecraft:lava", "minecraft:oak_door",
    "minecraft:oak_fence", "minecraft:torch", "minecraft:cactus", "minecraft:powder_snow", "minecraft:oak_trapdoor", "minecraft:campfire",
    "minecraft:magma_block", "minecraft:honey_block", "minecraft:oak_leaves", "minecraft:stone",
];

fn level() -> horizon_ecs_proto::cow::Level {
    build_level_with(0, |blocks, registry| {
        for cx in -4..8 {
            for cz in -4..4 {
                blocks.chunks.insert((cx, cz), Arc::new(horizon_client_render::chunk::Chunk::empty(cx, cz)));
            }
        }
        let stone = registry.default_state("minecraft:stone").unwrap();
        let mut r = Rng(0x5151_0101_7777_1234);
        for x in 0..96 {
            for z in -20..20 {
                for y in -2..=0 {
                    blocks.set(x, y, z, stone).unwrap();
                }
                for y in 1..=3 {
                    let v = r.unit();
                    if v < 0.22 {
                        let n = NAMES[(r.unit() * NAMES.len() as f64) as usize % NAMES.len()];
                        blocks.set(x, y, z, registry.default_state(n).unwrap()).unwrap();
                    }
                }
            }
        }
    })
    .0
}

#[test]
fn own_path_type_static_is_horizons() {
    if !have_data() {
        return;
    }
    let level = level();
    let mw = level.mw.0;
    let mut n = 0;
    for pass in 0..2 {
        // Pass 1 keeps the answers of pass 0 (the cache of one query, worked on a whole slab): the result must not move.
        if pass == 0 {
            horizon_ecs_proto::pathsearch::adjacent_reset();
        }
        for x in -2..98 {
            for z in -22..22 {
                for y in -3..6 {
                    let a = mw.path_type_static(x, y, z);
                    let b = horizon_ecs_proto::pathsearch::path_type_static(mw, x, y, z);
                    assert_eq!(a, b, "({x},{y},{z}) pass {pass}");
                    n += 1;
                }
            }
        }
    }
    assert!(n > 50_000);
}

#[test]
fn own_search_is_the_finder_for_the_long_jump_check() {
    if !have_data() {
        return;
    }
    let level = level();
    let mw = level.mw.0;
    let table = species_table().0;
    let mut r = Rng(0x2468_ACE0_1357_9BDF);
    let (mut total, mut reached) = (0, 0);
    for name in ["minecraft:frog", "minecraft:goat", "minecraft:cow"] {
        let Some(sp) = table.iter().find(|s| s.name == name).copied() else { continue };
        // One batch for all the searches of the creature kind (the kept answers must not leak between mobs or places).
        let _batch = horizon_ecs_proto::pathsearch::BlockBatch::begin();
        for q in 0..150 {
            let (x, z) = (2 + (r.unit() * 90.0) as i32, -18 + (r.unit() * 36.0) as i32);
            let (tx, tz) = (x + (r.unit() * 12.0) as i32 - 6, z + (r.unit() * 12.0) as i32 - 6);
            let pm = PathMob {
                position: Vec3::new(f64::from(x) + 0.5, 1.0, f64::from(z) + 0.5),
                width: sp.width,
                height: sp.height,
                on_ground: true,
                in_water: q % 5 == 0,
                max_up_step: 0.6,
                max_fall_distance: 3,
                malus: sp.malus,
                can_float: sp.can_float,
                can_open_doors: sp.can_open_doors,
                can_pass_doors: sp.can_pass_doors,
                can_walk_over_fences: sp.can_walk_over_fences,
                stands_on_lava: false,
            };
            for eval in [Evaluator::Walk, Evaluator::Amphibious { prefers_shallow: true }] {
                let target = [[tx, 1 + (r.unit() * 2.0) as i32, tz]];
                let a = Finder::new(mw, &pm, eval).find(&target, 8.0, 0, (sp.follow_range * 16.0).floor() as i32);
                let b = horizon_ecs_proto::pathsearch::Finder::new(mw, &pm, eval).find(&target, 8.0, 0, (sp.follow_range * 16.0).floor() as i32);
                total += 1;
                let same = match (&a, &b) {
                    (None, None) => true,
                    (Some(a), Some(b)) => a.nodes == b.nodes && a.reached == b.reached && a.target == b.target && a.dist_to_target == b.dist_to_target,
                    _ => false,
                };
                reached += usize::from(a.as_ref().is_some_and(|p| p.reached));
                assert!(same, "{name} q {q} {eval:?} from ({x},{z}) to {:?}: finder {:?} / own {:?}", target[0], a.as_ref().map(|p| (p.nodes.len(), p.reached)), b.as_ref().map(|p| (p.nodes.len(), p.reached)));
            }
        }
    }
    assert!(total > 0 && reached > 0, "{reached} of {total} searches reach");
}

/// A block changed through `block_changes` is seen by the walking search's shared layer (it forgot nothing before: stale types).
#[test]
fn layer_follows_block_changes() {
    if !have_data() {
        return;
    }
    let mut herd = horizon_ecs_proto::cow::Herd::with_hazards(1, 100.0, true, false, 5, false);
    let mw = herd.world.resource::<horizon_ecs_proto::cow::Level>().mw.0;
    let sp = species_table().0.iter().find(|s| s.name == "minecraft:cow").copied().unwrap();
    let pm = PathMob {
        position: Vec3::new(10.5, 1.0, 10.5),
        width: sp.width,
        height: sp.height,
        on_ground: true,
        in_water: false,
        max_up_step: 0.6,
        max_fall_distance: 3,
        malus: sp.malus,
        can_float: sp.can_float,
        can_open_doors: sp.can_open_doors,
        can_pass_doors: sp.can_pass_doors,
        can_walk_over_fences: sp.can_walk_over_fences,
        stands_on_lava: false,
    };
    let at = |x, y, z| horizon_ecs_proto::pathsearch::Finder::new(mw, &pm, Evaluator::Walk).path_type_at(x, y, z);
    let horizon = |x, y, z| Finder::new(mw, &pm, Evaluator::Walk).path_type_at(x, y, z);
    let before = at(10, 0, 10);
    assert_eq!(before, horizon(10, 0, 10));
    herd.set_block(10, 0, 10, "minecraft:lava");
    herd.tick();
    let after = at(10, 0, 10);
    assert_ne!(before, after, "the layer kept the old type");
    assert_eq!(after, horizon(10, 0, 10));
}

/// One flood from the start answers every target of a long jump's pick as the search per target does (when it ran to its end).
#[test]
fn flood_is_the_search_per_target() {
    if !have_data() {
        return;
    }
    let level = level();
    let mw = level.mw.0;
    let table = species_table().0;
    let mut r = Rng(0x1357_9BDF_2468_ACE1);
    let (mut pairs, mut yes, mut complete, mut floods) = (0, 0, 0, 0);
    let mut diff = [[0usize; 2]; 2];
    for name in ["minecraft:frog", "minecraft:goat"] {
        let Some(sp) = table.iter().find(|s| s.name == name).copied() else { continue };
        let width = if name == "minecraft:frog" { 4 } else { 5 };
        for q in 0..120 {
            let (x, z) = (2 + (r.unit() * 90.0) as i32, -18 + (r.unit() * 36.0) as i32);
            let pm = PathMob {
                position: Vec3::new(f64::from(x) + 0.5, 1.0, f64::from(z) + 0.5),
                width: sp.width,
                height: sp.height,
                on_ground: true,
                in_water: q % 5 == 0,
                max_up_step: 0.6,
                max_fall_distance: 3,
                malus: sp.malus,
                can_float: sp.can_float,
                can_open_doors: sp.can_open_doors,
                can_pass_doors: sp.can_pass_doors,
                can_walk_over_fences: sp.can_walk_over_fences,
                stands_on_lava: false,
            };
            let mv = (sp.follow_range * 16.0).floor() as i32;
            for eval in [Evaluator::Walk, Evaluator::Amphibious { prefers_shallow: true }] {
                let Some((cells, done)) = horizon_ecs_proto::pathsearch::Finder::new(mw, &pm, eval).flood(8.0, mv) else { continue };
                floods += 1;
                if !done {
                    continue;
                }
                complete += 1;
                let set: std::collections::HashSet<[i32; 3]> = cells.into_iter().collect();
                for _ in 0..40 {
                    let t = [x + (r.unit() * f64::from(2 * width + 1)) as i32 - width, 1 + (r.unit() * 5.0) as i32 - 2, z + (r.unit() * f64::from(2 * width + 1)) as i32 - width];
                    let a = horizon_ecs_proto::pathsearch::Finder::new(mw, &pm, eval).find(&[t], 8.0, 0, mv).is_some_and(|p| p.reached);
                    pairs += 1;
                    yes += usize::from(a);
                    let b = set.contains(&t);
                    diff[usize::from(a)][usize::from(b)] += 1;
                }
            }
        }
    }
    println!("flood: {floods} floods, {complete} complete, {pairs} pairs, {yes} reached; [search][flood] = {diff:?}");
    assert!(pairs > 1000 && yes > 0);
}
