//! Worlds and call lists for the block-collision kernel alone (the design notes): the cases of a
//! cow, and a varied random world with random calls for the equality test of the optimised kernel.
use horizon_client_lib::mobs::MobWorld;
use horizon_client_render::{chunk::Chunk, collisions::SceneCollisions};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_world::registry::{BlockRegistry, BlockStateId};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.unit()
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// One call of the kernel.
#[derive(Clone, Copy)]
pub struct Call {
    pub bounds: Aabb,
    pub movement: Vec3,
    pub step: f32,
    pub on_ground: bool,
}

const VARIED: &[&str] = &[
    "stone", "dirt", "oak_slab", "stone_slab", "oak_stairs", "stone_stairs", "oak_fence", "cobblestone_wall",
    "snow", "oak_fence_gate", "oak_trapdoor", "white_carpet", "powder_snow", "scaffolding", "water", "lava",
    "cobweb", "glass_pane", "iron_bars", "lily_pad", "soul_sand", "honey_block", "cactus", "farmland", "dirt_path",
    "chest", "white_bed", "anvil", "hopper", "cauldron", "brewing_stand", "end_portal_frame", "short_grass",
    "torch", "ladder", "vine", "sea_pickle", "turtle_egg", "lantern", "pointed_dripstone", "flower_pot", "piston",
    "sticky_piston", "piston_head", "moving_piston", "shulker_box", "composter", "big_dripleaf", "rail",
    "stone_button", "candle", "cake", "end_rod", "chain", "bell", "grindstone", "stonecutter", "lectern",
    "enchanting_table", "daylight_detector", "ice", "slime_block", "oak_door", "oak_leaves", "amethyst_cluster",
    "dragon_egg", "sculk_sensor", "campfire", "conduit", "heavy_core", "mud", "chorus_plant", "chorus_flower",
    "oak_sign", "oak_pressure_plate", "tripwire", "snow_block", "bamboo", "azalea", "moss_carpet", "creaking_heart",
];

/// The states of the names above, per name.
fn varied_states(registry: &BlockRegistry) -> Vec<Vec<BlockStateId>> {
    let mut by_name: Vec<Vec<BlockStateId>> = vec![Vec::new(); VARIED.len()];
    for i in 0..registry.len() {
        let id = BlockStateId(i as u32);
        let name = &registry.state(id).unwrap().name;
        if let Some(k) = VARIED.iter().position(|v| name.strip_prefix("minecraft:") == Some(v)) {
            by_name[k].push(id);
        }
    }
    by_name.retain(|v| !v.is_empty());
    by_name
}

pub fn leaked(build: impl FnOnce(&mut horizon_client_render::world::World, &BlockRegistry)) -> &'static MobWorld<'static> {
    horizon_client_lib::mobs::world::leaked_test_world(true, build).expect("HORIZON_TEST_RESOURCES").0
}

fn insert_chunks(w: &mut horizon_client_render::world::World, c0: i32, c1: i32) {
    for cx in c0..c1 {
        for cz in c0..c1 {
            w.chunks.insert((cx, cz), Arc::new(Chunk::empty(cx, cz)));
        }
    }
}

