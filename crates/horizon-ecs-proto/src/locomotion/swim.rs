//! The swim: `WaterBoundPathNavigation` (the path finder with `Evaluator::Swim`, the 0.5 vertical reach of a node, the middle of the
//! body as the position of the mob), `FishMoveControl`, `AbstractFish.travelInWater` and `aiStep`'s flop, `WaterAnimal.handleAirSupply`
//! (the air of an animal that needs water runs down out of it). The mode is active while the animal is in water
//! (`fluid_changed`, an event of `reindex`); out of the water the walk is: a fish on land falls, flops and suffocates.
//!
//! What it supposes of the species (data, not names): `Params { needs_water }`; the speed and the size come from the body.
use crate::{
    behaviours::{Cap, Inst, Module},
    cow::*,
    locomotion::{SwimMode, WalkMode},
};
use bevy_ecs::prelude::*;
use myriade_engine_lib::mobs::path::{Evaluator, Finder, PathMob};
use horizon_core::geometry::{trig, Vec3};
use crate::rules::swim::{arrived, cycle_back, cycle_turned, ease_speed, flop_velocity, guardian_velocity, is_stuck, moved_sq, node_close, node_reach_xz, node_reach_y, sink_rate, squid_dry, squid_swim, swim_drag, target_next_node, tentacle_speed, turn_factor, turtle_ground_speed, wanted_speed, EPSILON};

/// How the animal is steered (the species chooses it by its `Def.locomotion` entry, not by its name).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Control {
    /// `FishMoveControl`: lift in the water, speed an eighth toward the wanted one, climb or dive toward the wanted height.
    Fish,
    /// `Turtle.TurtleMoveControl`, in both media.
    Turtle,
    /// `Drowned.DrownedMoveControl`: while it wants to swim (the marker [`WantsToSwim`], put by its goals) in the water, a push along the vector to
    /// the wanted point (`xd`, `zd` not normalised); otherwise the ground's move control.
    Drowned,
    /// `SmoothSwimmingMoveControl(maxTurnX, maxTurnY, inWater, outsideWater, applyGravity)` (dolphin, axolotl, tadpole, frog): the pitch follows
    /// the wanted point, the forward and the vertical inputs follow the pitch.
    Smooth { max_turn_x: f32, max_turn_y: f32, in_water: f32, outside: f32, gravity: bool },
    /// `Guardian.GuardianMoveControl` (`mobs/guardian.rs:167`): the yaw turns to the wanted point (90 degrees a tick), the speed goes an eighth
    /// toward the wanted one, and the velocity gets a wobble of the age and a push toward the wanted height; no forward input (`zza` 0). With no
    /// target and no move the animal sinks (the travel reads the speed).
    Guardian,
}
/// What `moveRelative` pushes by in the water.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Push {
    /// 0.01 for a fish, 0.1 for the turtle's `travelInWater`.
    Fixed(f32),
    /// The mob's own speed (`Mob.getSpeed`: the dolphin, the axolotl), and the vertical input counts (`Drive.yya`).
    Speed,
}
/// The animal wants to swim (`Drowned.wantsToSwim`: its goal says so, or its target is in the water): put and removed by its goals.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct WantsToSwim;
#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The animal's air runs down out of the water, and it drowns there (`WaterAnimal`: a fish, a squid).
    pub needs_water: bool,
    /// `AmphibiousPathNavigation` (the turtle, the axolotl): the path is searched on land and in water (`Evaluator::Amphibious`, with its
    /// `prefers_shallow`), the node is reached within 1 vertically, a node in a straight line is taken at once only in a liquid.
    pub amphibious: Option<bool>,
    /// `WaterBoundPathNavigation(breach)`: the path is searched out of the water too, the animal may breach (the dolphin).
    pub breach: bool,
    /// The move control of the species: see [`Control`].
    pub control: Control,
    /// What `moveRelative` pushes by in the water: see [`Push`].
    pub push: Push,
    /// `Vec3.add(0.0, -0.005, 0.0)` after the move in the water: an animal with no target sinks a little (a fish, a turtle, a dolphin; not an axolotl).
    pub sinks: bool,
    /// `Squid.travel` and `Squid.aiStep`: no control, no path; the motion is the impulse of the tentacle cycle (`SquidState`), the move only
    /// applies the velocity.
    pub squid: bool,
    /// The look control puts the pitch back to 0 at the end of every tick (`LookControl.resetXRotOnTick`: the frog's, with no tongue target).
    pub reset_pitch: bool,
    /// `AbstractFish.getFlopSound`: the sound of the hop out of the water, at `makeSound`'s pitch; None for an animal that does not flop.
    pub flop: Option<&'static str>,
}
impl Params {
    pub const DEFAULT: Params = Params { needs_water: true, amphibious: None, breach: false, control: Control::Fish, push: Push::Fixed(0.01), sinks: true, squid: false, reset_pitch: false, flop: None };
}
/// The swim's parameters of the entity, as the systems read them.
#[derive(Component, Clone, Copy, Debug)]
pub struct SwimParams {
    pub amphibious: Option<bool>,
    pub breach: bool,
    pub control: Control,
    pub push: Push,
    pub sinks: bool,
    pub squid: bool,
    pub reset_pitch: bool,
    pub flop: Option<&'static str>,
}
/// The tentacle cycle of a squid (`Squid.tentacleMovement`, `tentacleSpeed`) and the vector its goals set (`Mob.extra.vector`): what moves it.
#[derive(Component, Clone, Copy, Debug)]
pub struct SquidState {
    pub movement: f32,
    pub speed: f32,
    pub vector: [f64; 3],
    /// `Squid.yBodyRot`: the body turns toward the movement each tick, and the yaw is set to it.
    pub body_yaw: f32,
}
/// The animal's air runs down while it is out of the water (set by `equip`, with `Dry` out of the water).
#[derive(Component, Clone, Copy, Debug)]
pub struct NeedsWater;
/// The animal is out of the water: its air runs down and it flops (it has `NeedsWater`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Dry;
/// The creature is not a `WaterAnimal`: out of the water its air stays (a guardian's `canBreatheUnderwater`, no `handleAirSupply`).
#[derive(Component, Clone, Copy, Debug)]
pub struct NoDrown;

