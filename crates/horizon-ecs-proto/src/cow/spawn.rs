//! The bundles a creature is spawned with and the schedule of the herd.
//! A cow as data components and batch systems (palier 1): the stroll goal (a geometric wait with the law of
//! `nextInt(60) == 0` each tick, then `LandRandomPos` 10 x 7), the move control, `LivingEntity.travel` on the
//! ground with Horizon's real block collision sweep (`SceneCollisions::resolve_in_border_with`), and sleeping.
//! Species data (size, step, speed, stroll range) are component values, not branches.
#[allow(unused_imports)]
use crate::IFloor as _;
#[allow(unused_imports)]
use bevy_ecs::{
    entity_disabling::Disabled,
    prelude::*,
    query::Allow,
    schedule::{Schedule, ScheduleLabel, SingleThreadedExecutor},
};
#[allow(unused_imports)]
use horizon_client_lib::mobs::{
    Mob, MobWorld,
    nav::Navigation,
    path::{Evaluator, Finder, Path, PathMob, PATH_TYPES},
};
#[allow(unused_imports)]
use horizon_client_render::{chunk::Chunk, collisions::SceneCollisions};
#[allow(unused_imports)]
use horizon_core::geometry::{Aabb, Vec3, trig};
#[allow(unused_imports)]
use std::sync::Arc;
use super::*;

/// `HORIZON_QUICK` (set by `suites.sh`, `FULL=1` removes it): the scenes that repeat a trial many times (laws against the
/// `Mob` over dozens of seeds) do a quarter of them. 
pub fn trials(n: usize) -> usize {
    if std::env::var_os("HORIZON_QUICK").is_some() { (n / 4).max(3) } else { n }
}

/// The box of a creature of this size standing at `p`.
pub fn box_at(p: Vec3, width: f32, height: f32) -> Aabb {
    let (hw, h) = (f64::from(width) / 2.0, f64::from(height));
    Aabb::new(Vec3::new(p.x - hw, p.y, p.z - hw), Vec3::new(p.x + hw, p.y + h, p.z + hw))
}

/// The capabilities of an entity are separate bundles: a configuration without a section has none of its components.
/// `Transform`: where it is, how fast, its own time and its identity. All a projectile has.
pub fn transform_bundle(sp: &Species, x: f64, z: f64, uid: u64) -> impl Bundle {
    (Pos(Vec3::new(x, 1.0, z)), Vel(Vec3::ZERO), SpeciesId(sp.id), TypeName(sp.name), crate::goals::Uid(uid), OwnTime)
}

/// `Solid`: a box in the spatial index (what the pushes, the searches and the targeting see).
pub fn solid_bundle(sp: &Species, x: f64, z: f64) -> impl Bundle {
    (Bounds(box_at(Vec3::new(x, 1.0, z), sp.width, sp.height)), Cell(u32::MAX, [i32::MIN; 8], u32::MAX))
}

/// `Body`: the physics of a body that falls, lands, floats and is pushed by the water and the blocks (`travel`).
pub fn body_bundle(sp: &Species) -> impl Bundle {
    (
        Body { width: sp.width, height: sp.height, step_height: sp.step_height, gravity: sp.gravity, speed: sp.speed, fall_factor: crate::rules::attrs_of(sp.rules).fall_factor.unwrap_or(1.0), object: false, orb: false, tnt: false, lava_walker: sp.lava_walker },
        Contact { on_ground: false, horizontal: false, fall_distance: 0.0 },
        FluidParams { eye_height: sp.eye_height, can_float: sp.can_float, water_slowdown: sp.water_slowdown, water_efficiency: sp.water_efficiency },
        crate::targeting::Eye(sp.eye_height),
        FluidState::default(),
        Recheck,
        Moving,
    )
}

