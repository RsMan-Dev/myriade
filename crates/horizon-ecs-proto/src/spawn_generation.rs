//! `NaturalSpawner.spawnMobsForChunkGeneration`: the creature packs of a chunk the generator has just made. The packs and their positions
//! are drawn with the decoration random (the world seed and the chunk's corner); the rules and the finalize draw from the region's
//! random, the jar's `level.getRandom()`. The biome is read at the chunk's corner column, at the top of the level.
use crate::cow::{box_at, SpeciesTable};
use crate::spawn_biome::biome_at;
use crate::spawn_finalize::Group;
use crate::spawn_natural::born_finalized_with;
use crate::spawn_reason::SpawnReason;
use crate::spawning::{place_ok, rule_ok, spawn_def, Category, Entry, Spawner};
use bevy_ecs::prelude::*;
use myriade_engine_lib::mobs::data::{PathComputationType, Placement};
use myriade_engine_lib::mobs::spawn_rules::SpawnLevel;
use myriade_engine_lib::mobs::spawn_place::{heightmap_blocks, Heightmap};
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;
use horizon_core::random::{LegacyRandom, RandomSource, WorldgenRandom};

/// The creature packs of the chunk `(cx, cz)`, placed into `w`: each creature is born and finalized as it is placed. `biome` is the
/// generator's own biome at the corner column (the region always has its neighbours); `None` reads the loaded chunks.
pub fn spawn_chunk_packs(w: &mut World, mw: &MobWorld<'_>, cx: i32, cz: i32, region: &mut impl RandomSource, biome: Option<u16>) {
    // `NaturalSpawner.spawnMobsForChunkGeneration`: nothing with the `spawn_mobs` rule off.
    if w.get_resource::<crate::spawn_env::SpawnEnv>().is_some_and(|e| !e.spawn_mobs) {
        return;
    }
    let Some(spawner) = w.get_resource::<Spawner>() else { return };
    let (zoom, max_y) = (spawner.zoom, mw.world.shape.max_y() - 1);
    let (xo, zo) = (cx * 16, cz * 16);
    let biome = biome.unwrap_or_else(|| biome_at(mw, zoom, xo, max_y, zo));
    let list = spawner.tables[biome as usize].lists[Category::Creature as usize].clone();
    if list.is_empty() {
        return;
    }
    let info = mw.biomes.get(biome);
    let biome_name = info.name.as_str();
    let probability = info.creature_probability;
    let mut random = WorldgenRandom::new(LegacyRandom::new(0));
    random.set_decoration_seed(mw.world_seed(), xo, zo);
    while random.next_float() < probability {
        let Some(entry) = pick(&list, &mut random) else { continue };
        let count = sample(entry, &mut random);
        let Some(sp) = w.resource::<SpeciesTable>().0.get(entry.species as usize).copied() else { continue };
        let Some(def) = spawn_def(&sp) else { continue };
        // `spawnGroupData` is null at each pack.
        let mut group: Option<Group> = None;
        let mut x = xo + random.next_int_bound(16).unwrap_or(0);
        let mut z = zo + random.next_int_bound(16).unwrap_or(0);
        let (start_x, start_z) = (x, z);
        for _ in 0..count {
            let mut success = false;
            let mut attempts = 0;
            while !success && attempts < 4 {
                attempts += 1;
                let [px, py, pz] = top_non_colliding(mw, &sp.name, x, z);
                // `false` where the jar skips the moves of the column (its `goto` past them).
                let mut moved = true;
                if place_ok(mw, def, px, py, pz) {
                    let fx = f64::from(x).clamp(f64::from(xo) + f64::from(sp.width), f64::from(xo) + 16.0 - f64::from(sp.width));
                    let fz = f64::from(z).clamp(f64::from(zo) + f64::from(sp.width), f64::from(zo) + 16.0 - f64::from(sp.width));
                    let fy = f64::from(py);
                    let (bx, bz) = (fx.floor() as i32, fz.floor() as i32);
                    if !mw.no_collision(box_at(Vec3::new(fx, fy, fz), sp.width, sp.height)) || !rule_ok(mw, biome_name, def, region, bx, py, bz) {
                        moved = false;
                    } else {
                        // The yaw of `snapTo`: drawn here as the jar does; `born` takes no yaw yet.
                        random.next_float();
                        let probe = box_at(Vec3::new(fx, fy, fz), sp.width, sp.height);
                        // `Mob.checkSpawnObstruction`: no liquid in the box unless the species lives in it; `WorldGenRegion.getEntities` is empty,
                        // so no creature of the pack is ever "in the way" while a chunk is generated.
                        if def.liquid || !mw.contains_any_liquid(probe) {
                            let seed = region.next_long() as u64;
                            success = born_finalized_with(w, mw, &sp, [fx, fy, fz], seed, f64::INFINITY, SpawnReason::ChunkGeneration, &mut group, region).is_some();
                        }
                    }
                }
                if moved {
                    x += random.next_int_bound(5).unwrap_or(0) - random.next_int_bound(5).unwrap_or(0);
                    z += random.next_int_bound(5).unwrap_or(0) - random.next_int_bound(5).unwrap_or(0);
                    while x < xo || x >= xo + 16 || z < zo || z >= zo + 16 {
                        x = start_x + random.next_int_bound(5).unwrap_or(0) - random.next_int_bound(5).unwrap_or(0);
                        z = start_z + random.next_int_bound(5).unwrap_or(0) - random.next_int_bound(5).unwrap_or(0);
                    }
                }
            }
        }
    }
}
/// `WeightedList.getRandom`: a draw below the total weight, then the entry it lands in; None when the total is not positive.
fn pick<'a>(list: &'a [Entry], random: &mut impl RandomSource) -> Option<&'a Entry> {
    let total: i32 = list.iter().map(|e| e.weight).sum();
    if total <= 0 {
        return None;
    }
    let mut r = random.next_int_bound(total).ok()?;
    list.iter().find(|e| {
        if r < e.weight {
            true
        } else {
            r -= e.weight;
            false
        }
    })
}
/// The count of a pack: its constant, or uniform on its range.
fn sample(e: &Entry, random: &mut impl RandomSource) -> u32 {
    if e.constant {
        e.min
    } else {
        e.min + random.next_int_bound((e.max.saturating_sub(e.min) + 1) as i32).unwrap_or(0) as u32
    }
}
/// `getTopNonCollidingPos`: the type's heightmap, under the ceiling in dimensions that have one, then the placement type's adjustment.
fn top_non_colliding(mw: &MobWorld<'_>, entity: &str, x: i32, z: i32) -> [i32; 3] {
    let kind = Heightmap::from_name(mw.mob_data().heightmap(entity));
    let (min_y, max_y) = (mw.world.shape.min_y, mw.world.shape.max_y() - 1);
    let mut y = if kind == Heightmap::WorldSurface {
        SpawnLevel::world_surface(mw, x, z).max(min_y)
    } else {
        (min_y..=max_y).rev().find(|&y| heightmap_blocks(mw, kind, x, y, z)).map_or(min_y, |y| y + 1)
    };
    let air = |y: i32| SpawnLevel::facts(mw).is_air(SpawnLevel::state(mw, x, y, z));
    if mw.world.shape.has_ceiling {
        loop {
            y -= 1;
            if air(y) {
                break;
            }
        }
        loop {
            y -= 1;
            if !(air(y) && y > min_y) {
                break;
            }
        }
    }
    if mw.mob_data().placement(entity) == Placement::OnGround && mw.data.pathfindable(SpawnLevel::state(mw, x, y - 1, z), PathComputationType::Land) {
        y -= 1;
    }
    [x, y, z]
}
