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

/// `ItemEntity.tick`'s motion for an object: a fluid slows it (water 0.99, lava 0.95, lifting a slow one by 5.0E-4) or gravity pulls it
/// down BEFORE the move; it moves unless it lies still on the ground; then the drag of the air (0.98 on every axis, times the block's friction
/// on the ground for x and z) and the bounce (a downward speed on the ground is halved and turned up). True when it is at rest. An object that
/// walks (`Drive`) is accelerated as a creature is, before the move.
#[allow(clippy::too_many_arguments)]
pub(crate) fn travel_object(level: &Level, pos: &mut Pos, vel: &mut Vel, yaw: &Yaw, c: &mut Contact, d: &mut Drive, body: &Body, bounds: &mut Bounds, fs: &FluidState, fall_damage: &mut f32) -> bool {
    if body.tnt {
        return crate::tnt::travel_tnt(level, pos, vel, c, body, bounds, fall_damage);
    }
    if body.orb {
        return travel_orb(level, pos, vel, c, d, body, bounds, fs, fall_damage);
    }
    let height = f64::from(0.1_f32);
    let (in_water, in_lava) = (fs.water > height, fs.lava > height);
    if in_water || in_lava {
        let drag = f64::from(if in_water { 0.99_f32 } else { 0.95_f32 });
        let lift = if vel.0.y < f64::from(0.06_f32) { f64::from(5.0e-4_f32) } else { 0.0 };
        vel.0 = Vec3::new(vel.0.x * drag, vel.0.y + lift, vel.0.z * drag);
    } else {
        vel.0.y -= body.gravity;
    }
    if d.zza != 0.0 || d.xxa != 0.0 {
        let speed = if c.on_ground { d.speed } else { 0.02 };
        move_relative(vel, yaw.0, speed, d.xxa, d.yya, d.zza);
    }
    let horizontal = vel.0.x * vel.0.x + vel.0.z * vel.0.z;
    if c.on_ground && horizontal <= 1.0e-5 && d.zza == 0.0 && d.xxa == 0.0 && vel.0.y <= 0.0 && vel.0.y > -body.gravity * 1.01 {
        // Lying still on the ground: the gravity of this tick is not integrated, nothing moves.
        vel.0 = Vec3::ZERO;
        return true;
    }
    do_move(level, pos, vel, c, body, bounds, fall_damage);
    // `Entity.move`'s last step: the horizontal velocity scaled by the block's speed factor, read only when it moves sideways.
    if vel.0.x != 0.0 || vel.0.z != 0.0 {
        let f = f64::from(crate::block_factor::speed(level.mw.0, [pos.0.x, pos.0.y, pos.0.z]));
        vel.0 = Vec3::new(vel.0.x * f, vel.0.y, vel.0.z * f);
    }
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
pub(crate) fn travel_orb(level: &Level, pos: &mut Pos, vel: &mut Vel, c: &mut Contact, d: &mut Drive, body: &Body, bounds: &mut Bounds, fs: &FluidState, fall_damage: &mut f32) -> bool {
    let mw = level.mw.0;
    if fs.eye_in_water {
        vel.0 = Vec3::new(vel.0.x * f64::from(0.99_f32), (vel.0.y + f64::from(5.0e-4_f32)).min(f64::from(0.06_f32)), vel.0.z * f64::from(0.99_f32));
    } else if level.no_collision(bounds.0) {
        vel.0.y -= body.gravity;
    }
    let horizontal = vel.0.x * vel.0.x + vel.0.z * vel.0.z;
    if c.on_ground && !fs.in_water && horizontal <= 1.0e-10 && vel.0.y <= 0.0 && vel.0.y > -body.gravity * 1.01 {
        vel.0 = Vec3::ZERO;
        return true;
    }
    let wanted_y = vel.0.y;
    do_move(level, pos, vel, c, body, bounds, fall_damage);
    // `Entity.move`'s last step, as for an object: the horizontal velocity scaled by the block's speed factor.
    if vel.0.x != 0.0 || vel.0.z != 0.0 {
        let f = f64::from(crate::block_factor::speed(mw, [pos.0.x, pos.0.y, pos.0.z]));
        vel.0 = Vec3::new(vel.0.x * f, vel.0.y, vel.0.z * f);
    }
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

/// `LivingEntity.travel` of a cow: in the air (on the ground), or in water or lava (`travelInFluid`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn travel_one(level: &Level, sp: &FluidParams, pos: &mut Pos, vel: &mut Vel, yaw: &Yaw, c: &mut Contact, d: &mut Drive, body: &Body, bounds: &mut Bounds, fs: &FluidState, jumping: bool, fall_damage: &mut f32) -> bool {
    travel_one_with(level, sp, pos, vel, yaw, c, d, body, bounds, fs, jumping, fall_damage, true)
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
    bounds: &mut Bounds,
    fs: &FluidState,
    jumping: bool,
    fall_damage: &mut f32,
    drop_tiny: bool,
) -> bool {
    if body.object {
        return travel_object(level, pos, vel, yaw, c, d, body, bounds, fs, fall_damage);
    }
    let rest_vy = (0.0 - body.gravity) * f64::from(0.98_f32);
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
            let m = vel.0;
            let m = Vec3::new(m.x * f64::from(slow), m.y * f64::from(0.8_f32), m.z * f64::from(slow));
            vel.0 = horizon_client_lib::mobs::living::fluid_falling(body.gravity, falling, false, m);
            c.fall_distance = 0.0;
        } else {
            move_relative(vel, yaw.0, 0.02, d.xxa, d.yya, d.zza);
            do_move(level, pos, vel, c, body, bounds, fall_damage);
            let v = vel.0;
            if fs.lava <= threshold {
                vel.0 = Vec3::new(v.x * 0.5, v.y * f64::from(0.8_f32), v.z * 0.5);
                vel.0 = horizon_client_lib::mobs::living::fluid_falling(body.gravity, falling, false, vel.0);
            } else {
                vel.0 = Vec3::new(v.x * 0.5, v.y * 0.5, v.z * 0.5);
            }
            if body.gravity != 0.0 {
                vel.0.y -= body.gravity / 4.0;
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
    let friction: f32 = if c.on_ground { 0.6 } else { 1.0 };
    let speed = if c.on_ground {
        if friction > 0.6 { d.speed * (0.216_000_02 / (friction * friction * friction)) } else { d.speed }
    } else {
        d.air_speed.unwrap_or(0.02)
    };
    move_relative(vel, yaw.0, speed, d.xxa, d.yya, d.zza);
    // `LivingEntity.handleOnClimbable`, the move, then the climb up a wall (a wall climber pressed against one).
    let climbing = d.climber && d.climbing;
    if climbing {
        c.fall_distance = 0.0;
        let limit = f64::from(0.15_f32);
        let u = vel.0;
        vel.0 = Vec3::new(u.x.clamp(-limit, limit), if u.y.is_nan() { u.y } else { u.y.max(-limit) }, u.z.clamp(-limit, limit));
    }
    do_move(level, pos, vel, c, body, bounds, fall_damage);
    let mut movement = vel.0;
    if climbing && (c.horizontal || jumping) {
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
        movement.y - body.gravity
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
        (Entity, &mut Pos, &mut Vel, Option<&Yaw>, &mut Contact, Option<&mut Drive>, &Body, &mut Bounds, &FluidState, Has<Following>, Option<&crate::goals::Jump>, Option<&FluidParams>, Option<&Immune>, Has<Stilled>),
        (With<Moving>, Without<Overlapping>, Without<crate::dragon::flight::NoAi>, Without<crate::mount::Riding>, Or<(With<crate::locomotion::WalkMode>, Without<crate::locomotion::Modes>)>),
    >,
) {
    let void_y = f64::from(level.mw.0.world.bottom_y() - 64);
    let one = |e: Entity, mut p: Mut<Pos>, mut v: Mut<Vel>, y: Option<&Yaw>, mut c: Mut<Contact>, d: Option<Mut<Drive>>, b: &Body, mut bb: Mut<Bounds>, fs: &FluidState, following: bool, jump: Option<&crate::goals::Jump>, fp: Option<&FluidParams>, imm: Option<&Immune>, still: bool| {
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
        let rest = travel_one(&level, fp.unwrap_or(&FluidParams::DEFAULT), &mut p, &mut v, y.unwrap_or(&Yaw(0.0)), &mut c, dr, b, &mut bb, fs, jumping, &mut fall);
        // The events of landing are the goals' (an object has none: an orb that glides over the ground would queue two a tick).
        if was_on_ground && !c.on_ground && !b.object {
            par.command_scope(e, |mut cmd| {
                cmd.queue(move |w: &mut World| w.resource_mut::<crate::goals::Events>().left_ground.push(e));
            });
        }
        if was_in_air && c.on_ground && !b.object {
            par.command_scope(e, |mut cmd| {
                cmd.queue(move |w: &mut World| {
                    w.resource_mut::<crate::goals::Events>().landed.push(e);
                    crate::fall_passengers::land_sound(w, e, f64::from(fell));
                });
            });
        }
        if fall > 0.0 && !b.object && !imm.is_some_and(|i| i.has(IMMUNE_FALL)) {
            par.command_scope(e, |mut cmd| {
                cmd.queue(move |w: &mut World| {
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
        q.par_iter_mut().for_each(|(e, p, v, y, c, d, b, bb, fs, f, j, i, m, st)| one(e, p, v, y, c, d, b, bb, fs, f, j, i, m, st));
    } else {
        for (e, p, v, y, c, d, b, bb, fs, f, j, i, m, st) in &mut q {
            one(e, p, v, y, c, d, b, bb, fs, f, j, i, m, st);
        }
    }
    par.flush();
}
