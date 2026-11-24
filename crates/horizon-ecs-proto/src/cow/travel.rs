//! `LivingEntity.travel` for creatures, objects and orbs.
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
use crate::rules::climbable;

/// `ItemEntity.tick`'s motion for an object: a fluid slows it (water 0.99, lava 0.95, lifting a slow one by 5.0E-4) or gravity pulls it
/// down BEFORE the move; it moves unless it lies still on the ground; then the drag of the air (0.98 on every axis, times the block's friction
/// on the ground for x and z) and the bounce (a downward speed on the ground is halved and turned up). True when it is at rest. An object that
/// walks (`Drive`) is accelerated as a creature is, before the move.
#[allow(clippy::too_many_arguments)]
pub(crate) fn travel_object(level: &Level, pos: &mut Pos, vel: &mut Vel, yaw: &Yaw, c: &mut Contact, d: &mut Drive, body: &Body, gravity: Option<f64>, bounds: &mut Bounds, fs: &FluidState, fall_damage: &mut f32) -> bool {
    if body.tnt {
        return crate::tnt::travel_tnt(level, pos, vel, c, body, gravity, bounds, fall_damage);
    }
    if body.orb {
        return travel_orb(level, pos, vel, c, d, body, gravity, bounds, fs, fall_damage);
    }
    let g = gravity.unwrap_or(0.0);
    let height = f64::from(0.1_f32);
    let (in_water, in_lava) = (fs.water > height, fs.lava > height);
    if in_water || in_lava {
        let drag = f64::from(if in_water { 0.99_f32 } else { 0.95_f32 });
        let lift = if vel.0.y < f64::from(0.06_f32) { f64::from(5.0e-4_f32) } else { 0.0 };
        vel.0 = Vec3::new(vel.0.x * drag, vel.0.y + lift, vel.0.z * drag);
    } else {
        vel.0.y -= g;
    }
    if d.zza != 0.0 || d.xxa != 0.0 {
        let speed = if c.on_ground { d.speed } else { 0.02 };
        move_relative(vel, yaw.0, speed, d.xxa, d.yya, d.zza);
    }
    let horizontal = vel.0.x * vel.0.x + vel.0.z * vel.0.z;
    if c.on_ground && horizontal <= 1.0e-5 && d.zza == 0.0 && d.xxa == 0.0 && vel.0.y <= 0.0 && vel.0.y > -g * 1.01 {
        // Lying still on the ground: the gravity of this tick is not integrated, nothing moves.
        vel.0 = Vec3::ZERO;
        return true;
    }
    do_move(level, pos, vel, c, body, bounds, fall_damage);
    let mut friction = 0.98_f32;
    if c.on_ground {
        let mw = level.mw.0;
        let below = mw.world.block(pos.0.x.ifloor(), (pos.0.y - 0.999_999).ifloor(), pos.0.z.ifloor());
        friction *= mw.facts.get(below).friction;
    }
    let (f, air) = (f64::from(friction), f64::from(0.98_f32));
    vel.0 = Vec3::new(vel.0.x * f, vel.0.y * air, vel.0.z * f);
    if c.on_ground && vel.0.y < 0.0 {
        vel.0.y *= -0.5;
    }
    false
}

