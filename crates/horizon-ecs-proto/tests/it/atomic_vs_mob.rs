//! The ecs's locomotion and spots functions against the `Mob` functions of the same name, on the same inputs, bit for bit
//! (`move_relative`, `stable_destination`, `walk_target_value`): a grid of inputs, no tolerance.
use crate::common;
use common::*;
use bevy_ecs::system::RunSystemOnce as _;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, locomotion::fly, spots};

fn bits(v: Vec3) -> [u64; 3] {
    [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]
}
/// A flat scene: grass on the half z >= 0, stone under the rest, a pool and an air gap.
fn scene(r: &MobRef) {
    for x in -12..=12 {
        for z in -12..=12 {
            r.set_block(x, 0, z, if z >= 0 { "minecraft:grass_block" } else { "minecraft:stone" });
        }
    }
    r.set_block(3, 1, 3, "minecraft:water");
    r.set_block(-4, 0, 6, "minecraft:air");
}

#[test]
fn move_relative_3d_is_the_mob_move_relative() {
    if !have_data() {
        return;
    }
    let (mut r, _h) = flat_pair();
    let i = r.add(Vec3::new(0.5, 1.0, 0.5));
    // Inputs: zero (both signs), units, a tiny one under 1e-7 in square, large, the largest f32.
    let vals: [f32; 13] = [0.0, -0.0, 1.0, -1.0, 0.5, -0.003, 1.0e-4, 1.0e5, f32::MAX, -f32::MIN_POSITIVE, 0.7071, 2.0, -3.5];
    let seeds = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(-0.0, -0.0, -0.0), Vec3::new(1.0e300, -2.5, 0.25), Vec3::new(-0.003, 0.0, 1.0e-9)];
    for (k, yaw) in (0..=1271).map(|k| -1080.0 + k as f32 * 1.7).enumerate() {
        let seed = seeds[k % seeds.len()];
        for speed in [0.0_f32, 0.1, 1.0] {
            for x in vals {
                for y in vals {
                    for z in vals {
                        let m = &mut r.mobs[i];
                        m.yaw = yaw;
                        m.velocity = seed;
                        m.move_relative(speed, Vec3::new(f64::from(x), f64::from(y), f64::from(z)));
                        let mut v = Vel(seed);
                        fly::move_relative_3d(&mut v, yaw, speed, x, y, z);
                        assert_eq!(bits(m.velocity), bits(v.0), "yaw {yaw} speed {speed} input {x} {y} {z} seed {seed:?}");
                    }
                }
            }
        }
    }
}

#[test]
fn stable_destination_is_the_mob_stable_destination() {
    if !have_data() {
        return;
    }
    let (mut r, _h) = flat_pair();
    if matches!(species().name, "minecraft:bee" | "minecraft:turtle") {
        horizon_ecs_proto::cow::skip_note("stable_destination_is_the_mob_stable_destination", "the bee and the turtle have their own navigation");
        return;
    }
    // The ground navigation's stable spot: a species that changes navigation with its medium (it has a water mode: the swim tests read the
    // switch), a flier (its navigation reads the block itself) or a lava walker draws its spots with another one.
    let modes = species().modes;
    let medium = |m| modes.iter().any(|e| e.0 == m);
    if medium(horizon_ecs_proto::locomotion::Medium::Water) || modes.first().map(|m| m.0) == Some(horizon_ecs_proto::locomotion::Medium::Air) || species().lava_walker {
        horizon_ecs_proto::cow::skip_note("stable_destination_is_the_mob_stable_destination", "its navigation is not the ground's in every medium (water mode, flier or lava walker)");
        return;
    }
    let kind = spots::Stable::Ground;
    scene(&r);
    let i = r.add(Vec3::new(0.5, 1.0, 0.5));
    for x in -12..=12 {
        for y in -1..=4 {
            for z in -12..=12 {
                let p = [x, y, z];
                assert_eq!(spots::stable_for(kind, r.w, p), r.mobs[i].stable_destination(r.w, p), "{p:?}");
            }
        }
    }
}

#[test]
fn walk_target_value_is_the_mob_walk_target_value() {
    if !have_data() {
        return;
    }
    let (mut r, _h) = flat_pair();
    scene(&r);
    let i = r.add(Vec3::new(0.5, 1.0, 0.5));
    let wv = species().walk;
    for x in -12..=12 {
        for y in -1..=4 {
            for z in -12..=12 {
                let p = [x, y, z];
                let (a, b) = (spots::walk_target_value(r.w, wv, p), horizon_client_lib::mobs::ai::walk_target_value(&r.mobs[i], r.w, p));
                assert_eq!(a.to_bits(), b.to_bits(), "{p:?}");
            }
        }
    }
}