pub static MODULE: Module = Module { name: "swim", requires: &[Cap::Locomotion, Cap::Living], equip, systems, ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let sp = *crate::behaviours::params::<Params>(p);
    w.entity_mut(e).insert(SwimParams { amphibious: sp.amphibious, breach: sp.breach, control: sp.control, push: sp.push, sinks: sp.sinks, squid: sp.squid, reset_pitch: sp.reset_pitch, flop: sp.flop });
    if sp.squid {
        // `Squid.register`: the cycle speed at birth; the animal is always in the physics (its cycle runs each tick).
        // A creature made by `spawn_declared` gets its stream after its locomotion: the entity's own number seeds a stream until then.
        let speed = match w.get_mut::<crate::cow::Rng>(e) {
            Some(mut r) => tentacle_speed(r.next_float()),
            None => tentacle_speed(crate::cow::Rng(e.to_bits() | 1).next_float()),
        };
        let body_yaw = w.get::<Yaw>(e).map_or(0.0, |y| y.0);
        w.entity_mut(e).insert((SquidState { movement: 0.0, speed, vector: [0.0; 3], body_yaw }, Moving));
    }
    if matches!(sp.control, Control::Guardian) {
        w.entity_mut(e).insert(NoDrown);
    }
    if sp.needs_water {
        // Out of the water until the first event says otherwise.
        w.entity_mut(e).insert((NeedsWater, Dry));
    }
}
/// The entity entered the water (`true`) or left it, at an event of `reindex`.
pub fn water_changed(w: &mut World, e: Entity, in_water: bool) {
    crate::moisture::water_changed(w, e, in_water);
    if w.get::<NeedsWater>(e).is_none() {
        return;
    }
    if in_water {
        w.entity_mut(e).remove::<Dry>();
        if let Some(mut a) = w.get_mut::<Air>(e) {
            // `WaterAnimal.handleAirSupply`: in the water the air is full again.
            a.0 = 300;
            drop(a);
            crate::bridge::view::mark(w, e);
        }
    } else {
        w.entity_mut(e).insert(Dry);
    }
}

fn systems(s: &mut bevy_ecs::schedule::Schedule, _: crate::goals::GoalSlot) {
    use bevy_ecs::schedule::IntoScheduleConfigs;
    s.add_systems((
        pathfind.after(crate::cow::pathfind).before(crate::cow::follow),
        follow.after(crate::cow::follow).before(crate::cow::path_ended),
        move_control.after(crate::cow::move_control),
        dry_air.before(crate::cow::apply_damage::<false>),
        dry.after(crate::cow::wall_check).before(crate::cow::travel),
        travel.after(crate::cow::travel).before(crate::cow::ordered_pass),
    ));
}