/// `ExperienceOrb.tick`'s motion: its eye in water floats it (0.99, lifted by 5.0E-4 up to 0.06), else gravity unless it is in a block; it moves,
/// then every axis loses the same drag (0.98, times the block's friction on the ground), and a fall faster than 0.03 bounces by 0.4 of it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn travel_orb(level: &Level, pos: &mut Pos, vel: &mut Vel, c: &mut Contact, d: &mut Drive, body: &Body, gravity: Option<f64>, bounds: &mut Bounds, fs: &FluidState, fall_damage: &mut f32) -> bool {
    let mw = level.mw.0;
    let g = gravity.unwrap_or(0.0);
    if fs.eye_in_water {
        vel.0 = Vec3::new(vel.0.x * f64::from(0.99_f32), (vel.0.y + f64::from(5.0e-4_f32)).min(f64::from(0.06_f32)), vel.0.z * f64::from(0.99_f32));
    } else if level.no_collision(bounds.0) {
        vel.0.y -= g;
    }
    let horizontal = vel.0.x * vel.0.x + vel.0.z * vel.0.z;
    if c.on_ground && !fs.in_water && horizontal <= 1.0e-10 && vel.0.y <= 0.0 && vel.0.y > -g * 1.01 {
        vel.0 = Vec3::ZERO;
        return true;
    }
    let wanted_y = vel.0.y;
    do_move(level, pos, vel, c, body, bounds, fall_damage);
    let mut friction = 0.98_f32;
    if c.on_ground {
        let below = mw.world.block(pos.0.x.ifloor(), (pos.0.y - 0.999_999).ifloor(), pos.0.z.ifloor());
        friction *= mw.facts.get(below).friction;
    }
    let f = f64::from(friction);
    vel.0 = Vec3::new(vel.0.x * f, vel.0.y * f, vel.0.z * f);
    if c.on_ground && wanted_y < -0.03 {
        vel.0.y = -wanted_y * 0.4;
    }
    let _ = d;
    false
}

/// The block under a creature on the ground (`getBlockPosBelowThatAffectsMyMovement`, `getOnPos(0.500001F)`): the supporting block under
/// the box, found as `world_push` finds it, else the centre column. Read here only: nothing is kept per entity.
pub(crate) fn ground_block(level: &Level, pos: Vec3, bounds: Aabb) -> horizon_core::position::BlockPos {
    let mw = level.mw.0;
    let mut scene = SceneCollisions { world: mw.world, binding: mw.collision, subject: None };
    let mut ground = horizon_world::ground_support::GroundState::default();
    let _ = ground.check_support(true, bounds, None, |foot| {
        horizon_world::ground_support::find_supporting_block(&mut scene, horizon_world::collision_context::CollisionContext::empty(false), pos, foot, 1_000_000)
    });
    let centre = horizon_core::position::BlockPos::new(pos.x.ifloor(), (pos.y - 0.500_001).ifloor(), pos.z.ifloor());
    horizon_world::movement_support::support_position(&mut crate::block_factor::PBlocks { mw }, pos, ground.supporting_block, horizon_world::movement_support::MOVEMENT_SUPPORT_OFFSET).unwrap_or(centre)
}

/// The block friction under a creature on the ground (`LivingEntity.travelInAir`), read at [`ground_block`].
fn ground_friction(level: &Level, pos: Vec3, bounds: Aabb) -> f32 {
    let at = ground_block(level, pos, bounds);
    level.mw.0.facts.get(level.mw.0.world.block(at.x, at.y, at.z)).friction
}

/// `LivingEntity.travel` of a cow: in the air (on the ground), or in water or lava (`travelInFluid`).
#[allow(clippy::too_many_arguments)]
/// `Entity.getBlockSpeedFactor` of a living body (`LivingEntity`: the block's factor lerped toward 1 by `movement_efficiency`), read by the
/// caller for the horizontal motion; 1 when it does not move sideways and takes no input (no block is read).
pub fn block_speed(level: &Level, pos: &Pos, body: &Body, vel: &Vel, input: bool) -> f32 {
    if !input && vel.0.x == 0.0 && vel.0.z == 0.0 {
        return 1.0;
    }
    let f = crate::block_factor::speed(level.mw.0, [pos.0.x, pos.0.y, pos.0.z]);
    f + (1.0 - f) * body.movement_efficiency
}

pub(crate) fn travel_one(level: &Level, sp: &FluidParams, pos: &mut Pos, vel: &mut Vel, yaw: &Yaw, c: &mut Contact, d: &mut Drive, body: &Body, gravity: Option<f64>, bounds: &mut Bounds, fs: &FluidState, jumping: bool, fall_damage: &mut f32) -> bool {
    travel_one_with(level, sp, pos, vel, yaw, c, d, body, gravity, bounds, fs, jumping, fall_damage, true)
}

