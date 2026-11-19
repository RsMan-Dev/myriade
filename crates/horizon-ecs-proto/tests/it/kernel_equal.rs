//! The optimised block-collision kernel against the reference one, bit for bit, over random calls in a varied
//! random world (suivi/ecs-noyau-collisions.md). `HORIZON_TEST_RESOURCES` must be set.
use horizon_ecs_proto::kernel_lab::{change_block, cube_world, hard_world, hard_world_sized, occupancy_of, random_calls, run, run_as, run_kernel, run_lava_walker, run_reference, subjects, varied_world, Call, Mode, Rng};

fn same(a: &Result<horizon_core::geometry::Vec3, String>, b: &Result<horizon_core::geometry::Vec3, String>) -> bool {
    match (a, b) {
        (Ok(a), Ok(b)) => [a.x, a.y, a.z].map(f64::to_bits) == [b.x, b.y, b.z].map(f64::to_bits),
        (Err(a), Err(b)) => a == b,
        _ => false,
    }
}

#[test]
fn the_fast_kernel_equals_the_reference_bit_for_bit() {
    let mut calls = 0;
    let mut moved = 0;
    for seed in 1..=6u64 {
        let world = varied_world(seed);
        for c in random_calls(seed, 12_000) {
            let (new, old) = (run(world, &c), run_reference(world, &c));
            assert!(
                same(&new, &old),
                "seed {seed}: {:?} {:?} step {} ground {}: new {new:?} old {old:?}",
                c.bounds, c.movement, c.step, c.on_ground
            );
            calls += 1;
            moved += usize::from(old.as_ref().is_ok_and(|v| v.x != c.movement.x || v.y != c.movement.y || v.z != c.movement.z));
        }
        let o = occupancy_of(world);
        eprintln!("seed {seed}: masks answered {} calls, kernel {}, states classified {:?}", o.fast.load(std::sync::atomic::Ordering::Relaxed), o.fallback.load(std::sync::atomic::Ordering::Relaxed), o.table_size());
    }
    // The cases must actually collide, or the equality says nothing.
    assert!(moved * 5 > calls, "only {moved} of {calls} calls were stopped by a block");
}

/// The flag of the unit-cube states, state by state: set exactly for a state whose cached shape is the unit cube and
/// that has no contextual rule and no entity-kind variants; and the states that are the unit cube but not plain (the
/// ones the flag must keep out) are listed, so that the equality test's worlds can hold them.
#[test]
fn the_plain_cube_flag_is_right_for_every_state() {
    use horizon_world::registry::BlockStateId;
    let mw = varied_world(1);
    let binding = mw.collision;
    let (mut plain, mut cube_but_contextual) = (0, Vec::new());
    for i in 0..mw.registry.len() {
        let id = BlockStateId(i as u32);
        let cube = binding.cached_shapes().cached_shape(id).unwrap().is_some_and(horizon_core::geometry::VoxelShape::is_block);
        let ctx = binding.has_context_rule(id);
        assert_eq!(binding.is_plain_cube(id), cube && !ctx, "state {i}");
        plain += usize::from(binding.is_plain_cube(id));
        if cube && ctx {
            cube_but_contextual.push(mw.registry.state(id).unwrap().name.clone());
        }
    }
    eprintln!("{plain} plain cube states; unit cubes with a contextual rule: {cube_but_contextual:?}");
    assert!(plain > 100);
}