/// `LivingEntity.getVisibilityPercent` of a creature for a looker: invisible by armour cover, the head items that fool one species only.
/// The ECS function against `Seen::visibility_percent`, every combination, bit for bit (clamped to 0..=10 as the game does).
#[test]
fn creature_visibility_is_the_mob_visibility_percent() {
    use horizon_client_lib::mobs::Species;
    use horizon_ecs_proto::goals::Slot;
    if !have_data() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    let i = r.add(Vec3::new(0.5, 1.0, 0.5));
    let e = ecs_cow(&mut h, 0.5, 1.0, 0.5, 5);
    let items = r.w.items.expect("items");
    let heads = [None, Some("minecraft:skeleton_skull"), Some("minecraft:zombie_head"), Some("minecraft:creeper_head"), Some("minecraft:piglin_head"), Some("minecraft:diamond_helmet")];
    let worn = [None, Some("minecraft:diamond_chestplate")];
    let lookers = [
        (Species::Skeleton, "minecraft:skeleton"),
        (Species::Zombie, "minecraft:zombie"),
        (Species::Creeper, "minecraft:creeper"),
        (Species::Piglin, "minecraft:piglin"),
        (Species::PiglinBrute, "minecraft:piglin_brute"),
        (Species::Stray, "minecraft:stray"),
    ];
    for invisible in [false, true] {
        for head in heads {
            for chest in worn {
                r.mobs[i].invisible = invisible;
                r.mobs[i].extra.set_slot_id(horizon_items::equipment::HEAD, head.map(Into::into));
                r.mobs[i].extra.set_slot_id(horizon_items::equipment::CHEST, chest.map(Into::into));
                if invisible {
                    h.world.entity_mut(e).insert(horizon_ecs_proto::effects::Invisible);
                } else {
                    h.world.entity_mut(e).remove::<horizon_ecs_proto::effects::Invisible>();
                }
                horizon_ecs_proto::equipment::set_slot(&mut h.world, e, Slot::Head, head.map_or(0, horizon_ecs_proto::items::item_id));
                horizon_ecs_proto::equipment::set_slot(&mut h.world, e, Slot::Chest, chest.map_or(0, horizon_ecs_proto::items::item_id));
                let seen = r.mobs[i].seen(None, items);
                for (sp, name) in lookers {
                    let Some(id) = horizon_ecs_proto::species::registry().id(name) else { continue };
                    let mob = seen.visibility_percent(sp).clamp(0.0, 10.0);
                    let ecs = horizon_ecs_proto::targeting::creature_visibility_to(&h.world, e, Some(id as u8));
                    assert_eq!(mob.to_bits(), ecs.to_bits(), "invisible {invisible} head {head:?} chest {chest:?} looker {name}");
                }
            }
        }
    }
}

/// `LivingEntity.aiStep` drops a motion under 0.003 on an axis whatever the move control does: a ghast with no wanted spot (its control
/// idle) loses a tiny velocity at the next tick, on the Mob and in the ECS.
#[test]
fn a_ghast_with_no_wanted_spot_drops_a_tiny_motion_as_a_mob_does() {
    if !have_data() || wrong_species(&["minecraft:ghast"]) {
        return;
    }
    let (mut r, mut h) = flat_pair();
    let i = r.add(Vec3::new(0.5, 5.0, 0.5));
    let e = ecs_cow(&mut h, 0.5, 5.0, 0.5, 5);
    let tiny = Vec3::new(0.002, -0.002, 0.001);
    r.mobs[i].velocity = tiny;
    r.step(None);
    assert_eq!(bits(r.mobs[i].velocity), bits(Vec3::ZERO), "the Mob dropped the tiny motion: {:?}", r.mobs[i].velocity);
    // The ECS: the move control alone (the goals pick a spot of their own), with no wanted spot: the tiny motion goes.
    h.world.get_mut::<MoveTo>(e).unwrap().active = false;
    h.world.get_mut::<Vel>(e).unwrap().0 = tiny;
    h.world.run_system_once(horizon_ecs_proto::locomotion::fly::fly_controls).unwrap();
    let c = h.world.get::<Vel>(e).unwrap().0;
    assert_eq!(bits(c), bits(Vec3::ZERO), "the ECS drops it too: {c:?}");
}