/// `travel_one`, with the tiny motions dropped here (`drop_tiny`) or already dropped by the caller's control, before it changed the velocity.
#[allow(clippy::too_many_arguments)]
pub(crate) fn travel_one_with(
    level: &Level,
    sp: &FluidParams,
    pos: &mut Pos,
    vel: &mut Vel,
    yaw: &Yaw,
    c: &mut Contact,
    d: &mut Drive,
    body: &Body,
    gravity: Option<f64>,
    bounds: &mut Bounds,
    fs: &FluidState,
    jumping: bool,
    fall_damage: &mut f32,
    drop_tiny: bool,
) -> bool {
    if body.object {
        return travel_object(level, pos, vel, yaw, c, d, body, gravity, bounds, fs, fall_damage);
    }
    let g = gravity.unwrap_or(0.0);
    // `LivingEntity.getEffectiveGravity`: a falling mob with slow falling has its gravity capped at 0.01.
    let g = if d.slow_falling && vel.0.y <= 0.0 { g.min(0.01) } else { g };
    let rest_vy = (0.0 - g) * f64::from(0.98_f32);
    let in_fluid = fs.in_water || fs.in_lava;
    let jumping = jumping || d.jump;
    // A cow at its resting state stays as it is: no sweep is made for it.
    if !in_fluid && !jumping && c.on_ground && d.levitation == 0 && d.zza == 0.0 && d.xxa == 0.0 && vel.0.x == 0.0 && vel.0.z == 0.0 && vel.0.y == rest_vy {
        return true;
    }
    // `Chicken.aiStep`: a hen in the air has its downward speed cut (the species' `slow_fall`, 1 for the others).
    if !c.on_ground && vel.0.y < 0.0 {
        vel.0.y *= body.fall_factor;
    }
    // `LivingEntity.aiStep`: the tiny motions are dropped.
    if drop_tiny {
        crate::locomotion::fly::drop_tiny_motion(vel);
    }
    // `LivingEntity.handleJumping`, in a fluid: the jump input lifts a cow whose feet are in water or lava.
    let threshold = if sp.eye_height < 0.4 { 0.0 } else { 0.4 };
    if jumping && in_fluid {
        let deep = if fs.in_lava { fs.lava > threshold } else { fs.water > threshold };
        if deep || !c.on_ground {
            vel.0.y += if sp.can_float { f64::from(0.04_f32) } else { 0.3 };
        }
    }
    // `LivingEntity.handleJumping` on the ground: a jump every 10 ticks at most (`noJumpDelay`), the delay reset when
    // the input stops; `jumpFromGround`: the jump power 0.42 (the block's factor is 1 here).
    if d.no_jump_delay > 0 {
        d.no_jump_delay -= 1;
    }
    if !jumping {
        d.no_jump_delay = 0;
    } else if !in_fluid && c.on_ground && d.no_jump_delay == 0 {
        vel.0.y = vel.0.y.max(0.42);
        d.no_jump_delay = 10;
    }
    if in_fluid {
        let falling = vel.0.y <= 0.0;
        let old_y = pos.0.y;
        if fs.in_water {
            let mut slow = sp.water_slowdown;
            let mut speed = 0.02_f32;
            let mut efficiency = sp.water_efficiency;
            if !c.on_ground {
                efficiency *= 0.5;
            }
            if efficiency > 0.0 {
                slow += (0.546_000_06 - slow) * efficiency;
                speed += (d.speed - speed) * efficiency;
            }
            move_relative(vel, yaw.0, speed, d.xxa, d.yya, d.zza);
            do_move(level, pos, vel, c, body, bounds, fall_damage);
            // `LivingEntity.travelInWater`: `horizontalCollision && onClimbable()` lifts it (a ladder under water too).
            let mut m = vel.0;
            if climbable::climb_lift(climbable::on_climbable(level.mw.0, pos.0, d.climber, d.climbing, d.no_climb, d.fall_flying), c.horizontal, false) {
                m.y = 0.2;
            }
            let m = Vec3::new(m.x * f64::from(slow), m.y * f64::from(0.8_f32), m.z * f64::from(slow));
            vel.0 = myriade_engine_lib::mobs::living::fluid_falling(g, falling, false, m);
            c.fall_distance = 0.0;
        } else {
            move_relative(vel, yaw.0, 0.02, d.xxa, d.yya, d.zza);
            do_move(level, pos, vel, c, body, bounds, fall_damage);
            let v = vel.0;
            if fs.lava <= threshold {
                vel.0 = Vec3::new(v.x * 0.5, v.y * f64::from(0.8_f32), v.z * 0.5);
                vel.0 = myriade_engine_lib::mobs::living::fluid_falling(g, falling, false, vel.0);
            } else {
                vel.0 = Vec3::new(v.x * 0.5, v.y * 0.5, v.z * 0.5);
            }
            if g != 0.0 {
                vel.0.y -= g / 4.0;
            }
        }
        // A cow against the shore with free space and no liquid ahead hops out.
        let m = vel.0;
        let ahead = bounds.0.move_by(Vec3::new(m.x, m.y + f64::from(0.6_f32) - pos.0.y + old_y, m.z));
        let mw = level.mw.0;
        if c.horizontal && level.no_collision(ahead) && !mw.contains_any_liquid(ahead) {
            vel.0.y = f64::from(0.3_f32);
        }
        return false;
    }
    let friction: f32 = if c.on_ground { ground_friction(level, pos.0, bounds.0) } else { 1.0 };
    let speed = if c.on_ground {
        if friction > 0.6 { d.speed * (0.216_000_02 / (friction * friction * friction)) } else { d.speed }
    } else {
        d.air_speed.unwrap_or(0.02)
    };
    move_relative(vel, yaw.0, speed, d.xxa, d.yya, d.zza);
    // `LivingEntity.handleOnClimbable`, the move, then the climb up a wall (a wall climber pressed against one).
    let on_climb = climbable::on_climbable(level.mw.0, pos.0, d.climber, d.climbing, d.no_climb, d.fall_flying);
    if on_climb {
        c.fall_distance = 0.0;
        vel.0 = climbable::velocity(vel.0);
    }
    do_move(level, pos, vel, c, body, bounds, fall_damage);
    let mut movement = vel.0;
    if climbable::climb_lift(on_climb, c.horizontal, jumping) {
        movement.y = 0.2;
    }
    if d.climber {
        // `Spider.tick`: climbing while pressed against a wall.
        d.climbing = c.horizontal;
    }
    // `LivingEntity.travelInAir`: levitation (`effects.rs`) pulls the vertical speed toward 0.05 * (level + 1) instead of gravity.
    let y = if d.levitation > 0 {
        c.fall_distance = 0.0;
        movement.y + (0.05 * f64::from(d.levitation) - movement.y) * 0.2
    } else {
        movement.y - g
    };
    if d.discard_friction {
        vel.0 = Vec3::new(movement.x, y, movement.z);
        return false;
    }
    let drag = 0.91_f32;
    let f = friction * drag;
    vel.0 = Vec3::new(movement.x * f64::from(f), y * f64::from(0.98_f32), movement.z * f64::from(f));
    !jumping && c.on_ground && d.zza == 0.0 && d.xxa == 0.0 && vel.0.x == 0.0 && vel.0.z == 0.0 && vel.0.y == rest_vy
}