/// Bench world: a stone floor at y = 0 over x, z in [-48, 48), a wall x = 20 (y 1..=3) and, for x in [24, 44),
/// z in [-20, 20), a field of random non-cubic blocks at y = 1 (and some at y = 2).
pub fn bench_world_mut() -> (&'static MobWorld<'static>, *mut horizon_client_render::world::World) {
    let (w, _, p) = horizon_client_lib::mobs::world::leaked_test_world_mut(true, |w, registry| {
        insert_chunks(w, -3, 3);
        let stone = registry.default_state("minecraft:stone").unwrap();
        let states = varied_states(registry);
        let mut r = Rng(0x9E37_79B9_7F4A_7C15);
        for x in -48..48 {
            for z in -48..48 {
                w.set(x, 0, z, stone).unwrap();
            }
        }
        for z in -48..48 {
            for y in 1..=3 {
                w.set(20, y, z, stone).unwrap();
            }
        }
        for x in 24..44 {
            for z in -20..20 {
                if r.below(3) == 0 {
                    continue;
                }
                let k = r.below(states.len());
                let s = states[k][r.below(states[k].len())];
                w.set(x, 1, z, s).unwrap();
                if r.below(8) == 0 {
                    let k = r.below(states.len());
                    w.set(x, 2, z, states[k][r.below(states[k].len())]).unwrap();
                }
            }
        }
        // `KB_HARD=name:N`: a patch (x in [30, 48), z in [-48, -20)) where the block `name` fills each cell of the two layers above the floor
        // with the odds that make N of them lie within a cow's box and its ring (32 cells): the case `h-hard`.
        if let Ok(spec) = std::env::var("KB_HARD") {
            let (name, n) = spec.split_once(':').expect("KB_HARD=name:N");
            let hard = states_named(registry, &[name]);
            let p = n.parse::<f64>().expect("N") / 32.0;
            for x in 30..48 {
                for z in -48..-20 {
                    for y in 1..=2 {
                        if r.unit() < p {
                            let k = r.below(hard.len());
                            w.set(x, y, z, hard[k][r.below(hard[k].len())]).unwrap();
                        }
                    }
                }
            }
        }
    })
    .expect("HORIZON_TEST_RESOURCES");
    (w, p)
}

pub fn bench_world() -> &'static MobWorld<'static> {
    bench_world_mut().0
}

pub fn cow_box(x: f64, y: f64, z: f64) -> Aabb {
    Aabb::new(Vec3::new(x - 0.45, y, z - 0.45), Vec3::new(x + 0.45, y + 1.4, z + 0.45))
}

pub const CASES: [&str; 9] = ["z-null", "a-rest", "b-walk", "c-wall", "d-fall", "e-furniture", "f-air", "g-churn", "h-hard"];

/// The calls of one cow case (a few hundred, cycled by the bench).
pub fn case_calls(case: &str, n: usize) -> Vec<Call> {
    let mut r = Rng(0x1234_5678_9ABC_DEF1);
    let g = -0.08 * f64::from(0.98_f32);
    (0..n)
        .map(|_| {
            let (x, z) = (r.range(-40.0, 10.0), r.range(-40.0, 40.0));
            let walk = Vec3::new(r.range(-0.12, 0.12), g, r.range(-0.12, 0.12));
            let (bounds, movement, on_ground) = match case {
                "a-rest" => (cow_box(x, 1.0, z), Vec3::new(0.0, g, 0.0), true),
                "b-walk" => (cow_box(x, 1.0, z), walk, true),
                "c-wall" => (cow_box(19.0 - 0.45 - r.range(0.0, 0.3), 1.0, z), Vec3::new(0.12, g, r.range(-0.05, 0.05)), true),
                "z-null" => (cow_box(x, 1.0, z), Vec3::new(0.0, 0.0, 0.0), true),
                "h-hard" => (cow_box(r.range(31.0, 47.0), 1.0, r.range(-47.0, -21.0)), walk, true),
                "g-churn" => (cow_box(r.range(-15.5, -0.5), 1.0, r.range(0.5, 15.5)), walk, true),
                "f-air" => (cow_box(x, r.range(40.0, 60.0), z), Vec3::new(r.range(-0.3, 0.3), r.range(-0.3, 0.3), r.range(-0.3, 0.3)), false),
                "d-fall" => (cow_box(x, r.range(5.0, 30.0), z), Vec3::new(0.0, r.range(-1.2, -0.1), 0.0), false),
                _ => (cow_box(r.range(24.0, 44.0), 1.0, r.range(-20.0, 20.0)), walk, true),
            };
            Call { bounds, movement, step: 0.6, on_ground }
        })
        .collect()
}

