//! The flight: `FlyingPathNavigation` (the path finder with `Evaluator::Fly`, a search at any moment, a node within 1.0 vertically, the
//! straight line `canMoveDirectly` that crosses fluids), `FlyingMoveControl(maxTurn, hovers)` and `LivingEntity.travelFlying` (no
//! gravity). The `Mob` of a flying animal (parrot, bee) travels as any walker (`travelInAir`) with gravity switched off by its move
//! control while it has somewhere to go (`setNoGravity`); that is what the mode does. In a fluid it falls back on the walk's. The
//! mode is one of the modes a species declares (`air`), next to a ground and a water mode if it has them.
//!
//! A flier does not sleep while it flies (its physics runs every tick, like the fish in the water); what can sleep is a flier at
//! rest (perched, hanging), which is a goal's state and is not in this module.
use crate::IFloor as _;
use crate::{
    behaviours::{Cap, Inst, Module},
    cow::*,
    locomotion::FlyMode,
};
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::path::{Evaluator, Finder, PathMob};
use horizon_core::geometry::{trig, Vec3};
use crate::rules::fly::{aim_push, air_velocity, bank, drag_velocity, face_motion, float_push, free_speed_drag, mean_size, noclip_drag, rest_vy, steer, EPSILON};
use crate::rules::swim::{arrived, is_stuck, moved_sq, node_close, node_reach_xz};

/// How the flier is steered.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Control {
    /// `FlyingMoveControl(max_turn, hovers)` along a path of `FlyingPathNavigation` (parrot, bee).
    Path,
    /// `GhastMoveControl`: a push toward the wanted spot every few ticks when the way is clear; no path (the goal sets the spot).
    Float,
    /// None: a goal or a rule moves the animal itself (a bat's flutter).
    None,
    /// `VexMoveControl`: accelerate straight at the wanted point (`MoveTo`, kept until reached: the animal then stops and the control sleeps),
    /// no path. With `noclip` the move goes through blocks.
    Direct,
    /// `PhantomMoveControl`: bank toward the wanted point (`MoveTo` is the goal's `vector`), faster on a straight line, turned back by a wall.
    Bank,
    /// `WitherBoss` flight: accelerate toward the wanted point at `speed`, blending the velocity by `accel` a tick, no path. The goal sets the point.
    Steer { speed: f64, accel: f64 },
}
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub control: Control,
    /// `LivingEntity.travelFlying` (no gravity at all, the animal is a `flying` type) instead of `travelInAir` with the gravity off while it moves.
    pub free: bool,
    /// The move ignores the blocks (`noPhysics`: a vex).
    pub noclip: bool,
    /// The vertical speed kept each tick after the move (a bat: 0.6); 1 keeps it all.
    pub damp_y: f64,
    /// `FlyingMoveControl.maxTurn`: the degrees a tick the pitch turns by.
    pub max_turn: f32,
    /// `FlyingMoveControl.hoversInPlace` (gravity stays off when there is nowhere to go; the flight has no gravity at all here, so it
    /// is only kept for the module that asks).
    pub hovers: bool,
    /// The factor of a falling flier's vertical speed after its travel (`Parrot.calculateFlapping`: 0.6; 1: none).
    pub glide: f64,
    /// `Allay.travel`: the airborne speed of `travelFlying` is the one the move control set this tick (`Drive.speed`), not the type's.
    pub drive_air: bool,
}
impl Params {
    pub const DEFAULT: Params = Params { control: Control::Path, free: false, noclip: false, damp_y: 1.0, max_turn: 10.0, hovers: false, glide: 1.0, drive_air: false };
}
/// What the flight reads of the animal (from its species, once).
#[derive(Component, Clone, Copy, Debug)]
pub struct FlyParams {
    pub control: Control,
    pub free: bool,
    pub noclip: bool,
    pub damp_y: f64,
    pub max_turn: f32,
    pub hovers: bool,
    pub glide: f64,
    pub drive_air: bool,
    /// `Attributes.FLYING_SPEED`, the speed of a flier that is not on the ground.
    pub flying_speed: f64,
    /// `LivingEntity.getFlyingSpeed` of the type: the `moveRelative` speed in the air.
    pub air_speed: f32,
    /// The vertical drag is 0.91 (`omnidirectional`) instead of 0.98.
    pub vertical_drag: f32,
}