/// The masks answer for plain-cube worlds (and decline, the kernel running unchanged, where a block that is not a plain cube is
/// near): equal to the reference bit for bit, the fast path taken for most calls of the pure-cube worlds, and still equal after
/// blocks change (the masks of their sections rebuilt).
#[test]
fn the_occupancy_masks_equal_the_reference_in_cube_worlds_and_after_changes() {
    use std::sync::atomic::Ordering::Relaxed;
    let (mut calls, mut fast_pure) = (0u64, 0u64);
    for (seed, tricky) in [(2u64, false), (4, false), (6, true), (8, true), (10, false)] {
        let (world, ptr) = cube_world(seed, tricky);
        let occ = occupancy_of(world);
        let states: Vec<_> = ["stone", "air", "oak_fence", "glass", "oak_slab", "dirt"]
            .iter()
            .map(|n| world.registry.default_state(&format!("minecraft:{n}")).unwrap())
            .collect();
        let mut r = Rng(seed * 77 + 5);
        for round in 0..12 {
            let before = occ.fast.load(Relaxed);
            let list = random_calls(seed, 2_000);
            for c in &list {
                let (new, old, kernel) = (run(world, c), run_reference(world, c), run_kernel(world, c));
                assert!(same(&new, &old), "seed {seed} round {round}: {:?} {:?} step {} ground {}: masks {new:?} reference {old:?}", c.bounds, c.movement, c.step, c.on_ground);
                assert!(same(&kernel, &old));
                let scene = horizon_client_render::collisions::SceneCollisions { world: world.world, binding: world.collision, subject: None };
                let mut scene2 = horizon_client_render::collisions::SceneCollisions { world: world.world, binding: world.collision, subject: None };
                assert_eq!(occ.no_collision(scene, c.bounds), scene2.unobstructed(c.bounds).unwrap_or(false), "no_collision {:?}", c.bounds);
                calls += 1;
            }
            if !tricky {
                fast_pure += occ.fast.load(Relaxed) - before;
            }
            // Blocks change between rounds: a mask kept past a change would answer for the old world.
            for _ in 0..60 {
                let (x, y, z) = (r.below(28) as i32 - 14, r.below(7) as i32, r.below(28) as i32 - 14);
                let s = states[r.below(states.len())];
                unsafe { change_block(world, ptr, x, y, z, s) };
            }
        }
    }
    assert!(fast_pure * 2 > calls / 2, "the fast path answered only {fast_pure} of the pure-cube calls");
}

/// The table's classes by state name: the states of a shape that depends on the entity, the world or a piston in motion stay with
/// the kernel; the fixed shapes (slabs, stairs, fences, walls, doors, beds, snow layers, anvils...) are in the table.
#[test]
fn the_shape_table_keeps_only_contextual_dynamic_and_moving_states_with_the_kernel() {
    use horizon_world::registry::BlockStateId;
    let mw = varied_world(1);
    let occ = occupancy_of(mw);
    let mut by_class: std::collections::BTreeMap<&str, std::collections::BTreeSet<String>> = Default::default();
    for i in 0..mw.registry.len() {
        let id = BlockStateId(i as u32);
        let name = mw.registry.state(id).unwrap().name.trim_start_matches("minecraft:").to_string();
        by_class.entry(occ.class_name(mw.collision, id)).or_default().insert(name);
    }
    let has = |class: &str, name: &str| by_class.get(class).is_some_and(|s| s.contains(name));
    for name in ["scaffolding", "powder_snow", "moving_piston"] {
        assert!(has("hard", name), "{name} must stay with the kernel");
    }
    for name in ["oak_slab", "oak_stairs", "snow", "white_bed", "anvil", "oak_door", "hopper", "cauldron"] {
        assert!(has("shaped", name) || has("large shaped", name), "{name} must be in the table");
    }
    for name in ["oak_fence", "cobblestone_wall"] {
        assert!(has("large shaped", name), "{name} reaches out of its cell");
    }
    for (c, names) in &by_class {
        eprintln!("{c}: {} names{}", names.len(), if *c == "hard" { format!(" {names:?}") } else { String::new() });
    }
}