/// The masks of a lab world (one per world, kept for the process).
pub fn occupancy_of(world: &MobWorld<'_>) -> &'static crate::occupancy::Occupancy {
    static MAP: std::sync::Mutex<Vec<(usize, &'static crate::occupancy::Occupancy)>> = std::sync::Mutex::new(Vec::new());
    let key = world.world as *const _ as usize;
    let mut m = MAP.lock().unwrap();
    if let Some(&(_, o)) = m.iter().find(|(k, _)| *k == key) {
        return o;
    }
    let o: &'static crate::occupancy::Occupancy = Box::leak(Box::default());
    m.push((key, o));
    o
}

/// [`run`] with the masks already at hand (the walk reaches them through its `Level`).
pub fn run_with(occ: &crate::occupancy::Occupancy, world: &MobWorld<'_>, c: &Call) -> Result<Vec3, String> {
    let scene = SceneCollisions { world: world.world, binding: world.collision, subject: None };
    occ.resolve(scene, c.bounds, c.movement, c.step, c.on_ground).map_err(|e| e.to_string())
}

/// The kernel as the walk calls it: through the occupancy masks (`occupancy.rs`), the kernel itself when they do not decide.
pub fn run(world: &MobWorld<'_>, c: &Call) -> Result<Vec3, String> {
    let scene = SceneCollisions { world: world.world, binding: world.collision, subject: None };
    occupancy_of(world).resolve(scene, c.bounds, c.movement, c.step, c.on_ground).map_err(|e| e.to_string())
}

/// The kernel alone, without the masks.
pub fn run_kernel(world: &MobWorld<'_>, c: &Call) -> Result<Vec3, String> {
    let scene = SceneCollisions { world: world.world, binding: world.collision, subject: None };
    scene
        .resolve_in_border_with(c.bounds, c.movement, c.step, c.on_ground, None, &[])
        .map_err(|e| e.to_string())
}

/// The old kernel (kept under another name), for the equality test.
pub fn run_reference(world: &MobWorld<'_>, c: &Call) -> Result<Vec3, String> {
    let scene = SceneCollisions { world: world.world, binding: world.collision, subject: None };
    scene
        .resolve_in_border_with_reference(c.bounds, c.movement, c.step, c.on_ground, None, &[])
        .map_err(|e| e.to_string())
}

/// A varied random world over x, z in [-32, 32) (chunk (2, 0) and beyond absent) and y in [-2, 12), with sections
/// of different densities, and `n` random calls over it (boxes of several sizes, speeds, steps).
/// The blocks that stress the scan: shapes that reach out of their cell, contextual ones, thin and stepped ones.
const TRICKY: &[&str] = &[
    "oak_fence", "cobblestone_wall", "oak_fence_gate", "scaffolding", "powder_snow", "oak_stairs", "oak_slab",
    "snow", "white_bed", "oak_trapdoor", "white_carpet", "moving_piston", "piston_head", "iron_bars", "anvil",
    "hopper", "cauldron", "lily_pad", "soul_sand", "cactus", "chest", "end_rod", "bamboo", "pointed_dripstone",
];

/// An even seed gives a dense small world of tricky blocks (x, z in [-12, 12), y in [0, 6)); an odd one the wide
/// varied world.
pub fn varied_world(seed: u64) -> &'static MobWorld<'static> {
    if seed % 2 == 0 {
        return leaked(|w, registry| {
            insert_chunks(w, -2, 2);
            let stone = registry.default_state("minecraft:stone").unwrap();
            let pool: Vec<Vec<BlockStateId>> = TRICKY
                .iter()
                .map(|n| {
                    (0..registry.len() as u32)
                        .map(BlockStateId)
                        .filter(|&id| registry.state(id).unwrap().name.strip_prefix("minecraft:") == Some(n))
                        .collect::<Vec<_>>()
                })
                .filter(|v| !v.is_empty())
                .collect();
            let mut r = Rng(seed | 1);
            for x in -12..12 {
                for z in -12..12 {
                    w.set(x, 0, z, stone).unwrap();
                    for y in 1..6 {
                        match r.below(100) {
                            0..=14 => w.set(x, y, z, stone).unwrap(),
                            15..=44 => {
                                let k = r.below(pool.len());
                                w.set(x, y, z, pool[k][r.below(pool[k].len())]).unwrap();
                            }
                            _ => {}
                        }
                    }
                }
            }
        });
    }
    leaked(|w, registry| {
        insert_chunks(w, -2, 2);
        let states = varied_states(registry);
        let stone = registry.default_state("minecraft:stone").unwrap();
        let mut r = Rng(seed | 1);
        for bx in (-32..32).step_by(4) {
            for by in (-4..12).step_by(4) {
                for bz in (-32..32).step_by(4) {
                    let density = [0, 0, 5, 20, 60, 100][r.below(6)];
                    let kind = r.below(3);
                    for x in bx..bx + 4 {
                        for y in by..by + 4 {
                            for z in bz..bz + 4 {
                                if r.below(100) >= density {
                                    continue;
                                }
                                let s = match kind {
                                    0 => stone,
                                    _ => {
                                        let k = r.below(states.len());
                                        states[k][r.below(states[k].len())]
                                    }
                                };
                                w.set(x, y, z, s).unwrap();
                            }
                        }
                    }
                }
            }
        }
    })
}

pub fn random_calls(seed: u64, n: usize) -> Vec<Call> {
    let mut r = Rng(seed ^ 0xABCD_EF01_2345_6789 | 1);
    (0..n)
        .map(|_| {
            let (w, h) = [(0.9, 1.4), (0.6, 1.8), (0.3, 0.3), (1.4, 0.9), (0.6, 0.6), (1.0, 1.0), (0.5, 0.5), (0.75, 1.5)]
                [r.below(8)];
            let (x, y, z) = if seed % 2 == 0 {
                (r.range(-14.0, 14.0), r.range(-0.5, 6.5), r.range(-14.0, 14.0))
            } else {
                (r.range(-34.0, 40.0), r.range(-4.0, 14.0), r.range(-34.0, 34.0))
            };
            // Faces on the 1/8 grid, and a hair (the vanilla thresholds, 1e-7) either side of it, are the ones that
            // stress the comparisons.
            let snap = |v: f64, r: &mut Rng| match r.below(4) {
                0 => (v * 8.0).round() / 8.0,
                1 => (v * 8.0).round() / 8.0 + [-2.0e-7, -1.0e-7, -5.0e-8, 5.0e-8, 1.0e-7, 2.0e-7][r.below(6)],
                _ => v,
            };
            let (x, y, z) = (snap(x, &mut r), snap(y, &mut r), snap(z, &mut r));
            let bounds = Aabb::new(Vec3::new(x - w / 2.0, y, z - w / 2.0), Vec3::new(x + w / 2.0, y + h, z + w / 2.0));
            let scale = [0.05, 0.2, 0.5, 2.0, 8.0][r.below(5)];
            let axis_only = r.below(3);
            let mut m = Vec3::new(r.range(-scale, scale), r.range(-scale, scale), r.range(-scale, scale));
            if axis_only == 0 {
                m = Vec3::new(0.0, -r.range(0.0, 0.08), 0.0);
            } else if axis_only == 1 && r.below(2) == 0 {
                m = Vec3::new(m.x, -0.0784, m.z);
            }
            if r.below(10) == 0 {
                m = Vec3::new(0.0, m.y, 0.0);
            }
            let step = [0.0_f32, 0.6, 1.0][r.below(3)];
            Call { bounds, movement: m, step, on_ground: r.below(2) == 0 }
        })
        .collect()
}

/// A world of plain cubes over x, z in [-14, 14) (y 0 to 6, regions of several densities), with, when `tricky` is set, a
/// few percent of non-cubic blocks; and the pointer that lets a test change blocks between calls (`occupancy.rs`).
pub fn cube_world(seed: u64, tricky: bool) -> (&'static MobWorld<'static>, *mut horizon_client_render::world::World) {
    let (w, _, p) = horizon_client_lib::mobs::world::leaked_test_world_mut(true, |w, registry| {
        insert_chunks(w, -2, 2);
        let cubes: Vec<BlockStateId> = ["stone", "dirt", "cobblestone", "oak_planks", "glass", "ice", "sand", "gravel"]
            .iter()
            .map(|n| registry.default_state(&format!("minecraft:{n}")).unwrap())
            .collect();
        let odd = varied_states(registry);
        let mut r = Rng(seed | 1);
        for bx in (-16..16).step_by(4) {
            for by in (0..8).step_by(2) {
                for bz in (-16..16).step_by(4) {
                    let density = [0, 3, 10, 30, 60, 100][r.below(6)];
                    for x in bx..bx + 4 {
                        for y in by..by + 2 {
                            for z in bz..bz + 4 {
                                if r.below(100) >= density {
                                    continue;
                                }
                                if tricky && r.below(25) == 0 {
                                    let k = r.below(odd.len());
                                    w.set(x, y, z, odd[k][r.below(odd[k].len())]).unwrap();
                                } else {
                                    w.set(x, y, z, cubes[r.below(cubes.len())]).unwrap();
                                }
                            }
                        }
                    }
                }
            }
        }
        let stone = registry.default_state("minecraft:stone").unwrap();
        for x in -16..16 {
            for z in -16..16 {
                w.set(x, 0, z, stone).unwrap();
            }
        }
    })
    .expect("HORIZON_TEST_RESOURCES");
    (w, p)
}

/// Changes one block of a lab world as the herd does (the block, then the mask of its section).
///
/// # Safety
/// No call of the kernel may run at the same time.
pub unsafe fn change_block(world: &MobWorld<'_>, ptr: *mut horizon_client_render::world::World, x: i32, y: i32, z: i32, state: BlockStateId) {
    unsafe {
        let _ = (*ptr).set(x, y, z, state);
    }
    occupancy_of(world).changed(world.world, world.collision, x, y, z);
}

/// [`run`] and [`run_reference`] for a body that walks on lava (`subject`: the kernel's contextual liquid shapes apply).
pub fn run_lava_walker(world: &MobWorld<'_>, c: &Call, reference: bool) -> Result<Vec3, String> {
    let subject = Some(horizon_client_render::collisions::PowderSnowSubject { lava_walker: true, ..Default::default() });
    let scene = SceneCollisions { world: world.world, binding: world.collision, subject };
    if reference {
        scene.resolve_in_border_with_reference(c.bounds, c.movement, c.step, c.on_ground, None, &[]).map_err(|e| e.to_string())
    } else {
        occupancy_of(world).resolve(scene, c.bounds, c.movement, c.step, c.on_ground).map_err(|e| e.to_string())
    }
}

/// How a call is answered: through the masks, by the kernel alone, or by the reference (cell by cell) kernel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Masks,
    Kernel,
    Reference,
}

/// [`run`] for a body with the given subject (the entity whose context the kernel's contextual shapes read).
pub fn run_as(world: &MobWorld<'_>, c: &Call, subject: Option<horizon_client_render::collisions::PowderSnowSubject<'_>>, mode: Mode) -> Result<Vec3, String> {
    let scene = SceneCollisions { world: world.world, binding: world.collision, subject };
    match mode {
        Mode::Masks => occupancy_of(world).resolve(scene, c.bounds, c.movement, c.step, c.on_ground),
        Mode::Kernel => scene.resolve_in_border_with(c.bounds, c.movement, c.step, c.on_ground, None, &[]),
        Mode::Reference => scene.resolve_in_border_with_reference(c.bounds, c.movement, c.step, c.on_ground, None, &[]),
    }
    .map_err(|e| e.to_string())
}

/// The subjects the contextual shapes tell apart: none (a position), a living entity with nothing special, one that fell,
/// one in leather boots, a walkable type (a fox, a rabbit), a falling block, a lava walker.
pub fn subjects() -> Vec<Option<horizon_client_render::collisions::PowderSnowSubject<'static>>> {
    use horizon_client_render::collisions::PowderSnowSubject as S;
    vec![
        None,
        Some(S::default()),
        Some(S { fall_distance: 3.0, ..Default::default() }),
        Some(S { leather_boots: true, ..Default::default() }),
        Some(S { walkable_type: true, ..Default::default() }),
        Some(S { falling_block: true, ..Default::default() }),
        Some(S { lava_walker: true, ..Default::default() }),
    ]
}