/// `GhastMoveControl.floatDuration`: the ticks to the next push.
#[derive(Component, Clone, Copy, Debug)]
pub struct FloatState(pub i32);

pub static MODULE: Module = Module { name: "fly", requires: &[Cap::Locomotion, Cap::Body], equip, systems, ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = crate::behaviours::params::<Params>(p);
    let Some(sid) = w.get::<SpeciesId>(e).map(|s| s.0) else { return };
    let sp = w.resource::<SpeciesTable>().0[sid as usize];
    if p.control == Control::Float {
        w.entity_mut(e).insert((FloatState(0), Moving));
    }
    if p.control == Control::Bank {
        // `PhantomMoveControl`'s speed starts at the movement speed attribute.
        let sp0 = w.get::<Body>(e).map_or(0.1, |b| b.speed as f32);
        if let Some(mut d) = w.get_mut::<Drive>(e) {
            d.speed = sp0;
        }
        w.entity_mut(e).insert(Moving);
    }
    w.entity_mut(e).insert((FlyParams { control: p.control, free: p.free, noclip: p.noclip, damp_y: p.damp_y, max_turn: p.max_turn, hovers: p.hovers, glide: p.glide, drive_air: p.drive_air, flying_speed: sp.flying_speed, air_speed: sp.air_speed, vertical_drag: if sp.omnidirectional { 0.91 } else { 0.98 } },));
}

fn systems(s: &mut bevy_ecs::schedule::Schedule, _: crate::goals::GoalSlot) {
    use bevy_ecs::schedule::IntoScheduleConfigs;
    s.add_systems((
        pathfind.after(crate::cow::pathfind).before(crate::cow::follow),
        follow.after(crate::cow::follow).before(crate::cow::path_ended),
        move_control.after(crate::cow::move_control),
        fly_controls.after(crate::cow::move_control),
        travel.after(crate::cow::travel).before(crate::cow::ordered_pass),
    ));
}