/// Water and lava are empty for a body with no subject and the kernel's business for one that stands on fluids (a strider): both
/// equal the reference bit for bit over liquid-rich worlds and over calls built on the liquid cells.
#[test]
fn liquids_collide_for_a_lava_walker_and_not_for_the_others_as_the_reference_says() {
    for seed in [1u64, 3, 5] {
        let world = varied_world(seed);
        for c in random_calls(seed, 6_000) {
            let (a, b) = (run_lava_walker(world, &c, false), run_lava_walker(world, &c, true));
            assert!(same(&a, &b), "seed {seed} lava walker: {:?} {:?}: masks {a:?} reference {b:?}", c.bounds, c.movement);
            let (n, o) = (run(world, &c), run_reference(world, &c));
            assert!(same(&n, &o));
        }
    }
    // Calls built over the liquid cells themselves: a box standing just above one, falling or walking.
    let world = varied_world(3);
    let (mut over, mut stopped_w, mut stopped_o) = (0, 0, 0);
    for x in -32..32 {
        for z in -32..32 {
            for y in -4..12 {
                let name = &world.registry.state(world.world.block(x, y, z)).unwrap().name;
                if name != "minecraft:lava" && name != "minecraft:water" {
                    continue;
                }
                for dy in [-0.5, -0.0784, 0.0] {
                    let (cx, cz) = (f64::from(x) + 0.5, f64::from(z) + 0.5);
                    let bounds = horizon_core::geometry::Aabb::new(
                        horizon_core::geometry::Vec3::new(cx - 0.3, f64::from(y) + 1.0, cz - 0.3),
                        horizon_core::geometry::Vec3::new(cx + 0.3, f64::from(y) + 2.8, cz + 0.3),
                    );
                    let c = horizon_ecs_proto::kernel_lab::Call { bounds, movement: horizon_core::geometry::Vec3::new(0.05, dy, 0.0), step: 0.6, on_ground: false };
                    let (a, b) = (run_lava_walker(world, &c, false), run_lava_walker(world, &c, true));
                    assert!(same(&a, &b), "over liquid: masks {a:?} reference {b:?}");
                    let (n, o) = (run(world, &c), run_reference(world, &c));
                    assert!(same(&n, &o));
                    over += 1;
                    stopped_w += usize::from(b.as_ref().is_ok_and(|v| v.y != dy));
                    stopped_o += usize::from(o.as_ref().is_ok_and(|v| v.y != dy));
                }
            }
        }
    }
    eprintln!("over liquids: {over} calls, a lava walker stopped in {stopped_w}, another body in {stopped_o}");
    assert!(over > 50, "only {over} calls over liquids");
}

/// Why the ring's x-edge cells can be dropped without changing a result (and why a mutation that drops them survives): no fixed shape
/// reaches out of its cell horizontally; a shape that reaches out does so upward (fences, walls: 1.5) or downward, and the ring
/// cells the scan reads for it lie above and below the box. A shape inside its cell in x never meets a box that stays inside the
/// cells of the box (the box's own x cells end where the ring begins).
#[test]
fn no_fixed_shape_reaches_out_of_its_cell_horizontally() {
    use horizon_core::geometry::Axis;
    use horizon_world::registry::BlockStateId;
    let mw = varied_world(1);
    let (mut large, mut vertical_only, mut horizontal) = (0, 0, Vec::new());
    for i in 0..mw.registry.len() {
        let id = BlockStateId(i as u32);
        let b = mw.collision;
        let Ok(Some(shape)) = b.cached_shapes().cached_shape(id) else { continue };
        if shape.is_empty() || b.has_context_rule(id) {
            continue;
        }
        let out = |a: Axis| shape.min(a) < 0.0 || shape.max(a) > 1.0;
        if out(Axis::X) || out(Axis::Z) {
            horizontal.push(mw.registry.state(id).unwrap().name.clone());
        } else if out(Axis::Y) {
            vertical_only += 1;
        }
        large += usize::from(b.reaches_out(id));
    }
    eprintln!("{large} states reach out; {vertical_only} only vertically; horizontally: {horizontal:?}");
    // Only the piston head reaches out sideways (its arm): the ring's x and z cells matter for it, and the next test puts heads there.
    assert!(horizontal.iter().all(|n| n == "minecraft:piston_head"), "shapes that reach out horizontally: {horizontal:?}");
}