#[allow(clippy::type_complexity)]
pub(crate) fn travel(
    cfg: Res<Config>,
    level: Res<Level>,
    mut par: crate::cow::OrdCmds,
    plain: ParallelCommands,
    mut q: Query<
        (Entity, &mut Pos, &mut Vel, Option<&Yaw>, &mut Contact, Option<&mut Drive>, &Body, Option<&Gravity>, &mut Bounds, &FluidState, Has<Following>, Option<&crate::goals::Jump>, Option<&FluidParams>, Option<&Immune>, Has<Stilled>),
        (With<Moving>, Without<Overlapping>, Without<crate::dragon::flight::NoAi>, Without<crate::mount::Riding>, Without<crate::seat::Seated>, Or<(With<crate::locomotion::WalkMode>, Without<crate::locomotion::Modes>)>),
    >,
) {
    let void_y = f64::from(level.mw.0.world.bottom_y() - 64);
    let one = |e: Entity, mut p: Mut<Pos>, mut v: Mut<Vel>, y: Option<&Yaw>, mut c: Mut<Contact>, d: Option<Mut<Drive>>, b: &Body, gr: Option<&Gravity>, mut bb: Mut<Bounds>, fs: &FluidState, following: bool, jump: Option<&crate::goals::Jump>, fp: Option<&FluidParams>, imm: Option<&Immune>, still: bool| {
        let mut fall = 0.0_f32;
        let jumping = jump.is_some_and(|j| j.0) && !still;
        // A body without walking (an object) has no yaw and no input.
        let mut none = Drive::default();
        let mut d = d;
        let dr: &mut Drive = match d.as_mut() {
            Some(d) => d,
            None => &mut none,
        };
        let was_in_air = !c.on_ground;
        let was_on_ground = c.on_ground;
        let fell = c.fall_distance;
        let rest = travel_one(&level, fp.unwrap_or(&FluidParams::DEFAULT), &mut p, &mut v, y.unwrap_or(&Yaw(0.0)), &mut c, dr, b, gr.map(|g| g.0), &mut bb, fs, jumping, &mut fall);
        // The events of landing are the goals' (an object has none: an orb that glides over the ground would queue two a tick).
        if was_on_ground && !c.on_ground && !b.object {
            par.command_scope(e, |mut cmd| {
                cmd.queue(move |w: &mut World| w.resource_mut::<crate::goals::Events>().left_ground.push(e));
            });
        }
        // `FarmlandBlock.fallOn`: a landing on farmland after a fall is marked sparse (`Landed`), judged once by `landing_trample`.
        if was_in_air && c.on_ground && fell > 0.0 && !b.object {
            let at = ground_block(&level, p.0, bb.0);
            if &*level.mw.0.facts.get(level.mw.0.world.block(at.x, at.y, at.z)).name == "minecraft:farmland" {
                let landed = crate::landing_trample::Landed { at: [at.x, at.y, at.z], fall: f64::from(fell) };
                par.command_scope(e, |mut cmd| {
                    cmd.queue(move |w: &mut World| {
                        if let Ok(mut en) = w.get_entity_mut(e) {
                            en.insert(landed);
                        }
                    });
                });
            }
        }
        if was_in_air && c.on_ground && !b.object {
            // `LivingEntity.checkFallDamage`: the dust of the landed block, pulled to the face the mob straddles.
            let dust = crate::landing_trample::landing_dust_count(f64::from(fell), 3.0).map(|n| {
                let g = ground_block(&level, p.0, bb.0);
                (crate::landing_trample::landing_dust_at(p.0, [g.x, g.z]), level.mw.0.world.block(g.x, g.y, g.z), n)
            });
            par.command_scope(e, |mut cmd| {
                cmd.queue(move |w: &mut World| {
                    w.resource_mut::<crate::goals::Events>().landed.push(e);
                    crate::fall_passengers::land_sound(w, e, f64::from(fell));
                    if let Some((at, state, n)) = dust {
                        w.resource_mut::<crate::goals::Events>().host.block_particles(state, [at.x, at.y, at.z], n, 0.0);
                    }
                });
            });
        }
        if fall > 0.0 && !b.object && !imm.is_some_and(|i| i.has(IMMUNE_FALL)) {
            // `LivingEntity.causeFallDamage`: the fall sound by the damage (big past 4), then the fall sound of the block under the feet
            // at half volume and three quarters of its pitch (`playBlockFallSound`, none in air).
            let at = [p.0.x, p.0.y, p.0.z];
            let g = ground_block(&level, p.0, bb.0);
            let state = level.mw.0.world.block(g.x, g.y, g.z);
            // `Block.fallOn`: the default fall loses its `fallDistanceReduction` of the distance before the damage.
            let fall = fall * (1.0 - level.mw.0.landing.default_fall_reduction(state).unwrap_or(0.0));
            let facts = level.mw.0.facts.get(state);
            let air = &*facts.name == "minecraft:air" || facts.name.ends_with("_air");
            let block_fall = (!air).then(|| (facts.sounds.fall.to_string(), facts.sounds.volume * 0.5, facts.sounds.pitch * 0.75));
            par.command_scope(e, |mut cmd| {
                cmd.queue(move |w: &mut World| {
                    // `calculateFallDamage` past 4 hearts: the big sound; a monster has its own pair (`Monster.getFallSounds`).
                    let big = (fall - 3.0).floor() > 4.0;
                    let hostile = w.get::<crate::landing_trample::HostileFall>(e).is_some();
                    let small_or_big = match (hostile, big) {
                        (true, true) => "entity.hostile.big_fall",
                        (true, false) => "entity.hostile.small_fall",
                        (false, true) => "entity.generic.big_fall",
                        (false, false) => "entity.generic.small_fall",
                    };
                    w.resource_mut::<crate::goals::Events>().host.sound(small_or_big, at, 1.0, 1.0);
                    if let Some((name, volume, pitch)) = block_fall {
                        w.resource_mut::<crate::goals::Events>().host.sound(name, at, volume, pitch);
                    }
                    w.resource_mut::<DamageQueue>().add(e, fall, Cause::Fall);
                    crate::fall_passengers::propagate(w, e, f64::from(fell), 1.0);
                });
            });
        }
        // `Entity.checkBelowWorld`: under the world a creature is hurt 4 a tick (it dies: it does not fall for ever).
        if p.0.y < void_y {
            let object = b.object;
            par.command_scope(e, |mut cmd| {
                cmd.queue(move |w: &mut World| {
                    // An object (orb, item, tnt) is not alive: `Entity.onBelowWorld` discards it; a creature is hurt.
                    if object {
                        w.despawn(e);
                    } else {
                        w.resource_mut::<DamageQueue>().add(e, 4.0, Cause::OutOfWorld);
                    }
                });
            });
        }
        // At rest and not following a path: the physics of the cow goes inactive until something moves it.
        if rest && !following {
            // Not commutative across threads: the entity takes the last row of the table it lands in (or the swap of a sparse set), and the
            // systems that walk it see that order. A parallel visit therefore sorts by emitter; the sequential visit is already in order, and
            // the ordered buffer costs a queue per entity (measured: zombie chase clear +10k instr per tick).
            // The path budget takes "the first of the queue in the table order": it needs the same order as the parallel run, so it is ordered too.
            if cfg.parallel || cfg.path_budget > 0 {
                par.command_scope(e, |mut cmd| {
                    cmd.entity(e).remove::<Moving>();
                });
            } else {
                plain.command_scope(|mut cmd| {
                    cmd.entity(e).remove::<Moving>();
                });
            }
        }
    };
    if cfg.parallel {
        q.par_iter_mut().for_each(|(e, p, v, y, c, d, b, gr, bb, fs, f, j, i, m, st)| one(e, p, v, y, c, d, b, gr, bb, fs, f, j, i, m, st));
    } else {
        for (e, p, v, y, c, d, b, gr, bb, fs, f, j, i, m, st) in &mut q {
            one(e, p, v, y, c, d, b, gr, bb, fs, f, j, i, m, st);
        }
    }
    par.flush();
}