/// The blocks the masks leave to the kernel cell by cell (an entity context, a block entity, a piston in motion, a column offset).
pub const HARD: &[&str] = &["scaffolding", "powder_snow", "moving_piston", "bamboo", "pointed_dripstone", "sulfur_spike", "water", "lava", "shulker_box", "red_shulker_box", "lime_shulker_box"];
const FIXED: &[&str] = &["oak_slab", "oak_stairs", "oak_fence", "cobblestone_wall", "snow", "white_bed", "anvil", "hopper", "cauldron", "oak_trapdoor", "piston_head"];

fn states_named(registry: &BlockRegistry, names: &[&str]) -> Vec<Vec<BlockStateId>> {
    names
        .iter()
        .map(|n| (0..registry.len() as u32).map(BlockStateId).filter(|&id| registry.state(id).unwrap().name.strip_prefix("minecraft:") == Some(n)).collect::<Vec<_>>())
        .filter(|v| !v.is_empty())
        .collect()
}

/// A dense small world (x, z in [-14, 14), y in [0, 6)) of stone, fixed shapes (slabs, stairs, fences, walls, snow, beds...) and
/// hard blocks (`only`: that one name, every column of the layer y = 1 holds one; None: all of `HARD`), with the lids of the
/// shulker boxes and the pistons in motion the world keeps for them. The pointer lets a test change blocks.
pub fn hard_world(seed: u64, only: Option<&str>) -> (&'static MobWorld<'static>, *mut horizon_client_render::world::World) {
    hard_world_sized(seed, only, 14)
}