/// Piston heads (the one shape that reaches out sideways) scattered through a cube world, among boxes of every size: the masks
/// read the ring cells beside and around the box for them, as the scan does.
#[test]
fn piston_heads_in_the_ring_agree_with_the_reference() {
    use horizon_world::registry::BlockStateId;
    let (world, ptr) = cube_world(12, false);
    let heads: Vec<BlockStateId> = (0..world.registry.len() as u32).map(BlockStateId).filter(|&id| world.registry.state(id).unwrap().name == "minecraft:piston_head").collect();
    let mut r = Rng(0xB15_704);
    for round in 0..8 {
        for _ in 0..40 {
            let (x, y, z) = (r.below(28) as i32 - 14, r.below(7) as i32, r.below(28) as i32 - 14);
            unsafe { change_block(world, ptr, x, y, z, heads[r.below(heads.len())]) };
        }
        for c in random_calls(12, 6_000) {
            let (new, old) = (run(world, &c), run_reference(world, &c));
            assert!(same(&new, &old), "round {round}: {:?} {:?} step {} ground {}: masks {new:?} reference {old:?}", c.bounds, c.movement, c.step, c.on_ground);
        }
    }
}

/// Faces exactly on cell boundaries (touching boxes, half-cell steps, snapped movements) in dense worlds, on the ground and
/// off it, with steps: the calls where a cube that only touches the swept box, or a shape that overlaps by nothing, could change
/// the step heights or the joins.
#[test]
fn touching_faces_agree_with_the_reference() {
    let mut r = Rng(0xFACE_5EED);
    let mut calls = 0;
    for (seed, tricky) in [(2u64, false), (4, true), (6, true)] {
        let (world, _) = cube_world(seed, tricky);
        let occ = occupancy_of(world);
        for _ in 0..40_000 {
            let half = |r: &mut Rng, lo: i32, hi: i32| f64::from(lo + r.below((hi - lo) as usize) as i32) / 2.0;
            let (w, h) = [(0.5, 1.0), (1.0, 1.0), (1.0, 2.0), (0.5, 1.5), (1.0, 1.5)][r.below(5)];
            let (x, y, z) = (half(&mut r, -24, 24), half(&mut r, 0, 12), half(&mut r, -24, 24));
            let bounds = horizon_core::geometry::Aabb::new(horizon_core::geometry::Vec3::new(x, y, z), horizon_core::geometry::Vec3::new(x + w, y + h, z + w));
            let q = |r: &mut Rng| [0.0, 0.25, -0.25, 0.5, -0.5, 1.0, -1.0, 0.0784, -0.0784][r.below(9)];
            let c = horizon_ecs_proto::kernel_lab::Call { bounds, movement: horizon_core::geometry::Vec3::new(q(&mut r), q(&mut r), q(&mut r)), step: [0.0_f32, 0.5, 0.6, 1.0][r.below(4)], on_ground: r.below(2) == 0 };
            let (new, old) = (run(world, &c), run_reference(world, &c));
            assert!(same(&new, &old), "seed {seed}: {:?} {:?} step {} ground {}: masks {new:?} reference {old:?}", c.bounds, c.movement, c.step, c.on_ground);
            calls += 1;
        }
        eprintln!("seed {seed}: masks {} kernel {}", occ.fast.load(std::sync::atomic::Ordering::Relaxed), occ.fallback.load(std::sync::atomic::Ordering::Relaxed));
    }
    assert!(calls > 100_000);
}

/// Why taking a contextual state for a fixed one cannot change a result today (a mutation that does survives): the states with a
/// contextual rule or entity-kind variants (other than water and lava) are exactly those with no cached shape, which the table
/// refuses anyway. A registry that gave one a cached shape would turn this test red, and the table's own check then decides.
#[test]
fn contextual_states_have_no_cached_shape() {
    use horizon_world::registry::BlockStateId;
    let mw = varied_world(1);
    let b = mw.collision;
    let mut with_shape = std::collections::BTreeSet::new();
    let mut without = std::collections::BTreeSet::new();
    for i in 0..mw.registry.len() {
        let id = BlockStateId(i as u32);
        let name = mw.registry.state(id).unwrap().name.clone();
        if !b.has_context_rule(id) || name == "minecraft:water" || name == "minecraft:lava" {
            continue;
        }
        if matches!(b.cached_shapes().cached_shape(id), Ok(Some(_))) { with_shape.insert(name); } else { without.insert(name); }
    }
    eprintln!("contextual states without a cached shape: {without:?}");
    assert!(with_shape.is_empty(), "contextual states with a cached shape: {with_shape:?}");
}