/// `Living`: health, damage, growth, air, cold, what it resists and is immune to.
pub fn living_bundle(sp: &Species) -> impl Bundle {
    (
        Sizes { adult: (sp.width, sp.height), baby: sp.baby },
        Health(sp.max_health),
        HurtAt { at: 0, last: 0.0 },
        Frozen(0),
        Air(300),
        Resist(sp.knockback_resistance),
        Immune(crate::rules::attrs_of(sp.rules).immune | if sp.breathes_water { BREATHES_WATER } else { 0 }),
    )
}

/// `Locomotion`: walking and paths (the heading, the walking input, the path and its target, the walking data).
pub fn locomotion_bundle(sp: &Species, yaw: f32) -> impl Bundle {
    (
        Yaw(yaw),
        crate::locomotion::Modes(sp.modes),
        Drive::default(),
        MoveTo::default(),
        Nav::default(),
        Stroll { walking: false, scheduled: false, tx: 0.0, tz: 0.0, ty: 1, horizontal_range: 10, vertical_range: 7, interval: 60, plain: 0.001, force: false },
        sp.walk,
        NavParams {
            malus: sp.malus,
            can_float: sp.can_float,
            can_open_doors: sp.can_open_doors,
            can_pass_doors: sp.can_pass_doors,
            can_walk_over_fences: sp.can_walk_over_fences,
            follow_range: sp.follow_range,
        },
    )
}

/// `Targets`: what the target selectors and the attacks read.
pub fn targets_bundle(sp: &Species) -> impl Bundle {
    (crate::targeting::FollowRange(f64::from(sp.follow_range)), AttackDamage(sp.attack_damage))
}

/// `Goals`: the selector, its head, and the activity zone and random stream the goals use.
pub(crate) fn goals_part(sp: &Species, uid: u64, rng: Rng) -> impl Bundle {
    (ActiveUntil(100), rng, crate::goals::goals_bundle(sp, uid))
}

/// An entity with EVERY capability at (x, z), for the tests and the benches: the species' data are the numbers.
/// `spawn_species` gives only the capabilities a configuration declares.
pub fn spawn_bundle(sp: &Species, x: f64, z: f64, seed: u64) -> impl Bundle {
    let mut rng = Rng(seed | 1);
    let yaw = (rng.next_u32() % 360) as f32;
    let uid = crate::goals::next_uid();
    (transform_bundle(sp, x, z, uid), solid_bundle(sp, x, z), body_bundle(sp), living_bundle(sp), locomotion_bundle(sp, yaw), targets_bundle(sp), crate::goals::head_bundle(sp), goals_part(sp, uid, rng))
}

/// Creates the entity with the capabilities `sp` declares and no others (body first: the goals' `equip` reads it).
pub(crate) fn spawn_declared(world: &mut World, sp: &Species, x: f64, z: f64, seed: u64) -> Entity {
    let mut rng = Rng(seed | 1);
    let yaw = (rng.next_u32() % 360) as f32;
    let uid = crate::goals::next_uid();
    let has = |c: crate::behaviours::Cap| sp.caps.contains(&c);
    // The species markers are zero-size: they go in first, so the bundle below is the only one that writes data.
    let id = world.spawn_empty().id();
    crate::type_tags::mark(world, id, sp.id);
    let saved = crate::nbt::is_saved(world, sp.name).then_some(crate::nbt::Saved);
    let mut e = world.entity_mut(id);
    e.insert((transform_bundle(sp, x, z, uid), saved));
    if has(crate::behaviours::Cap::Solid) {
        e.insert(solid_bundle(sp, x, z));
    }
    if has(crate::behaviours::Cap::Body) {
        e.insert(body_bundle(sp));
    }
    if has(crate::behaviours::Cap::Living) {
        e.insert(living_bundle(sp));
    }
    if has(crate::behaviours::Cap::Locomotion) {
        e.insert(locomotion_bundle(sp, yaw));
    }
    if has(crate::behaviours::Cap::Targets) {
        e.insert(targets_bundle(sp));
    }
    if has(crate::behaviours::Cap::Head) {
        e.insert(crate::goals::head_bundle(sp));
    }
    if has(crate::behaviours::Cap::Goals) {
        e.insert(goals_part(sp, uid, rng));
    }
    if has(crate::behaviours::Cap::Rideable) {
        e.insert(crate::mount::Mount::default());
    }
    if has(crate::behaviours::Cap::Tamable) {
        e.insert(crate::tame::Owner::default());
    }
    if has(crate::behaviours::Cap::Brain)
        && let Some(plan) = sp.brain
    {
        e.insert(crate::brain::bundle(plan));
    }
    e.id()
}