/// [`hard_world`] over x, z in [-half, half) (half at most 32).
pub fn hard_world_sized(seed: u64, only: Option<&str>, half: i32) -> (&'static MobWorld<'static>, *mut horizon_client_render::world::World) {
    let (w, _, p) = horizon_client_lib::mobs::world::leaked_test_world_mut(true, |w, registry| {
        insert_chunks(w, -2, 2);
        let stone = registry.default_state("minecraft:stone").unwrap();
        let fixed = states_named(registry, FIXED);
        let hard = match only {
            Some(n) => states_named(registry, &[n]),
            None => states_named(registry, HARD),
        };
        assert!(!hard.is_empty(), "no state named {only:?}");
        let mut r = Rng(seed | 1);
        for x in -half..half {
            for z in -half..half {
                w.set(x, 0, z, stone).unwrap();
                for y in 1..6 {
                    let kind = if only.is_some() && y == 1 { 99 } else { r.below(100) };
                    let s = match kind {
                        0..=11 => stone,
                        12..=26 => {
                            let k = r.below(fixed.len());
                            fixed[k][r.below(fixed[k].len())]
                        }
                        27..=69 | 99 => {
                            let k = r.below(hard.len());
                            hard[k][r.below(hard[k].len())]
                        }
                        _ => continue,
                    };
                    w.set(x, y, z, s).unwrap();
                    let name = registry.state(s).unwrap().name.as_str();
                    if name.ends_with("shulker_box") {
                        use horizon_client_render::block_entity_anim::{Shulker, ShulkerStatus};
                        let progress = [0.0f32, 0.25, 0.5, 0.75, 1.0][r.below(5)];
                        if r.below(4) != 0 {
                            w.shulkers.insert([x, y, z], Shulker { open_count: 1, status: ShulkerStatus::Opening, progress, progress_old: progress * 0.5 });
                        }
                    } else if name == "minecraft:moving_piston" && r.below(3) != 0 {
                        let moved = fixed[r.below(fixed.len())][0];
                        let direction = horizon_core::geometry::Direction::ALL[r.below(6)];
                        let progress = r.below(5) as f32 / 4.0;
                        w.moving_pistons.push(([x, y, z], horizon_world::piston_shape::MovingPistonState { moved_state: moved, direction, extending: r.below(2) == 0, source: r.below(2) == 0, progress, previous_progress: progress * 0.5 }));
                    }
                }
            }
        }
    })
    .expect("HORIZON_TEST_RESOURCES");
    (w, p)
}