/// Calls built on the hard cells of a world: boxes whose feet are at the heights that tell the contextual shapes apart (on the
/// top of the block, a hair above or below, inside it), standing on it, falling onto it, walking against it.
fn feet_calls(world: &horizon_client_lib::mobs::MobWorld<'_>, r: &mut Rng, half: i32) -> Vec<Call> {
    use horizon_core::geometry::{Aabb, Vec3};
    let mut out = Vec::new();
    for x in -half..half {
        for z in -half..half {
            for y in 0..6 {
                let name = &world.registry.state(world.world.block(x, y, z)).unwrap().name;
                if !HARD_NAMES.iter().any(|n| name.ends_with(n)) || r.below(3) != 0 {
                    continue;
                }
                for feet in [0.0, 0.5, 0.875, 0.9, 0.99, 1.0, 1.0 + 1.0e-7, 1.0 - 1.0e-7, 1.0 + 3.0e-6, 1.125, 1.5, 2.0] {
                    let (cx, cz) = (f64::from(x) + r.range(0.0, 1.0), f64::from(z) + r.range(0.0, 1.0));
                    let (w, h) = [(0.6, 1.8), (0.9, 1.4), (0.3, 0.3), (0.6, 0.6)][r.below(4)];
                    let bounds = Aabb::new(Vec3::new(cx - w / 2.0, f64::from(y) + feet, cz - w / 2.0), Vec3::new(cx + w / 2.0, f64::from(y) + feet + h, cz + w / 2.0));
                    let movement = [Vec3::new(0.0, -0.0784, 0.0), Vec3::new(0.1, -0.08, 0.05), Vec3::new(0.0, -0.5, 0.0), Vec3::new(0.2, 0.0, -0.1), Vec3::new(0.0, 0.0, 0.0)][r.below(5)];
                    out.push(Call { bounds, movement, step: [0.0_f32, 0.6, 1.0][r.below(3)], on_ground: r.below(2) == 0 });
                }
            }
        }
    }
    out
}
const HARD_NAMES: &[&str] = &["scaffolding", "powder_snow", "moving_piston", "bamboo", "pointed_dripstone", "sulfur_spike", "water", "lava", "shulker_box"];

/// Hard cells (scaffolding, powder snow, moving pistons, shulker boxes with their lids, bamboo, stalactites, fluids) among cubes and
/// fixed shapes: the masks ask the kernel for the shape of the cells that need it and rebuild the rest, in the scan's order. Equal to
/// the reference bit for bit for every kind of body (a position, a living entity, one that fell, in leather boots, a walkable type, a
/// falling block, a lava walker), over random calls and calls with the feet at the heights that tell the shapes apart.
#[test]
fn hard_cells_asked_one_by_one_equal_the_reference_bit_for_bit() {
    use std::sync::atomic::Ordering::Relaxed;
    let subs = subjects();
    let (mut calls, mut stopped, mut fast, mut slow, mut errors) = (0u64, 0u64, 0u64, 0u64, 0u64);
    for (seed, only) in [(2u64, None), (4, None), (6, Some("scaffolding")), (8, Some("powder_snow")), (10, Some("moving_piston")), (12, Some("shulker_box")), (14, Some("bamboo")), (16, Some("pointed_dripstone")), (18, Some("water"))] {
        let (world, _) = hard_world(seed, only);
        let occ = occupancy_of(world);
        let mut r = Rng(seed * 31 + 7);
        let mut list = random_calls(seed, 2_500);
        list.extend(feet_calls(world, &mut r, 14));
        let (f0, k0) = (occ.fast.load(Relaxed), occ.fallback.load(Relaxed));
        for c in &list {
            for (i, sub) in subs.iter().enumerate() {
                let (new, old) = (run_as(world, c, *sub, Mode::Masks), run_as(world, c, *sub, Mode::Reference));
                assert!(same(&new, &old), "seed {seed} {only:?} subject {i}: {:?} {:?} step {} ground {}: masks {new:?} reference {old:?}", c.bounds, c.movement, c.step, c.on_ground);
                calls += 1;
                if let Err(e) = &old {
                    errors += 1;
                    if errors == 1 { eprintln!("first reference error: {e}"); }
                }
                stopped += u64::from(old.as_ref().is_ok_and(|v| v.x != c.movement.x || v.y != c.movement.y || v.z != c.movement.z));
            }
        }
        let (f, k) = (occ.fast.load(Relaxed) - f0, occ.fallback.load(Relaxed) - k0);
        fast += f;
        slow += k;
        eprintln!("seed {seed} {only:?}: {} calls x {} subjects; masks {f}, whole kernel {k}", list.len(), subs.len());
    }
    eprintln!("{errors} calls ended in a kernel error (the same one on both sides)");
    eprintln!("{calls} calls, {stopped} stopped by a block; the whole kernel took {slow} ({:.1} %)", slow as f64 * 100.0 / (fast + slow) as f64);
    assert!(stopped * 5 > calls, "only {stopped} of {calls} calls were stopped by a block");
    assert!(slow * 20 < fast + slow, "the whole kernel still took {slow} of {} calls", fast + slow);
}