/// A new entity of the species of `parent` (a baby), in the index; the framework looks the species up, the module does not.
pub fn spawn_like(world: &mut World, parent: Entity, x: f64, z: f64, seed: u64) -> Entity {
    let sp = world.resource::<SpeciesTable>().0[world.get::<SpeciesId>(parent).unwrap().0 as usize];
    let e = spawn_declared(world, &sp, x, z, seed);
    enter_index(world, e);
    crate::rules::equip_all(world, e, sp.rules);
    e
}

/// `spawn_species` and the first deadlines of its modules (what a creature born at the start of a scene needs; `breed` does it for
/// a baby). `spawn_species` alone arms nothing: the herds arm in bulk (`goals::arm_all`), a test calls this or `arm_all`.
pub fn spawn_armed(world: &mut World, sp: &Species, x: f64, z: f64, seed: u64) -> Entity {
    let e = spawn_species(world, sp, x, z, seed);
    crate::goals::arm_timers(world, e);
    e
}

/// Spawns an animal of species `sp` (with the extra data of a species that attacks).
pub fn spawn_species(world: &mut World, sp: &Species, x: f64, z: f64, seed: u64) -> Entity {
    let e = spawn_declared(world, sp, x, z, seed);
    enter_index(world, e);
    crate::rules::equip_all(world, e, sp.rules);
    crate::brain::start(world, e);
    e
}

/// An animal of the species the process was built for (see `species`).
pub fn cow_bundle(x: f64, z: f64, seed: u64) -> impl Bundle {
    spawn_bundle(species(), x, z, seed)
}

pub fn schedule() -> Schedule {
    let mut s = Schedule::new(CowTick);
    s.set_executor(SingleThreadedExecutor::new());
    // The order is explicit: due, index, stroll, pathfind, follow, move control, travel, push, resume, sleep.
    // The damage queued since the last tick (a blow, a fall, a hazard) is applied first: its recoil is integrated by
    // this tick's physics, as a mob's hurt precedes its tick.
    s.add_systems((crate::zone::zone_step.run_if(crate::zone::zone_on), due, apply_damage, block_changes, recompute, player_near, fluid_update, pathfind, follow, path_ended, move_control, breathing, wall_check, travel, ordered_pass, reindex, hazard_tick, wall_damage, push, apply_pushes).chain());
    s.add_systems(cramming.after(apply_pushes));
    s.add_systems(crate::breath_cloud::tick_live.after(apply_pushes));
    crate::goals::register(&mut s);
    s.add_systems(crate::despawn::step.after(crate::goals::online_events).before(player_near));
    s.add_systems(crate::spawning::step.after(crate::goals::online_events).before(player_near));
    crate::farm::register(&mut s);
    crate::projectile::register(&mut s);
    crate::brain::register(&mut s);
    crate::item::register(&mut s);
    crate::orb::register(&mut s);
    crate::hanging::register(&mut s);
    crate::species::cushion::register(&mut s);
    crate::species::leash_knot::register(&mut s);
    crate::firework::register(&mut s);
    crate::trident::register(&mut s);
    crate::falling_block::register(&mut s);
    crate::stand::register(&mut s);
    crate::eye::register(&mut s);
    crate::fishing_hook::register(&mut s);
    crate::poi::register(&mut s);
    crate::block_watch::register(&mut s);
    crate::tame::register(&mut s);
    crate::mount::register(&mut s);
    crate::hostile::register(&mut s);
    s
}
