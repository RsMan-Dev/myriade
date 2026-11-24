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
use myriade_engine_lib::mobs::{
    MobWorld,
    nav::Navigation,
    path::{Evaluator, Finder, Path, PathMob, PATH_TYPES},
};
#[allow(unused_imports)]
use myriade_level::{chunk::Chunk, collisions::SceneCollisions};
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
        Body { width: sp.width, height: sp.height, step_height: sp.step_height, speed: sp.speed, fall_factor: crate::rules::attrs_of(sp.rules).fall_factor.unwrap_or(1.0), safe_fall: 3.0, fall_mult: 1.0, movement_efficiency: 0.0, object: false, orb: false, tnt: false, lava_walker: sp.lava_walker },
        Contact { on_ground: false, horizontal: false, fall_distance: 0.0, stuck: 0 },
        FluidParams { eye_height: sp.eye_height, can_float: sp.can_float, water_slowdown: sp.water_slowdown, water_efficiency: sp.water_efficiency, max_air: sp.max_air },
        crate::targeting::Eye(sp.eye_height),
        FluidState::default(),
        Recheck,
        Moving,
    )
}

/// `bit` when the species has the capability `cap`, else 0.
fn cap_bit(sp: &Species, cap: crate::behaviours::Cap, bit: u32) -> u32 {
    if sp.caps.contains(&cap) { bit } else { 0 }
}