/// The same for `no_collision` (a position context whatever the body).
#[test]
fn hard_cells_agree_for_no_collision() {
    let subs = subjects();
    for (seed, only) in [(2u64, None), (6, Some("scaffolding")), (10, Some("moving_piston")), (12, Some("shulker_box"))] {
        let (world, _) = hard_world(seed, only);
        let occ = occupancy_of(world);
        for c in random_calls(seed, 6_000) {
            for sub in subs.iter().take(3) {
                let scene = horizon_client_render::collisions::SceneCollisions { world: world.world, binding: world.collision, subject: *sub };
                let mut scene2 = horizon_client_render::collisions::SceneCollisions { world: world.world, binding: world.collision, subject: *sub };
                assert_eq!(occ.no_collision(scene, c.bounds), scene2.unobstructed(c.bounds).unwrap_or(false), "no_collision {:?} seed {seed} {only:?}", c.bounds);
            }
        }
    }
}

/// Bamboo, pointed dripstone and sulfur spikes in every column of a 64 x 64 field (every offset key met), mixed with cubes and fixed
/// shapes, bodies standing on them, falling onto them, walking through them: the table's moved shape per (state, column offset) equals
/// what the kernel gives for each column, bit for bit.
#[test]
fn column_offset_shapes_agree_with_the_kernel_in_every_column() {
    use std::sync::atomic::Ordering::Relaxed;
    let subs = subjects();
    for (seed, name) in [(20u64, "bamboo"), (22, "pointed_dripstone"), (24, "sulfur_spike")] {
        let (world, _) = hard_world_sized(seed, Some(name), 32);
        let occ = occupancy_of(world);
        let mut r = Rng(seed * 17 + 3);
        let mut list: Vec<Call> = Vec::new();
        for _ in 0..6_000 {
            let (w, h) = [(0.6, 1.8), (0.9, 1.4), (0.3, 0.3), (0.6, 0.6)][r.below(4)];
            let (cx, cz) = (r.range(-31.0, 31.0), r.range(-31.0, 31.0));
            let y = [1.0, 1.0, 2.0, 0.0, 3.5][r.below(5)] + [0.0, 1.0e-7, -1.0e-7, 0.5][r.below(4)];
            let bounds = horizon_core::geometry::Aabb::new(horizon_core::geometry::Vec3::new(cx - w / 2.0, y, cz - w / 2.0), horizon_core::geometry::Vec3::new(cx + w / 2.0, y + h, cz + w / 2.0));
            let movement = [horizon_core::geometry::Vec3::new(0.0, -0.0784, 0.0), horizon_core::geometry::Vec3::new(0.1, -0.08, 0.1), horizon_core::geometry::Vec3::new(-0.15, 0.0, 0.0), horizon_core::geometry::Vec3::new(0.0, -0.6, 0.12)][r.below(4)];
            list.push(Call { bounds, movement, step: [0.0_f32, 0.6, 1.0][r.below(3)], on_ground: r.below(2) == 0 });
        }
        list.extend(feet_calls(world, &mut r, 32).into_iter().step_by(3));
        let (f0, k0) = (occ.fast.load(Relaxed), occ.fallback.load(Relaxed));
        let mut stopped = 0;
        for c in &list {
            for sub in subs.iter().take(2) {
                let (new, old) = (run_as(world, c, *sub, Mode::Masks), run_as(world, c, *sub, Mode::Reference));
                assert!(same(&new, &old), "{name}: {:?} {:?} step {} ground {}: masks {new:?} reference {old:?}", c.bounds, c.movement, c.step, c.on_ground);
                stopped += usize::from(old.as_ref().is_ok_and(|v| v.x != c.movement.x || v.y != c.movement.y || v.z != c.movement.z));
            }
        }
        eprintln!("{name}: {} calls x 2 subjects, {stopped} stopped; masks {}, whole kernel {}", list.len(), occ.fast.load(Relaxed) - f0, occ.fallback.load(Relaxed) - k0);
        assert!(stopped * 4 > list.len(), "{name}: only {stopped} stopped");
    }
}