/// `FlyingPathNavigation.canUpdatePath` is always true: the search is made wherever the animal is.
#[allow(clippy::type_complexity)]
fn pathfind(
    clock: Res<Clock>,
    level: Res<Level>,
    mut par: crate::cow::OrdCmds,
    mut q: Query<(Entity, &Pos, &Body, &Contact, &mut Stroll, &mut Nav, Has<InFluid>, &NavParams), (With<NeedsPath>, With<FlyMode>)>,
) {
    let level = &*level;
    for (e, pos, body, c, mut s, mut nav, fluid, sp) in &mut q {
        let now = clock.of(e);
        let mw = level.mw.0;
        let pm = PathMob {
            position: pos.0,
            width: body.width,
            height: body.height,
            on_ground: c.on_ground,
            in_water: fluid,
            max_up_step: body.step_height,
            max_fall_distance: comfortable_fall_distance(0.0),
            malus: sp.malus,
            can_float: sp.can_float,
            can_open_doors: sp.can_open_doors,
            can_pass_doors: sp.can_pass_doors,
            can_walk_over_fences: sp.can_walk_over_fences,
            stands_on_lava: false,
        };
        let max_len = sp.follow_range;
        let target = [s.tx.ifloor(), s.ty, s.tz.ifloor()];
        nav.searches += 1;
        crate::pathsearch::probe!(crate::goals::FINDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        let path = Finder::new(mw, &pm, Evaluator::Fly).find(&[target], max_len, nav.reach, (max_len * 16.0).ifloor());
        par.command_scope(e, |mut cmd| match path {
            Some(p) if !p.nodes.is_empty() => {
                nav.path = Some(p);
                // `PathNavigation.moveTo` sets `lastStuckCheck` before the tick of the navigation counts (the first tick of a path is `+1`).
                nav.last_check = now.saturating_sub(1);
                nav.last_pos = pos.0;
                cmd.entity(e).remove::<NeedsPath>().insert((Following, Moving));
            }
            _ => {
                nav.path = None;
                s.walking = false;
                cmd.entity(e).remove::<NeedsPath>().insert(PathEnded);
            }
        });
    }
    par.flush();
}

/// Whether a flying path from where the animal is reaches the block `target` (`createPath(pos, reach).reached`): the search `pathfind` makes,
/// without touching the animal (the bee asks it of every flower near it).
pub fn reaches(w: &World, e: Entity, target: [i32; 3], reach: i32) -> bool {
    let (Some(pos), Some(body), Some(c), Some(sp)) = (w.get::<Pos>(e), w.get::<Body>(e), w.get::<Contact>(e), w.get::<NavParams>(e)) else { return false };
    let level = w.resource::<Level>();
    let pm = PathMob {
        position: pos.0,
        width: body.width,
        height: body.height,
        on_ground: c.on_ground,
        in_water: w.get::<FluidState>(e).is_some_and(|f| f.in_water),
        max_up_step: body.step_height,
        max_fall_distance: comfortable_fall_distance(0.0),
        malus: sp.malus,
        can_float: sp.can_float,
        can_open_doors: sp.can_open_doors,
        can_pass_doors: sp.can_pass_doors,
        can_walk_over_fences: sp.can_walk_over_fences,
        stands_on_lava: false,
    };
    let max_len = sp.follow_range;
    Finder::new(level.mw.0, &pm, Evaluator::Fly).find(&[target], max_len, reach, (max_len * 16.0).ifloor()).is_some_and(|p| p.reached)
}

/// `followThePath` of the air: the node is reached within 1.0 vertically, the position is the animal's own, a node seen in a straight
/// line (clear of blocks, fluids included) is taken at once, and the wanted height is the node's.
#[allow(clippy::type_complexity)]
fn follow(
    clock: Res<Clock>,
    level: Res<Level>,
    mut par: crate::cow::OrdCmds,
    mut q: Query<(Entity, &Pos, &Body, &mut Drive, &mut Stroll, &mut Nav, &mut MoveTo), (With<Following>, With<FlyMode>)>,
) {
    let level = &*level;
    for (e, pos, body, mut drive, mut s, mut nav, mut m) in &mut q {
        let now = clock.of(e);
        let mw = level.mw.0;
        let mut end = false;
        match nav.path.as_mut() {
            None => end = true,
            Some(path) => {
                let mob = pos.0;
                let width = body.width;
                let max = node_reach_xz(width);
                let n = path.next_node();
                let (xd, yd, zd) = ((pos.0.x - (f64::from(n.x) + 0.5)).abs(), (pos.0.y - f64::from(n.y)).abs(), (pos.0.z - (f64::from(n.z) + 0.5)).abs());
                let close = node_close(xd, yd, zd, max, 1.0);
                let direct = |from: Vec3, to: Vec3| mw.clear_between(from, Vec3::new(to.x, to.y + f64::from(body.height) * 0.5, to.z), true);
                if close || (horizon_client_lib::mobs::nav::Navigation::can_cut_corner(n.kind) && crate::rules::swim::target_next_node(path, mob, &direct, width)) {
                    path.advance();
                }
                let mut stuck = false;
                if now - nav.last_check > 100 {
                    stuck = is_stuck(drive.speed, moved_sq(mob, nav.last_pos));
                    nav.last_check = now;
                    nav.last_pos = mob;
                }
                let path = nav.path.as_ref().unwrap();
                if stuck || path.is_done() {
                    end = true;
                } else {
                    let target = path.entity_pos_at(path.next, width);
                    *m = MoveTo { active: true, x: target.x, y: target.y, z: target.z, speed_modifier: nav.speed };
                }
            }
        }
        if end {
            nav.path = None;
            s.walking = false;
            drive.zza = 0.0;
            drive.yya = 0.0;
            m.active = false;
            par.command_scope(e, |mut cmd| {
                cmd.entity(e).remove::<Following>().insert(PathEnded);
            });
        }
    }
    par.flush();
}

/// `FlyingMoveControl.tick`: toward the wanted spot, the heading follows at 90 degrees a tick, the speed is the modifier times the
/// attribute (the flying speed off the ground), and the vertical input is the speed up or down.
#[allow(clippy::type_complexity)]
fn move_control(mut q: Query<(&Pos, &mut Yaw, &mut MoveTo, &mut Drive, &Body, &Contact, &FlyParams, Option<&mut crate::goals::Head>), With<FlyMode>>) {
    for (pos, mut yaw, mut m, mut d, body, c, fp, head) in &mut q {
        if fp.control != Control::Path {
            continue;
        }
        if m.active {
            m.active = false;
            d.no_gravity = true;
            let (xd, yd, zd) = (m.x - pos.0.x, m.y - pos.0.y, m.z - pos.0.z);
            if arrived(xd, yd, zd) {
                d.yya = 0.0;
                d.zza = 0.0;
                continue;
            }
            yaw.0 = rotlerp(yaw.0, trig::atan2_degrees(zd, xd) - 90.0, 90.0);
            let speed = (m.speed_modifier * if c.on_ground { body.speed } else { fp.flying_speed }) as f32;
            d.speed = speed;
            d.zza = speed;
            let sd = (xd * xd + zd * zd).sqrt();
            if yd.abs() > EPSILON || sd.abs() > EPSILON {
                let x_rot = -trig::atan2_degrees(yd, sd);
                if let Some(mut h) = head {
                    h.pitch = (rotlerp(h.pitch, x_rot, fp.max_turn) % 360.0).clamp(-90.0, 90.0);
                }
                d.yya = if yd > 0.0 { speed } else { -speed };
            }
        } else {
            if !fp.hovers {
                d.no_gravity = false;
            }
            d.yya = 0.0;
            d.zza = 0.0;
        }
        d.xxa = 0.0;
    }
}

/// `LivingEntity.aiStep` drops the tiny motions BEFORE the step of the AI: a control that changes the velocity (`Float`, a goal's) does it
/// itself first (`drop_tiny_motion`), and the travel does it only for the control that leaves the velocity alone (`Path`).
pub fn drop_tiny_motion(v: &mut Vel) {
    for c in [&mut v.0.x, &mut v.0.y, &mut v.0.z] {
        if c.abs() < 0.003 {
            *c = 0.0;
        }
    }
}
fn drop_tiny(v: &mut Vel, fp: &FlyParams) {
    if fp.control == Control::Path {
        drop_tiny_motion(v);
    }
}

/// The moves controls of the fliers that have one of their own, in ONE system (the controls cost the schedule one system): `Ghast.GhastMoveControl`
/// (a push of `FLYING_SPEED * 5 / 3` toward the wanted spot every 2 to 6 ticks if the way is clear, else the spot is dropped),
/// `VexMoveControl` (accelerate straight at the wanted point; reached = nearer than the mean size of the box: halve the speed and forget it)
/// and `PhantomMoveControl` (turn toward the point by at most 4 degrees a tick; speed up toward 1.8 on a straight line, down toward 0.2 in a
/// turn; a wall turns it round and slows it to 0.1: the speed is `Drive.speed`). The tiny motions are dropped first (`aiStep`).
#[allow(clippy::type_complexity)]
pub fn fly_controls(
    level: Res<Level>,
    mut q: Query<(&Pos, &Bounds, &mut Vel, &mut MoveTo, &mut Yaw, &mut Drive, &Contact, &FlyParams, Option<&mut FloatState>, &mut Rng), (With<FlyMode>, With<Moving>)>,
) {
    for (pos, b, mut v, mut m, mut yaw, mut d, c, fp, st, mut rng) in &mut q {
        match fp.control {
            Control::Float => {
                let Some(mut st) = st else { continue };
                drop_tiny_motion(&mut v);
                if !m.active {
                    continue;
                }
                // `floatDuration-- <= 0`: the test is on the value before the decrement.
                let before = st.0;
                st.0 -= 1;
                if before > 0 {
                    continue;
                }
                st.0 += rng.next_int(5) + 2;
                let t = Vec3::new(m.x - pos.0.x, m.y - pos.0.y, m.z - pos.0.z);
                let len = t.length();
                if len < 1.0e-6 {
                    continue;
                }
                let steps = len.ceil() as i32;
                let clear = (1..=steps).all(|i| {
                    let f = f64::from(i) / f64::from(steps);
                    let dd = Vec3::new(t.x * f, t.y * f, t.z * f);
                    level.no_collision(horizon_core::geometry::Aabb::new(b.0.min_position().add_vector(dd), b.0.max_position().add_vector(dd)))
                });
                if clear {
                    v.0 = float_push(v.0, t, len);
                } else {
                    m.active = false;
                }
            }
            Control::Direct => {
                drop_tiny_motion(&mut v);
                if !m.active {
                    continue;
                }
                let dv = Vec3::new(m.x - pos.0.x, m.y - pos.0.y, m.z - pos.0.z);
                let len = (dv.x * dv.x + dv.y * dv.y + dv.z * dv.z).sqrt();
                let (lo, hi) = (b.0.min_position(), b.0.max_position());
                let size = mean_size(lo, hi);
                if len < size {
                    m.active = false;
                    v.0 = v.0.scale(0.5);
                    continue;
                }
                v.0 = aim_push(v.0, dv, m.speed_modifier, len);
                // Faces along the motion (the `Mob` faces its target when it has one: the heading is not what moves it).
                yaw.0 = face_motion(v.0);
            }
            Control::Bank => {
                drop_tiny_motion(&mut v);
                let mut speed = d.speed;
                if c.horizontal {
                    yaw.0 += 180.0;
                    speed = 0.1;
                }
                let (yaw_v, speed_v, u) = bank(yaw.0, speed, m.x - pos.0.x, m.y - pos.0.y, m.z - pos.0.z, v.0);
                yaw.0 = yaw_v;
                speed = speed_v;
                v.0 = u;
                d.speed = speed;
            }
            Control::Steer { speed, accel } => {
                drop_tiny_motion(&mut v);
                if !m.active {
                    continue;
                }
                let dv = Vec3::new(m.x - pos.0.x, m.y - pos.0.y, m.z - pos.0.z);
                let len = dv.length();
                if len < 1.0e-6 {
                    continue;
                }
                v.0 = steer(v.0, dv, speed, len, accel);
                yaw.0 = face_motion(v.0);
            }
            Control::Path | Control::None => {}
        }
    }
}
/// The goal's wish: fly to this point (the controls `Direct` and `Bank` read it). The animal moves from now on.
pub fn steer_to(w: &mut World, e: Entity, at: [f64; 3], speed: f64) {
    if let Some(mut m) = w.get_mut::<MoveTo>(e) {
        *m = MoveTo { active: true, x: at[0], y: at[1], z: at[2], speed_modifier: speed };
    }
    w.entity_mut(e).insert(Moving);
}
/// `LivingEntity.travelInAir` of a flier: the walk's with the vertical input and, while the move control says so, no gravity; in a
/// fluid it is the walk's travel. Then the glide of a parrot (`Parrot.calculateFlapping`).
#[allow(clippy::type_complexity)]
pub(crate) fn travel(
    level: Res<Level>,
    mut par: crate::cow::OrdCmds,
    mut q: Query<(Entity, &mut Pos, &mut Vel, &Yaw, &mut Contact, &mut Drive, &Body, &mut Bounds, &FluidState, &FluidParams, &Immune, &FlyParams, Has<Following>, &MoveTo), (With<Moving>, Without<Overlapping>, With<FlyMode>)>,
) {
    for (e, mut p, mut v, y, mut c, mut d, b, mut bb, fs, fluid, imm, fp, following, wanted) in &mut q {
        // A flier at rest on the ground (perched) is at rest as a walker is: no sweep, and it leaves the physics until something moves it.
        let rest = rest_vy(b.gravity, fp.vertical_drag);
        if !fs.in_water && !fs.in_lava && c.on_ground && d.zza == 0.0 && d.xxa == 0.0 && d.yya == 0.0 && v.0.x == 0.0 && v.0.z == 0.0 && v.0.y == rest {
            if !following {
                par.command_scope(e, |mut cmd| {
                    cmd.entity(e).remove::<Moving>();
                });
            }
            continue;
        }
        let mut fall = 0.0_f32;
        let was_in_air = !c.on_ground;
        if fp.noclip {
            // `noPhysics`: straight through the blocks, the air's friction.
            p.0 = p.0.add_vector(v.0);
            bb.0 = bb.0.move_by(v.0);
            v.0 = noclip_drag(v.0);
            c.on_ground = false;
            // Nothing wanted and no motion left: out of the physics until a goal steers it (a resting vex costs nothing).
            if !wanted.active && v.0 == Vec3::ZERO {
                par.command_scope(e, |mut cmd| {
                    cmd.entity(e).remove::<Moving>();
                });
            }
            continue;
        }
        if fp.free {
            // `LivingEntity.travelFlying`: no gravity, the drag of the medium (0.91 in the air, 0.8 in water, 0.5 in lava).
            drop_tiny(&mut v, fp);
            let (speed, drag) = free_speed_drag(fs.in_water, fs.in_lava, fp.drive_air, d.speed, d.air_speed.unwrap_or(fp.air_speed));
            move_relative_3d(&mut v, y.0, speed, d.xxa, d.yya, d.zza);
            do_move(&level, &mut p, &mut v, &mut c, b, &mut bb, &mut fall);
            let m = v.0;
            v.0 = drag_velocity(m, drag);
        } else if fs.in_water || fs.in_lava {
            travel_one(&level, fluid, &mut p, &mut v, y, &mut c, &mut d, b, &mut bb, fs, false, &mut fall);
        } else {
            drop_tiny(&mut v, fp);
            let friction: f32 = if c.on_ground { 0.6 } else { 1.0 };
            let speed = if c.on_ground { d.speed } else { fp.air_speed };
            move_relative_3d(&mut v, y.0, speed, d.xxa, d.yya, d.zza);
            do_move(&level, &mut p, &mut v, &mut c, b, &mut bb, &mut fall);
            let m = v.0;
            let gravity = if d.no_gravity { 0.0 } else { b.gravity };
            v.0 = air_velocity(m, friction, fp.vertical_drag, gravity);
        }
        if fp.damp_y != 1.0 {
            v.0.y *= fp.damp_y;
        }
        if fp.glide != 1.0 && !c.on_ground && v.0.y < 0.0 {
            v.0.y *= fp.glide;
        }
        if was_in_air && c.on_ground {
            par.command_scope(e, |mut cmd| {
                cmd.queue(move |w: &mut World| w.resource_mut::<crate::goals::Events>().landed.push(e));
            });
        }
        if fall > 0.0 && !imm.has(IMMUNE_FALL) {
            par.command_scope(e, |mut cmd| {
                cmd.queue(move |w: &mut World| w.resource_mut::<DamageQueue>().add(e, fall, Cause::Fall));
            });
        }
    }
    par.flush();
}
/// `Entity.moveRelative` with a vertical input.
#[doc(hidden)]
pub fn move_relative_3d(vel: &mut Vel, yaw: f32, speed: f32, xxa: f32, yya: f32, zza: f32) {
    let input = Vec3::new(f64::from(xxa), f64::from(yya), f64::from(zza));
    let len_sq = input.x * input.x + input.y * input.y + input.z * input.z;
    if len_sq < 1.0e-7 {
        // `getInputVector` is `Vec3.ZERO`, still added to the motion: a -0.0 becomes 0.0.
        vel.0 = Vec3::new(vel.0.x + 0.0, vel.0.y + 0.0, vel.0.z + 0.0);
        return;
    }
    let l = len_sq.sqrt();
    let i = if len_sq > 1.0 {
        // `Vec3.normalize`: ZERO under 1.0E-5.
        if l < f64::from(1.0e-5_f32) { Vec3::ZERO } else { Vec3::new(input.x / l, input.y / l, input.z / l) }
    } else {
        input
    };
    let i = Vec3::new(i.x * f64::from(speed), i.y * f64::from(speed), i.z * f64::from(speed));
    let angle = f64::from(yaw * trig::DEG_TO_RAD);
    let (s, co) = (f64::from(trig::sin(angle)), f64::from(trig::cos(angle)));
    let u = vel.0;
    vel.0 = Vec3::new(u.x + (i.x * co - i.z * s), u.y + i.y, u.z + (i.z * co + i.x * s));
}