/// `WaterBoundPathNavigation`: the search with the swim evaluator, made only in the liquid (`canUpdatePath`).
#[allow(clippy::type_complexity)]
fn pathfind(
    clock: Res<Clock>,
    level: Res<Level>,
    mut par: crate::cow::OrdCmds,
    mut q: Query<(Entity, &Pos, &Body, &Contact, &mut Stroll, &mut Nav, Has<InFluid>, &NavParams, &SwimParams), (With<NeedsPath>, With<SwimMode>)>,
) {
    let level = &*level;
    for (e, pos, body, c, mut s, mut nav, fluid, sp, swp) in &mut q {
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
        let target = [s.tx.floor() as i32, s.ty, s.tz.floor() as i32];
        let evaluator = match swp.amphibious {
            Some(prefers_shallow) => Evaluator::Amphibious { prefers_shallow },
            None => Evaluator::Swim { breach: swp.breach },
        };
        // `canUpdatePath`: the water's navigation only in the liquid, the amphibious one anywhere.
        let path = (fluid || swp.amphibious.is_some() || swp.breach)
            .then(|| {
                nav.searches += 1;
                crate::pathsearch::probe!(crate::goals::FINDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
                Finder::new(mw, &pm, evaluator).find(&[target], max_len, nav.reach, (max_len * 16.0).floor() as i32)
            })
            .flatten();
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

/// `PathNavigation.tick` -> `followThePath` of the water: the node is reached within 0.5 vertically, the mob is the middle of its body,
/// and a node that the mob sees in a straight line (`canMoveDirectly`) is taken at once.
#[allow(clippy::type_complexity)]
fn follow(
    clock: Res<Clock>,
    level: Res<Level>,
    mut par: crate::cow::OrdCmds,
    mut q: Query<(Entity, &Pos, &Body, &mut Drive, &mut Stroll, &mut Nav, &mut MoveTo, &SwimParams, &FluidState), (With<Following>, With<SwimMode>)>,
) {
    let level = &*level;
    for (e, pos, body, mut drive, mut s, mut nav, mut m, swp, fs) in &mut q {
        let now = clock.of(e);
        let mw = level.mw.0;
        let mut end = false;
        let vertical = node_reach_y(swp.amphibious.is_some());
        let liquid = swp.amphibious.is_none() || fs.in_water || fs.in_lava;
        match nav.path.as_mut() {
            None => end = true,
            Some(path) => {
                let mob = Vec3::new(pos.0.x, pos.0.y + f64::from(body.height) * 0.5, pos.0.z);
                let width = body.width;
                let max = node_reach_xz(width);
                let n = path.next_node();
                let (xd, yd, zd) = ((pos.0.x - (f64::from(n.x) + 0.5)).abs(), (pos.0.y - f64::from(n.y)).abs(), (pos.0.z - (f64::from(n.z) + 0.5)).abs());
                let close = node_close(xd, yd, zd, max, vertical);
                let direct = |from: Vec3, to: Vec3| liquid && mw.clear_between(from, Vec3::new(to.x, to.y + f64::from(body.height) * 0.5, to.z), false);
                if close || (myriade_engine_lib::mobs::nav::Navigation::can_cut_corner(n.kind) && target_next_node(path, mob, &direct, width)) {
                    path.advance();
                }
                // `doStuckDetection`, as the walk's.
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
            drive.jump = false;
            m.active = false;
            par.command_scope(e, |mut cmd| {
                cmd.entity(e).remove::<Following>().insert(PathEnded);
            });
        }
    }
    par.flush();
}

/// `FishMoveControl.tick`, every tick of a fish in the water: the water lifts it a little, and while a path is followed the speed
/// goes toward the wanted one by an eighth of the gap, it climbs or dives toward the wanted height and turns toward the node.
#[allow(clippy::type_complexity)]
fn move_control(level: Res<Level>, clock: Res<crate::cow::Clock>, mut q: Query<(Entity, &Pos, &mut Vel, &mut Yaw, &mut MoveTo, &mut Drive, &Body, &FluidState, &Contact, &SwimParams, Has<Following>, Has<WantsToSwim>), With<SwimMode>>) {
    for (e, pos, mut vel, mut yaw, mut m, mut d, body, fs, c, swp, following, wants) in &mut q {
        // `LivingEntity.aiStep` drops the tiny motions BEFORE the control (the lift of the water comes after).
        crate::locomotion::fly::drop_tiny_motion(&mut vel);
        if swp.squid {
            continue;
        }
        match swp.control {
            Control::Fish => {}
            Control::Turtle => {
                turtle_control(pos, &mut vel, &mut yaw, &mut m, &mut d, body, fs, c, following);
                continue;
            }
            Control::Drowned => {
                drowned_control(&level, pos, &mut vel, &mut yaw, &mut m, &mut d, body, fs, c, following, wants);
                continue;
            }
            Control::Guardian => {
                guardian_control(&clock, e, pos, &mut vel, &mut yaw, &mut m, &mut d, body, following);
                continue;
            }
            Control::Smooth { max_turn_x, max_turn_y, in_water, outside, gravity } => {
                if swp.reset_pitch {
                    d.pitch = 0.0;
                }
                smooth_control(pos, &mut vel, &mut yaw, &mut m, &mut d, fs, body, following, max_turn_x, max_turn_y, in_water, outside, gravity);
                continue;
            }
        }
        if fs.eye_in_water {
            vel.0.y += 0.005;
        }
        d.xxa = 0.0;
        if m.active && following {
            let speed = ease_speed(d.speed, wanted_speed(m.speed_modifier, body.speed));
            d.speed = speed;
            d.zza = speed;
            let (xd, yd, zd) = (m.x - pos.0.x, m.y - pos.0.y, m.z - pos.0.z);
            if yd != 0.0 {
                let dd = (xd * xd + yd * yd + zd * zd).sqrt();
                vel.0.y += f64::from(d.speed) * (yd / dd) * 0.1;
            }
            if xd != 0.0 || zd != 0.0 {
                yaw.0 = rotlerp(yaw.0, trig::atan2_degrees(zd, xd) - 90.0, 90.0);
            }
        } else {
            d.speed = 0.0;
            d.zza = 0.0;
        }
        m.active = false;
    }
}

/// `GuardianMoveControl.tick`: swim at the wanted point with a wobble. The Mob draws the age and the id: here the entity's own time and its index.
#[allow(clippy::too_many_arguments)]
fn guardian_control(clock: &crate::cow::Clock, e: Entity, pos: &Pos, vel: &mut Vel, yaw: &mut Yaw, m: &mut MoveTo, d: &mut Drive, body: &Body, following: bool) {
    d.xxa = 0.0;
    if !(m.active && following) {
        d.speed = 0.0;
        d.zza = 0.0;
        d.moving = false;
        m.active = false;
        return;
    }
    d.moving = true;
    let (dx, dy, dz) = (m.x - pos.0.x, m.y - pos.0.y, m.z - pos.0.z);
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    let yd = dy / len;
    yaw.0 = rotlerp(yaw.0, trig::atan2_degrees(dz, dx) - 90.0, 90.0);
    let speed = ease_speed(d.speed, wanted_speed(m.speed_modifier, body.speed));
    d.speed = speed;
    d.zza = speed;
    let t = (clock.of(e) as i64 + i64::from(e.index().index())) as f64;
    vel.0 = guardian_velocity(vel.0, t, yaw.0, yd, speed);
    m.active = false;
}

/// `SmoothSwimmingMoveControl.tick`: in the water the pitch goes toward the wanted point (at most `max_turn_x`, 5 degrees a tick) and the
/// inputs follow it; out of the water the speed falls with the turn.
#[allow(clippy::too_many_arguments)]
fn smooth_control(pos: &Pos, vel: &mut Vel, yaw: &mut Yaw, m: &mut MoveTo, d: &mut Drive, fs: &FluidState, body: &Body, following: bool, max_turn_x: f32, max_turn_y: f32, in_water: f32, outside: f32, gravity: bool) {
    if gravity && fs.in_water {
        vel.0.y += 0.005;
    }
    if m.active && following {
        let (xd, yd, zd) = (m.x - pos.0.x, m.y - pos.0.y, m.z - pos.0.z);
        if arrived(xd, yd, zd) {
            d.zza = 0.0;
        } else {
            let y_rot = trig::atan2_degrees(zd, xd) - 90.0;
            yaw.0 = rotlerp(yaw.0, y_rot, max_turn_y);
            let speed = wanted_speed(m.speed_modifier, body.speed);
            if fs.in_water {
                d.speed = speed * in_water;
                let sd = (xd * xd + zd * zd).sqrt();
                if yd.abs() > EPSILON || sd.abs() > EPSILON {
                    let x_rot = (-trig::atan2_degrees(yd, sd)).clamp(-max_turn_x, max_turn_x);
                    let x_rot = crate::cow::wrap(x_rot).clamp(-max_turn_x, max_turn_x);
                    d.pitch += crate::cow::wrap(x_rot - d.pitch).clamp(-5.0, 5.0);
                }
                let angle = f64::from(d.pitch * trig::DEG_TO_RAD);
                let (c, s) = (trig::cos(angle), trig::sin(angle));
                d.zza = c * speed;
                d.yya = -s * speed;
            } else {
                let left = crate::cow::wrap(yaw.0 - y_rot).abs();
                let factor = turn_factor(left);
                d.speed = speed * outside * factor;
                d.zza = d.speed;
            }
        }
    } else {
        d.speed = 0.0;
        d.xxa = 0.0;
        d.yya = 0.0;
        d.zza = 0.0;
    }
    m.active = false;
}
/// `Drowned.DrownedMoveControl.tick`: swimming (it wants to, in the water): a lift of 0.002, the speed an eighth toward the wanted one, and the
/// velocity pushed along the vector to the point (`x` and `z` NOT normalised); otherwise the ground's move control, with its small fall.
#[allow(clippy::too_many_arguments)]
fn drowned_control(level: &Level, pos: &Pos, vel: &mut Vel, yaw: &mut Yaw, m: &mut MoveTo, d: &mut Drive, body: &Body, fs: &FluidState, c: &Contact, following: bool, wants: bool) {
    if wants && fs.in_water {
        vel.0.y += 0.002;
        if !(m.active && following) {
            d.speed = 0.0;
            d.zza = 0.0;
            m.active = false;
            return;
        }
        let (xd, yd, zd) = (m.x - pos.0.x, m.y - pos.0.y, m.z - pos.0.z);
        let dd = (xd * xd + yd * yd + zd * zd).sqrt();
        let yn = yd / dd;
        yaw.0 = rotlerp(yaw.0, trig::atan2_degrees(zd, xd) - 90.0, 90.0);
        let speed = ease_speed(d.speed, wanted_speed(m.speed_modifier, body.speed));
        d.speed = speed;
        d.zza = speed;
        let s = f64::from(speed);
        vel.0 = Vec3::new(vel.0.x + s * xd * 0.005, vel.0.y + s * yn * 0.1, vel.0.z + s * zd * 0.005);
        m.active = false;
    } else {
        if !c.on_ground {
            vel.0.y -= 0.008;
        }
        crate::cow::move_control_one(level, pos, yaw, m, d, body, c.on_ground || fs.in_water || fs.in_lava);
    }
}
/// `Turtle.TurtleMoveControl.tick` (an adult at home: no home yet): in the water the lift of 0.005; on the ground the speed is halved each tick
/// (never under 0.06); no path or no wanted spot: stopped; else the speed eases an eighth toward the wanted one, the turtle faces the spot
/// and is lifted toward it. Used in both media.
#[allow(clippy::too_many_arguments)]
fn turtle_control(pos: &Pos, vel: &mut Vel, yaw: &mut Yaw, m: &mut MoveTo, d: &mut Drive, body: &Body, fs: &FluidState, c: &Contact, following: bool) {
    d.xxa = 0.0;
    if fs.in_water {
        vel.0.y += 0.005;
    } else if c.on_ground {
        let s = turtle_ground_speed(d.speed);
        d.speed = s;
        d.zza = s;
    }
    if !(m.active && following) {
        d.speed = 0.0;
        d.zza = 0.0;
        m.active = false;
        return;
    }
    let (xd, yd, zd) = (m.x - pos.0.x, m.y - pos.0.y, m.z - pos.0.z);
    let dist = (xd * xd + yd * yd + zd * zd).sqrt();
    m.active = false;
    if dist < f64::from(1.0e-5_f32) {
        d.speed = 0.0;
        d.zza = 0.0;
        return;
    }
    yaw.0 = rotlerp(yaw.0, trig::atan2_degrees(zd, xd) - 90.0, 90.0);
    let speed = ease_speed(d.speed, wanted_speed(m.speed_modifier, body.speed));
    d.speed = speed;
    d.zza = speed;
    vel.0.y += f64::from(speed) * (yd / dist) * 0.1;
}

/// `WaterAnimal.handleAirSupply` out of the water (the air runs down, drowning hurts 2 at -20), at the start of the entity's tick as
/// `baseTick` is, so the hit lands in the same tick as the air reaching 0. A dead body is given its air back (`isAlive` is false).
fn dry_air(
    level: Res<Level>,
    mut q: Query<(Entity, &mut Air, &crate::goals::Uid, &TypeName, Has<crate::tear_down::Crumbling>), (With<Dry>, Without<NoDrown>)>,
    mut queue: ResMut<DamageQueue>,
    mut commands: Commands,
) {
    let cx = level.mw.0.loop_context;
    for (e, mut air, uid, tn, dead) in &mut q {
        let air_before = air.0;
        if dead {
            air.0 = 300;
        } else if let Some(damage) = myriade_engine_lib::breathing::own_air(cx, uid.0 as u32, tn.0, &mut air.0, false, 300) {
            queue.add(e, damage, Cause::Drown);
        }
        if air.0 != air_before {
            commands.queue(move |w: &mut World| crate::bridge::view::mark(w, e));
        }
    }
}

/// `AbstractFish.aiStep`'s flop: an animal on the ground out of the water jumps a little at random.
#[allow(clippy::type_complexity)]
fn dry(
    mut q: Query<(Entity, &mut Air, &crate::goals::Uid, &TypeName, &mut Contact, &mut Vel, &mut Rng, Has<Moving>, &SwimParams, &Pos, Has<crate::behaviours::elder_curse::Silent>, &mut Yaw), With<Dry>>,
    mut events: ResMut<crate::goals::Events>,
    mut commands: Commands,
) {
    for (e, _, _, _, mut c, mut v, mut rng, moving, swp, at, silent, mut yaw) in &mut q {
        // `Guardian.aiStep` out of the water, on the ground: +/-0.4 sideways, 0.5 up, a turn at random (no sound, no ceiling test).
        if c.on_ground && matches!(swp.control, Control::Guardian) {
            let (a, b) = (rng.next_float(), rng.next_float());
            v.0 = Vec3::new(v.0.x + f64::from((a * 2.0 - 1.0) * 0.4), v.0.y + 0.5, v.0.z + f64::from((b * 2.0 - 1.0) * 0.4));
            yaw.0 = rng.next_float() * 360.0;
            c.on_ground = false;
        } else if c.on_ground && !swp.squid {
            // The flop is `AbstractFish.aiStep`'s: a squid only suffocates.
            let (a, b) = (rng.next_f64() as f32, rng.next_f64() as f32);
            v.0 = flop_velocity(v.0, a, b);
            c.on_ground = false;
            // `makeSound(getFlopSound())`: the pitch's two draws come whether the fish is silent or not.
            if let Some(name) = swp.flop {
                let pitch = ((rng.next_u32() >> 8) as f32 / 16_777_216.0 - (rng.next_u32() >> 8) as f32 / 16_777_216.0) * 0.2 + 1.0;
                if !silent {
                    events.host.sound(name, [at.0.x, at.0.y, at.0.z], 1.0, pitch);
                }
            }
        }
        if !moving {
            commands.entity(e).insert(Moving);
        }
    }
}

/// `Squid.aiStep` (server side): the tentacle cycle advances by its speed; in the water the stroke from 3/4 of the first half sets the velocity
/// to the goals' vector, the second half slows it by 0.9; out of the water it falls straight down (no control).
fn squid_step(st: &mut SquidState, v: &mut Vel, rng: &mut Rng, in_water: bool, gravity: f64, yaw: &mut Yaw) {
    st.movement += st.speed;
    if cycle_turned(st.movement) {
        st.movement = cycle_back(st.movement);
        if rng.next_int(10) == 0 {
            st.speed = tentacle_speed(rng.next_float());
        }
    }
    if in_water {
        v.0 = squid_swim(st.movement, st.vector, v.0);
    } else {
        v.0 = squid_dry(v.0, gravity);
    }
    // The body turns toward the movement after the stroke (`Squid.aiStep`: `yBodyRot += (-atan2(x, z) - yBodyRot) * 0.1`, then `setYRot`).
    st.body_yaw += (-trig::atan2_degrees(v.0.x, v.0.z) - st.body_yaw) * 0.1;
    yaw.0 = st.body_yaw;
}

/// `AbstractFish.travelInWater` of an animal in the water; out of it (partly) the walk's `travel_one` (gravity on the ground).
#[allow(clippy::type_complexity)]
fn travel(
    level: Res<Level>,
    mut par: crate::cow::OrdCmds,
    targets: Query<(), With<crate::targeting::Target>>,
    following: Query<(), With<Following>>,
    mut q: Query<
        (Entity, &mut Pos, &mut Vel, &mut Yaw, &mut Contact, &mut Drive, (&Body, Option<&Gravity>, Has<crate::no_gravity::NoGravity>), &mut Bounds, &FluidState, &FluidParams, &Immune, &SwimParams, Option<&mut SquidState>, &mut Rng, Has<WantsToSwim>),
        (With<Moving>, Without<Overlapping>, Without<crate::dragon::flight::NoAi>, With<SwimMode>),
    >,
) {
    for (e, mut p, mut v, mut y, mut c, mut d, (b, gr, nog), mut bb, fs, fp, imm, swp, squid, mut rng, wants) in &mut q {
        let has_target = targets.contains(e);
        let mut fall = 0.0_f32;
        let was_in_air = !c.on_ground;
        if swp.squid {
            // `Squid.travel`: the move by the velocity and nothing else; then `Squid.aiStep` (the species' step, after the travel).
            // `LivingEntity.aiStep` drops the tiny motions before the move. The fluid state is the one `baseTick` read at the start of the tick
            // (the entity's own `FluidState` is behind by a tick when it enters the water from above: its marker comes with the move), read again after
            // the move when it was dry.
            crate::locomotion::fly::drop_tiny_motion(&mut v);
            let mut in_water = level.mw.0.fluids(bb.0, p.0.y + fp.eye_height).in_water;
            do_move(&level, &mut p, &mut v, &mut c, b, &mut bb, &mut fall);
            // `LivingEntity.checkFallDamage` (inside the move): out of the water the fluid state is read again at the new place, and `Squid.aiStep`
            // (after the travel) sees the squid in the water that it has just entered from above.
            if !in_water {
                in_water = level.mw.0.fluids(bb.0, p.0.y + fp.eye_height).in_water;
            }
            if let Some(mut st) = squid {
                squid_step(&mut st, &mut v, &mut rng, in_water, gr.filter(|_| !nog).map_or(0.0, |g| g.0), &mut *y);
            }
            continue;
        }
        // A drowned swims only under water and when it wants to (`Drowned.travelInWater`); else the generic travel of the fluid.
        let swims = fs.in_water && (swp.control != Control::Drowned || (fs.was_eye_in_water && wants));
        if swims {
            if !c.on_ground && v.0.y < 0.0 {
                v.0.y *= b.fall_factor;
            }
            let push = match swp.push {
                Push::Fixed(p) => p,
                Push::Speed => d.speed,
            };
            if swp.push == Push::Speed {
                crate::locomotion::fly::move_relative_3d(&mut v, y.0, push, d.xxa, d.yya, d.zza);
            } else {
                move_relative(&mut v, y.0, push, d.xxa, d.yya, d.zza);
            }
            do_move(&level, &mut p, &mut v, &mut c, b, &mut bb, &mut fall);
            let m = v.0;
            // `Vec3.add(0.0, -0.005, 0.0)`: an animal with no target sinks a little.
            let sink = sink_rate(swp.sinks, swp.control == Control::Guardian, d.speed == 0.0 && !has_target);
            v.0 = swim_drag(m, sink);
        } else {
            let rest = crate::cow::travel_one_with(&level, fp, &mut p, &mut v, &y, &mut c, &mut d, b, gr.filter(|_| !nog).map(|g| g.0), &mut bb, fs, false, &mut fall, false);
            // At rest out of the water (on the ground, still) and not following a path: inactive until something moves it, as the cow's physics.
            if rest && !following.contains(e) {
                par.command_scope(e, |mut cmd| {
                    cmd.entity(e).remove::<Moving>();
                });
            }
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

#[allow(dead_code)]
fn _marks(_: WalkMode) {}