/// What the table relies on: the moved shape of these states depends on the column through two 4-bit values of its position seed and
/// nothing else. Over 300 000 random columns (up to 30 million blocks out), two columns with the same two values give bit-identical
/// shapes, and the 256 values all occur.
#[test]
fn a_column_offset_is_a_function_of_two_seed_nibbles() {
    use horizon_core::{position::BlockPos, random::position_seed};
    use horizon_world::registry::BlockStateId;
    let mw = varied_world(1);
    let b = mw.collision;
    let mut scene = horizon_client_render::collisions::SceneCollisions { world: mw.world, binding: mw.collision, subject: None };
    let mut checked = 0;
    for name in ["bamboo", "pointed_dripstone", "sulfur_spike"] {
        let Some(state) = (0..mw.registry.len() as u32).map(BlockStateId).find(|&id| mw.registry.state(id).unwrap().name == format!("minecraft:{name}") && b.cached_shapes().cached_shape(id).unwrap().is_none()) else { continue };
        let mut by_key: std::collections::HashMap<usize, Vec<u64>> = Default::default();
        let mut r = Rng(0xC01_0FF5E7 ^ name.len() as u64);
        for _ in 0..100_000 {
            let (x, z) = ((r.next() % 60_000_000) as i32 - 30_000_000, (r.next() % 60_000_000) as i32 - 30_000_000);
            let seed = position_seed(BlockPos::new(x, 0, z));
            let key = ((seed & 15) | (seed >> 8 & 15) << 4) as usize;
            let shape = b.collision_shape(&mut scene, state, [x, 64, z]).unwrap();
            let bits: Vec<u64> = shape.boxes().iter().flat_map(|bx| [bx.min_position(), bx.max_position()]).flat_map(|p| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]).collect();
            match by_key.get(&key) {
                Some(first) => assert_eq!(*first, bits, "{name}: column ({x}, {z}) key {key}"),
                None => {
                    by_key.insert(key, bits);
                }
            }
            // Inside its cell on every axis: the reason the ring is never read for these states.
            for a in horizon_core::geometry::Axis::ALL {
                assert!(shape.min(a) >= 0.0 && shape.max(a) <= 1.0, "{name}: column ({x}, {z}) reaches out of its cell on {a:?}");
            }
            checked += 1;
        }
        assert_eq!(by_key.len(), 256, "{name}: {} of 256 offset keys met", by_key.len());
    }
    eprintln!("{checked} columns checked");
    assert!(checked > 0);
}

/// Every state of these three blocks that has no cached shape is a column-offset state in the table.
#[test]
fn bamboo_and_stalactites_are_column_states() {
    use horizon_world::registry::BlockStateId;
    let mw = varied_world(1);
    let occ = occupancy_of(mw);
    let mut n = 0;
    for i in 0..mw.registry.len() {
        let id = BlockStateId(i as u32);
        let name = &mw.registry.state(id).unwrap().name;
        if matches!(name.as_str(), "minecraft:bamboo" | "minecraft:pointed_dripstone" | "minecraft:sulfur_spike") && mw.collision.cached_shapes().cached_shape(id).unwrap().is_none() {
            assert_eq!(occ.class_name(mw.collision, id), "column", "{name}");
            n += 1;
        }
    }
    assert!(n > 20, "{n} column states");
}