/// `Living`: health, damage, growth, air, cold, what it resists and is immune to.
pub fn living_bundle(sp: &Species) -> impl Bundle {
    (
        Sizes { adult: (sp.width, sp.height), baby: sp.baby },
        Health(sp.max_health),
        HurtAt { at: 0, last: 0.0 },
        Frozen(0),
        Air(sp.max_air),
        Resist(sp.knockback_resistance),
        Restitution(LIVING_RESTITUTION),
        Immune(crate::rules::attrs_of(sp.rules).immune | cap_bit(sp, crate::behaviours::Cap::FireImmune, crate::effects::IMMUNE_FIRE) | cap_bit(sp, crate::behaviours::Cap::WitherImmune, crate::effects::IMMUNE_WITHER) | cap_bit(sp, crate::behaviours::Cap::PoisonImmune, crate::effects::IMMUNE_POISON) | cap_bit(sp, crate::behaviours::Cap::WeaknessImmune, crate::effects::IMMUNE_WEAKNESS) | cap_bit(sp, crate::behaviours::Cap::FreezeImmune, crate::effects::IMMUNE_FREEZE)),
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

/// The speed and jump strength drawn after the max health, as base values, in the order of the class's `randomizeAttributes`: `Horse` draws
/// its speed then its jump (`generateSpeed`, `generateJumpStrength`: three doubles each), `ZombieHorse` its jump then its speed (the zombie
/// formulas), `SkeletonHorse` its jump.
fn horse_attrs(g: crate::species::HorseGen, rng: &mut Rng) -> Vec<(String, f64, Vec<crate::attr_mod::Modifier>)> {
    use crate::species::HorseGen;
    let mut three = || [rng.next_f64(), rng.next_f64(), rng.next_f64()];
    let mut out: Vec<(&str, f64)> = Vec::new();
    match g {
        HorseGen::None => {}
        HorseGen::Horse => {
            let s = three();
            let j = three();
            out.push(("movement_speed", (0.449_999_988_079_071_04 + s[0] * 0.3 + s[1] * 0.3 + s[2] * 0.3) * 0.25));
            out.push(("jump_strength", 0.400_000_005_960_464_5 + j[0] * 0.2 + j[1] * 0.2 + j[2] * 0.2));
        }
        HorseGen::Skeleton => {
            let j = three();
            out.push(("jump_strength", 0.400_000_005_960_464_5 + j[0] * 0.2 + j[1] * 0.2 + j[2] * 0.2));
        }
        HorseGen::Zombie => {
            let j = three();
            let s = three();
            out.push(("jump_strength", 0.5 + j[0] * 0.066_666_666_666_666_67 + j[1] * 0.066_666_666_666_666_67 + j[2] * 0.066_666_666_666_666_67));
            out.push(("movement_speed", (9.0 + s[0] + s[1] + s[2]) / 42.159_999_847_412_11));
        }
    }
    out.into_iter().map(|(n, b)| (n.to_owned(), b, Vec::new())).collect()
}

/// Creates the entity with the capabilities `sp` declares and no others (body first: the goals' `equip` reads it).
pub(crate) fn spawn_declared(world: &mut World, sp: &Species, x: f64, z: f64, seed: u64, natural: bool) -> Entity {
    let mut rng = Rng(seed | 1);
    let yaw = (rng.next_u32() % 360) as f32;
    // `Goat.finalizeSpawn`: the screaming draw follows the yaw, only for a species that has a chance (the Mob draws it at the spawn too).
    let screams = natural && sp.screaming.is_some_and(|c| rng.next_f64() < c);
    // `Llama.finalizeSpawn` (`mobs/llama.rs` `finalize`): 1 to 3, or 1 to 5 in 4 percent of the draws; `Caravan` is the llama's component.
    let strength = sp.llama_strength.then(|| {
        let max: u32 = if rng.next_f64() < 0.04 { 5 } else { 3 };
        1 + (rng.next_u32() % max) as i32
    });
    // `AbstractHorse.randomizeAttributes` (`Horse`, `AbstractChestedHorse`, a llama): `generateMaxHealth`, 15 and a `nextInt(8)` and a `nextInt(9)`.
    let max_health = sp.random_max_health.then(|| 15.0 + (rng.next_u32() % 8) as f32 + (rng.next_u32() % 9) as f32);
    let mut attrs = horse_attrs(sp.horse_gen, &mut rng);
    let uid = crate::goals::next_uid();
    let has = |c: crate::behaviours::Cap| sp.caps.contains(&c);
    // The species markers are zero-size: they go in first, so the bundle below is the only one that writes data.
    let id = world.spawn_empty().id();
    crate::type_tags::mark(world, id, sp.id);
    let saved = crate::nbt::is_saved(world, sp.name);
    let mut e = world.entity_mut(id);
    if screams {
        e.insert(crate::species::goat::Screaming);
    }
    if let Some(s) = strength {
        e.insert(crate::behaviours::forget_target::LlamaStrength(s));
    }
    if sp.body_wear {
        e.insert(crate::equipment_wear::BodyWear);
    }
    // `getSwimSound`: the species' water sound, a component only where it differs from `entity.generic.swim`.
    if let Some(sw) = crate::species::swim::swim_sounds(sp.name, crate::cow::is_monster(sp.name)) {
        e.insert(sw);
    }
    // The seats of a vehicle and of a rider (`species::attachments`), a component only where the species has one.
    if let Some(seats) = crate::species::attachments::passenger_seats(sp.name) {
        e.insert(crate::mount::PassengerSeats { adult: seats.adult, baby: seats.baby });
    }
    if let Some(seat) = crate::species::attachments::rider_seat(sp.name) {
        e.insert(seat);
    }
    e.insert(transform_bundle(sp, x, z, uid));
    if saved {
        e.insert(crate::nbt::Saved);
    }
    if has(crate::behaviours::Cap::Solid) {
        e.insert(solid_bundle(sp, x, z));
    }
    if has(crate::behaviours::Cap::Body) {
        e.insert(body_bundle(sp));
        if let Some(g) = sp.gravity {
            e.insert(crate::cow::Gravity(g));
        }
    }
    if has(crate::behaviours::Cap::Living) {
        e.insert(living_bundle(sp));
    }
    if let Some(m) = max_health {
        attrs.insert(0, ("max_health".to_owned(), f64::from(m), Vec::new()));
        e.insert(Health(m));
    }
    if !attrs.is_empty() {
        e.insert(crate::attr_mod::AttrMods(attrs));
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
    if sp.persistent {
        e.insert(crate::despawn::Persistent);
    }
    if sp.not_in_peaceful {
        e.insert(crate::bridge::host_peaceful::NotInPeaceful);
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
    let id = e.id();
    // `Goat.finalizeSpawn`: a natural adult may have lost one horn (the coin and the chance of its species, drawn after the yaw).
    if natural && world.get::<GrowAt>(id).is_none() && sp.lost_horn.is_some_and(|c| rng.next_f64() < c) {
        let left = rng.next_u32() & 1 == 1;
        world.entity_mut(id).insert(crate::behaviours::ram::Shed(if left { 1 } else { 2 }));
    }
    id
}

/// `/summon` with data does not call `finalizeSpawn`: a phantom has no anchor, the equid keeps the attributes of its type (53 health, the base speed and jump), without
/// the draws of `AbstractHorse.randomizeAttributes` that `spawn_declared` made.
pub fn forget_birth_draws(world: &mut World, e: Entity, sp: &Species) {
    // `Phantom.finalizeSpawn` sets the anchor 5 blocks up; the data a summon is given leaves it null (`anchor_pos` is read, or none).
    if sp.name == "minecraft:phantom" {
        world.entity_mut(e).remove::<crate::cow::Anchor>();
    }
    if !sp.random_max_health && matches!(sp.horse_gen, crate::species::HorseGen::None) {
        return;
    }
    world.entity_mut(e).remove::<crate::attr_mod::AttrMods>().insert(Health(sp.max_health));
}

/// A new entity of the species of `parent` (a baby), in the index; the framework looks the species up, the module does not.
pub fn spawn_like(world: &mut World, parent: Entity, x: f64, z: f64, seed: u64) -> Entity {
    let sp = world.resource::<SpeciesTable>().0[world.get::<SpeciesId>(parent).unwrap().0 as usize];
    let e = spawn_declared(world, &sp, x, z, seed, false);
    enter_index(world, e);
    crate::rules::equip_all(world, e, sp.rules);
    crate::leash::equip_default(world, e, sp.name);
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
    let e = spawn_declared(world, sp, x, z, seed, true);
    if let Ok(r) = crate::species::registry().by_name(sp.name) {
        if r.merchant {
            world.entity_mut(e).insert_if_new(crate::behaviours::trade_offers::Merchant::new(false, 0, 0));
        }
        if r.no_portal {
            world.entity_mut(e).insert(crate::portal::NoPortal);
        }
        if let Some(ticks) = r.despawn_delay {
            crate::despawn_delay::start_for(world, e, ticks);
        }
    }
    enter_index(world, e);
    crate::rules::equip_all(world, e, sp.rules);
    crate::leash::equip_default(world, e, sp.name);
    crate::brain::start(world, e);
    e
}

/// An animal of the species the process was built for (see `species`).
pub fn cow_bundle(x: f64, z: f64, seed: u64) -> impl Bundle {
    // A cow is always subject to gravity: `LivingEntity.getGravity` of the type (0.08, the attribute's default).
    (spawn_bundle(species(), x, z, seed), Gravity(0.08))
}

pub fn schedule() -> Schedule {
    let mut s = Schedule::new(CowTick);
    s.set_executor(SingleThreadedExecutor::new());
    // The order is explicit: due, index, stroll, pathfind, follow, move control, travel, push, resume, sleep.
    // The damage queued since the last tick (a blow, a fall, a hazard) is applied first: its recoil is integrated by
    // this tick's physics, as a mob's hurt precedes its tick.
    s.add_systems(((crate::zone::zone_step.run_if(crate::zone::zone_on), due, apply_damage, block_changes, recompute, player_near, fluid_update, pathfind, follow, path_ended, move_control).chain(), (breathing, wall_check, crate::fall_reset::fall_reset, travel, ordered_pass, reindex, hazard_tick, wall_damage, push, apply_pushes).chain()).chain());
    s.add_systems(cramming.after(apply_pushes));
    s.add_systems(crate::cow::health::rain_out.after(crate::cow::health::hazard_tick));
    s.add_systems(crate::breath_cloud::tick_live.after(apply_pushes));
    crate::goals::register(&mut s);
    s.add_systems(crate::despawn::step.after(crate::goals::online_events).before(player_near));
    s.add_systems(crate::spawning::step.after(crate::goals::online_events).before(player_near));
    crate::farm::register(&mut s);
    crate::hand_over::register(&mut s);
    crate::projectile::register(&mut s);
    crate::brain::register(&mut s);
    crate::item::register(&mut s);
    crate::orb::register(&mut s);
    crate::hanging::register(&mut s);
    crate::species::cushion::register(&mut s);
    crate::species::leash_knot::register(&mut s);
    crate::behaviours::teleport_on::register(&mut s);
    crate::firework::register(&mut s);
    crate::trident::register(&mut s);
    crate::falling_block::register(&mut s);
    crate::stand::register(&mut s);
    crate::eye::register(&mut s);
    crate::fishing_hook::register(&mut s);
    crate::inside::register(&mut s);
    crate::inside_effects::register(&mut s);
    crate::landing_trample::register(&mut s);
    crate::inside_more::register(&mut s);
    crate::poi::register(&mut s);
    crate::block_watch::register(&mut s);
    crate::chest_index::register(&mut s);
    crate::tame::register(&mut s);
    crate::mount::register(&mut s);
    crate::hostile::register(&mut s);
    crate::behaviours::bed::register(&mut s);
    crate::seat::register(&mut s);
    crate::boat_tick::register(&mut s);
    crate::boat_effects::register(&mut s);
    crate::cart_move::register(&mut s);
    crate::cart_variants::register(&mut s);
    crate::general::register(&mut s);
    crate::stuck::register(&mut s);
    crate::gliding::register(&mut s);
    crate::colony_register::register(&mut s);
    crate::raids::register(&mut s);
    crate::tnt_tick::register(&mut s);
    crate::move_events::register(&mut s);
    crate::nav_supplied::register(&mut s);
    crate::behaviours::glow_particles::register(&mut s);
    crate::vehicle_push::register(&mut s);
    s
}