/// A stone floor at y = 0 and, above it at y = 1, a floor of open shulker boxes (lids at full progress) over x, z in [-14, 14); air above.
pub fn shulker_floor_world() -> (&'static MobWorld<'static>, *mut horizon_client_render::world::World) {
    let (w, _, p) = horizon_client_lib::mobs::world::leaked_test_world_mut(true, |w, registry| {
        insert_chunks(w, -2, 2);
        let stone = registry.default_state("minecraft:stone").unwrap();
        let shulker = registry.default_state("minecraft:shulker_box").unwrap();
        for x in -14..14 {
            for z in -14..14 {
                w.set(x, 0, z, stone).unwrap();
                w.set(x, 1, z, shulker).unwrap();
                use horizon_client_render::block_entity_anim::{Shulker, ShulkerStatus};
                w.shulkers.insert([x, 1, z], Shulker { open_count: 1, status: ShulkerStatus::Opening, progress: 1.0, progress_old: 1.0 });
            }
        }
    })
    .expect("HORIZON_TEST_RESOURCES");
    (w, p)
}

/// A world whose features lie on the seams between sections (x, z = -1 | 0 and 15 | 16, y = 15 | 16): plain cubes, shapes that reach out of their
/// cell (fences, walls, pistons' heads), fixed shapes and hard blocks, so that boxes near a seam read cells of the neighbouring sections. Returns the
/// world and the calls (boxes standing on the floor at y = 16, or inside the features).
pub fn seam_world(seed: u64) -> (&'static MobWorld<'static>, Vec<Call>) {
    let (w, _, _) = horizon_client_lib::mobs::world::leaked_test_world_mut(true, |w, registry| {
        insert_chunks(w, -2, 2);
        let stone = registry.default_state("minecraft:stone").unwrap();
        let pool = states_named(registry, &["oak_fence", "cobblestone_wall", "piston_head", "oak_slab", "oak_stairs", "scaffolding", "bamboo", "snow", "iron_bars", "oak_fence_gate", "moving_piston"]);
        let mut r = Rng(seed | 1);
        for x in -20..20 {
            for z in -20..20 {
                // The floor is not whole: shapes taller than their cell stand at y = 15 and reach into the section above.
                match r.below(100) {
                    0..=69 => w.set(x, 15, z, stone).unwrap(),
                    70..=84 => {
                        let k = r.below(pool.len());
                        w.set(x, 15, z, pool[k][r.below(pool[k].len())]).unwrap();
                    }
                    _ => {}
                }
            }
        }
        let near_seam = |v: i32| matches!(v, -2..=1 | 14..=17);
        for x in -20..20i32 {
            for z in -20..20i32 {
                for y in 14..19 {
                    let (nx, nz, ny) = (near_seam(x), near_seam(z), matches!(y, 14..=17));
                    if !(nx || nz || ny) || y == 15 {
                        continue;
                    }
                    match r.below(100) {
                        0..=9 => w.set(x, y, z, stone).unwrap(),
                        10..=21 => {
                            let k = r.below(pool.len());
                            w.set(x, y, z, pool[k][r.below(pool[k].len())]).unwrap();
                        }
                        _ => {}
                    }
                }
            }
        }
    })
    .expect("HORIZON_TEST_RESOURCES");
    let mut r = Rng(seed ^ 0x5EA3);
    let calls = (0..20_000)
        .map(|_| {
            let (bw, bh) = [(0.9, 1.4), (0.6, 1.8), (0.3, 0.3), (0.6, 0.6)][r.below(4)];
            let seam = |r: &mut Rng| [-1.0, 0.0, 15.0, 16.0][r.below(4)] + r.range(-1.6, 1.6);
            let (x, z) = (seam(&mut r), seam(&mut r));
            let y = [16.0, 15.0, 16.5, 17.0, 14.0][r.below(5)] + [0.0, 1.0e-7, -1.0e-7][r.below(3)];
            let bounds = Aabb::new(Vec3::new(x - bw / 2.0, y, z - bw / 2.0), Vec3::new(x + bw / 2.0, y + bh, z + bw / 2.0));
            let m = [Vec3::new(0.0, -0.0784, 0.0), Vec3::new(0.12, -0.08, 0.05), Vec3::new(-0.1, 0.0, 0.1), Vec3::new(0.3, 0.2, -0.3), Vec3::new(0.0, 0.0, 0.0)][r.below(5)];
            Call { bounds, movement: m, step: [0.0_f32, 0.6, 1.0][r.below(3)], on_ground: r.below(2) == 0 }
        })
        .collect();
    (w, calls)
}
