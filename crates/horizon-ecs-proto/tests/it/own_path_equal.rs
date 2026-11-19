//! `pathsearch::Finder` (the prototype's own search) against Horizon's `Finder`, node for node, on eight scenes: flat, obstacles,
//! slope and cliffs, water, doors and fences, hazards, an unreachable target, and a target beyond the range.
use crate::common::*;
use horizon_client_lib::mobs::path::{Evaluator, Finder, PathMob};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::build_level_with, cow::species_table, kernel_lab::Rng};
use std::sync::Arc;

const SCENES: i32 = 8;
const CELL: i32 = 64;
/// Stand height (y of the feet) at (x, z) of scene `s` whose origin is `ox`.
fn stand(s: i32, ox: i32, x: i32, _z: i32) -> i32 {
    match s {
        2 => {
            let d = x - ox;
            if d < 24 { 1 + d / 6 } else if d < 30 { 1 + 24 / 6 } else { 1 }
        }
        _ => 1,
    }
}

#[test]
fn own_search_is_node_for_node_the_finder() {
    if !have_data() {
        return;
    }
    let (level, _, _) = build_level_with(0, |blocks, registry| {
        let st = |n: &str| registry.default_state(n).unwrap();
        let stone = st("minecraft:stone");
        for cx in -4..(SCENES * CELL / 16 + 4) {
            for cz in -4..4 {
                blocks.chunks.insert((cx, cz), Arc::new(horizon_client_render::chunk::Chunk::empty(cx, cz)));
            }
        }
        let mut r = Rng(0xABCD_EF01_2345_6789);
        for s in 0..SCENES {
            let ox = s * CELL;
            for x in ox..ox + 40 {
                for z in -20..20 {
                    let top = stand(s, ox, x, z) - 1;
                    for y in -2..=top {
                        blocks.set(x, y, z, stone).unwrap();
                    }
                }
            }
            let put = |blocks: &mut horizon_client_render::world::World, name: &str, x: i32, y: i32, z: i32| {
                blocks.set(x, y, z, registry.default_state(name).unwrap()).unwrap();
            };
            match s {
                1 => {
                    for x in ox..ox + 40 {
                        for z in -20..20 {
                            if r.unit() < 0.04 {
                                put(blocks, "minecraft:stone", x, 1, z);
                                put(blocks, "minecraft:stone", x, 2, z);
                            }
                        }
                    }
                    for z in -20..20 {
                        if !(0..3).contains(&z) {
                            for y in 1..=3 {
                                put(blocks, "minecraft:stone", ox + 20, y, z);
                            }
                        }
                    }
                }
                3 => {
                    for x in ox + 10..ox + 30 {
                        for z in -10..10 {
                            put(blocks, "minecraft:water", x, 0, z);
                        }
                    }
                }
                4 => {
                    for z in -20i32..20 {
                        let kind = match z.rem_euclid(8) {
                            0 | 1 => Some("minecraft:oak_door"),
                            2 | 3 => Some("minecraft:oak_fence_gate"),
                            4 => Some("minecraft:oak_trapdoor"),
                            _ => None,
                        };
                        match kind {
                            Some(k) => {
                                put(blocks, k, ox + 20, 1, z);
                                if k == "minecraft:oak_door" {
                                    put(blocks, k, ox + 20, 2, z);
                                }
                            }
                            None => {
                                if z.rem_euclid(8) == 5 {
                                    put(blocks, "minecraft:oak_fence", ox + 20, 1, z);
                                } else {
                                    for y in 1..=3 {
                                        put(blocks, "minecraft:stone", ox + 20, y, z);
                                    }
                                }
                            }
                        }
                    }
                }
                5 => {
                    for x in ox..ox + 40 {
                        for z in -20..20 {
                            let v = r.unit();
                            if v < 0.02 {
                                put(blocks, "minecraft:lava", x, 0, z);
                            } else if v < 0.04 {
                                put(blocks, "minecraft:cactus", x, 1, z);
                            } else if v < 0.06 {
                                put(blocks, "minecraft:fire", x, 1, z);
                            } else if v < 0.08 {
                                put(blocks, "minecraft:powder_snow", x, 0, z);
                            } else if v < 0.10 {
                                put(blocks, "minecraft:sweet_berry_bush", x, 1, z);
                            }
                        }
                    }
                }
                6 => {
                    for x in ox + 28..=ox + 32 {
                        for z in -2i32..=2 {
                            if x == ox + 28 || x == ox + 32 || z.abs() == 2 {
                                for y in 1..=3 {
                                    put(blocks, "minecraft:stone", x, y, z);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    });
    let mw = level.mw.0;
    let table = species_table().0;
    let mut dev = Vec::new();
    let mut total = 0usize;
    for name in ["minecraft:cow", "minecraft:zombie", "minecraft:pig"] {
        let Some(sp) = table.iter().find(|s| s.name == name).copied() else { continue };
        for s in 0..SCENES {
            let ox = s * CELL;
            let mut r = Rng(0x1234_5678 + s as u64);
            for q in 0..30 {
                let (x, z) = (ox + 2 + (r.unit() * 36.0) as i32, -18 + (r.unit() * 36.0) as i32);
                let (tx, tz) = if s == 6 && q % 2 == 0 {
                    (ox + 30, 0)
                } else if s == 7 {
                    (ox + 2 + (r.unit() * 36.0) as i32, -18 + (r.unit() * 36.0) as i32)
                } else {
                    (ox + 2 + (r.unit() * 36.0) as i32, -18 + (r.unit() * 36.0) as i32)
                };
                let pm = PathMob {
                    position: Vec3::new(x as f64 + 0.5, stand(s, ox, x, z) as f64, z as f64 + 0.5),
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
                let max_len = if s == 7 { 6.0 } else { sp.follow_range };
                let visited = (max_len * 16.0).floor() as i32;
                let target = [[tx, stand(s, ox, tx, tz), tz]];
                let a = Finder::new(mw, &pm, Evaluator::Walk).find(&target, max_len, 1, visited);
                let b = horizon_ecs_proto::pathsearch::Finder::new(mw, &pm, Evaluator::Walk).find(&target, max_len, 1, visited);
                total += 1;
                let same = match (&a, &b) {
                    (None, None) => true,
                    (Some(a), Some(b)) => a.nodes == b.nodes && a.reached == b.reached && a.target == b.target && a.dist_to_target == b.dist_to_target,
                    _ => false,
                };
                if !same {
                    dev.push(format!("{name} scene {s} q {q} from ({x},{z}) to ({tx},{tz}): finder {:?} / own {:?}", a.as_ref().map(|p| (p.nodes.len(), p.reached)), b.as_ref().map(|p| (p.nodes.len(), p.reached))));
                }
            }
        }
    }
    assert!(total > 0);
    assert!(dev.is_empty(), "{} of {total} searches differ:\n{}", dev.len(), dev.iter().take(8).cloned().collect::<Vec<_>>().join("\n"));
}
