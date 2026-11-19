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
use horizon_client_lib::mobs::path::{Evaluator, Finder, PathMob};
use horizon_core::geometry::{trig, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The animal's air runs down out of the water, and it drowns there (`WaterAnimal`: a fish, a squid).
    pub needs_water: bool,
    /// `AmphibiousPathNavigation` (the turtle, the axolotl): the path is searched on land and in water (`Evaluator::Amphibious`, with its
    /// `prefers_shallow`), the node is reached within 1 vertically, a node in a straight line is taken at once only in a liquid.
    pub amphibious: Option<bool>,
    /// `Turtle.TurtleMoveControl` (in both media) instead of `FishMoveControl`.
    pub turtle_control: bool,
    /// What `moveRelative` pushes by in the water (0.01 for a fish, 0.1 for the turtle's `travelInWater`).
    pub push: f32,
}
impl Params {
    pub const DEFAULT: Params = Params { needs_water: true, amphibious: None, turtle_control: false, push: 0.01 };
}
/// The swim's parameters of the entity, as the systems read them.
#[derive(Component, Clone, Copy, Debug)]
pub struct SwimParams {
    pub amphibious: Option<bool>,
    pub turtle_control: bool,
    pub push: f32,
}
/// The animal's air runs down while it is out of the water (set by `equip`, with `Dry` out of the water).
#[derive(Component, Clone, Copy, Debug)]
pub struct NeedsWater;
/// The animal is out of the water: its air runs down and it flops (it has `NeedsWater`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Dry;

pub static MODULE: Module = Module { name: "swim", requires: &[Cap::Locomotion, Cap::Living], equip, systems, ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    let sp = *crate::behaviours::params::<Params>(p);
    w.entity_mut(e).insert(SwimParams { amphibious: sp.amphibious, turtle_control: sp.turtle_control, push: sp.push });
    if sp.needs_water {
        // Out of the water until the first event says otherwise.
        w.entity_mut(e).insert((NeedsWater, Dry));
    }
}
/// The entity entered the water (`true`) or left it, at an event of `reindex`.
pub fn water_changed(w: &mut World, e: Entity, in_water: bool) {
    if w.get::<NeedsWater>(e).is_none() {
        return;
    }
    if in_water {
        w.entity_mut(e).remove::<Dry>();
        if let Some(mut a) = w.get_mut::<Air>(e) {
            // `WaterAnimal.handleAirSupply`: in the water the air is full again.
            a.0 = 300;
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
        move_control.after(crate::cow::move_control).before(crate::cow::fluid_update),
        dry.after(crate::cow::wall_check).before(crate::cow::travel),
        travel.after(crate::cow::travel).before(crate::cow::ordered_pass),
    ));
}

/// `WaterBoundPathNavigation`: the search with the swim evaluator, made only in the liquid (`canUpdatePath`).
#[allow(clippy::type_complexity)]
fn pathfind(
    clock: Res<Clock>,
    level: Res<Level>,
    par: ParallelCommands,
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
            None => Evaluator::Swim { breach: false },
        };
        // `canUpdatePath`: the water's navigation only in the liquid, the amphibious one anywhere.
        let path = (fluid || swp.amphibious.is_some())
            .then(|| {
                nav.searches += 1;
                crate::goals::FINDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Finder::new(mw, &pm, evaluator).find(&[target], max_len, nav.reach, (max_len * 16.0).floor() as i32)
            })
            .flatten();
        par.command_scope(|mut cmd| match path {
            Some(p) if !p.nodes.is_empty() => {
                nav.path = Some(p);
                nav.last_check = now;
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
}

/// `PathNavigation.tick` -> `followThePath` of the water: the node is reached within 0.5 vertically, the mob is the middle of its body,
/// and a node that the mob sees in a straight line (`canMoveDirectly`) is taken at once.
#[allow(clippy::type_complexity)]
fn follow(
    clock: Res<Clock>,
    level: Res<Level>,
    par: ParallelCommands,
    mut q: Query<(Entity, &Pos, &Body, &mut Drive, &mut Stroll, &mut Nav, &mut MoveTo, &SwimParams, &FluidState), (With<Following>, With<SwimMode>)>,
) {
    let level = &*level;
    for (e, pos, body, mut drive, mut s, mut nav, mut m, swp, fs) in &mut q {
        let now = clock.of(e);
        let mw = level.mw.0;
        let mut end = false;
        let vertical = if swp.amphibious.is_some() { 1.0 } else { 0.5 };
        let liquid = swp.amphibious.is_none() || fs.in_water || fs.in_lava;
        match nav.path.as_mut() {
            None => end = true,
            Some(path) => {
                let mob = Vec3::new(pos.0.x, pos.0.y + f64::from(body.height) * 0.5, pos.0.z);
                let width = body.width;
                let max = f64::from(if width > 0.75 { width / 2.0 } else { 0.75 - width / 2.0 });
                let n = path.next_node();
                let (xd, yd, zd) = ((pos.0.x - (f64::from(n.x) + 0.5)).abs(), (pos.0.y - f64::from(n.y)).abs(), (pos.0.z - (f64::from(n.z) + 0.5)).abs());
                let close = xd < max && zd < max && yd < vertical;
                let direct = |from: Vec3, to: Vec3| liquid && mw.clear_between(from, Vec3::new(to.x, to.y + f64::from(body.height) * 0.5, to.z), false);
                if close || (horizon_client_lib::mobs::nav::Navigation::can_cut_corner(n.kind) && target_next_node(path, mob, &direct, width)) {
                    path.advance();
                }
                // `doStuckDetection`, as the walk's.
                let mut stuck = false;
                if now - nav.last_check > 100 {
                    let speed = drive.speed;
                    let effective = if speed >= 1.0 { speed } else { speed * speed };
                    let threshold = f64::from(effective * 100.0 * 0.25);
                    let d = (mob.x - nav.last_pos.x).powi(2) + (mob.y - nav.last_pos.y).powi(2) + (mob.z - nav.last_pos.z).powi(2);
                    stuck = d < threshold * threshold;
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
            par.command_scope(|mut cmd| {
                cmd.entity(e).remove::<Following>().insert(PathEnded);
            });
        }
    }
}
/// `PathNavigation.shouldTargetNextNode` with the water's `canMoveDirectly`.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub(crate) fn target_next_node(path: &horizon_client_lib::mobs::path::Path, mob: Vec3, direct: &dyn Fn(Vec3, Vec3) -> bool, width: f32) -> bool {
    if path.next + 1 >= path.nodes.len() {
        return false;
    }
    let n = path.nodes[path.next];
    let current = Vec3::new(f64::from(n.x) + 0.5, f64::from(n.y), f64::from(n.z) + 0.5);
    let d = |a: Vec3, b: Vec3| (a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2);
    if d(mob, current) >= 4.0 {
        return false;
    }
    if direct(mob, path.entity_pos_at(path.next, width)) {
        return true;
    }
    let m = path.nodes[path.next + 1];
    let next = Vec3::new(f64::from(m.x) + 0.5, f64::from(m.y), f64::from(m.z) + 0.5);
    let (to_current, to_next) = (Vec3::new(current.x - mob.x, current.y - mob.y, current.z - mob.z), Vec3::new(next.x - mob.x, next.y - mob.y, next.z - mob.z));
    let (cs, ns) = (to_current.length_squared(), to_next.length_squared());
    if !(ns < cs) && !(cs < 0.5) {
        return false;
    }
    let (a, b) = (to_current.normalize(), to_next.normalize());
    a.x * b.x + a.y * b.y + a.z * b.z < 0.0
}

/// `FishMoveControl.tick`, every tick of a fish in the water: the water lifts it a little, and while a path is followed the speed
/// goes toward the wanted one by an eighth of the gap, it climbs or dives toward the wanted height and turns toward the node.
#[allow(clippy::type_complexity)]
fn move_control(mut q: Query<(&Pos, &mut Vel, &mut Yaw, &mut MoveTo, &mut Drive, &Body, &FluidState, &Contact, &SwimParams, Has<Following>), With<SwimMode>>) {
    for (pos, mut vel, mut yaw, mut m, mut d, body, fs, c, swp, following) in &mut q {
        if swp.turtle_control {
            turtle_control(pos, &mut vel, &mut yaw, &mut m, &mut d, body, fs, c, following);
            continue;
        }
        if fs.eye_in_water {
            vel.0.y += 0.005;
        }
        d.xxa = 0.0;
        if m.active && following {
            let target = (m.speed_modifier * body.speed) as f32;
            let speed = d.speed + (target - d.speed) * 0.125;
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

/// `Turtle.TurtleMoveControl.tick` (an adult at home: no home yet): in the water the lift of 0.005; on the ground the speed is halved each tick
/// (never under 0.06); no path or no wanted spot: stopped; else the speed eases an eighth toward the wanted one, the turtle faces the spot
/// and is lifted toward it. Used in both media.
#[allow(clippy::too_many_arguments)]
fn turtle_control(pos: &Pos, vel: &mut Vel, yaw: &mut Yaw, m: &mut MoveTo, d: &mut Drive, body: &Body, fs: &FluidState, c: &Contact, following: bool) {
    d.xxa = 0.0;
    if fs.in_water {
        vel.0.y += 0.005;
    } else if c.on_ground {
        let s = (d.speed / 2.0).max(0.06);
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
    let wanted = (m.speed_modifier * body.speed) as f32;
    let speed = d.speed + 0.125 * (wanted - d.speed);
    d.speed = speed;
    d.zza = speed;
    vel.0.y += f64::from(speed) * (yd / dist) * 0.1;
}

/// `WaterAnimal.handleAirSupply` out of the water (the air runs down, drowning hurts 2 at -20) and `AbstractFish.aiStep`'s flop: an
/// animal on the ground out of the water jumps a little at random.
#[allow(clippy::type_complexity)]
fn dry(
    level: Res<Level>,
    mut q: Query<(Entity, &mut Air, &crate::goals::Uid, &TypeName, &mut Contact, &mut Vel, &mut Rng, Has<Moving>), With<Dry>>,
    mut queue: ResMut<DamageQueue>,
    mut commands: Commands,
) {
    let cx = level.mw.0.loop_context;
    for (e, mut air, uid, tn, mut c, mut v, mut rng, moving) in &mut q {
        if let Some(damage) = horizon_client_lib::breathing::own_air(cx, uid.0 as u32, tn.0, &mut air.0, false, 300) {
            queue.add(e, damage, Cause::Drown);
        }
        if c.on_ground {
            let (a, b) = (rng.next_f64() as f32, rng.next_f64() as f32);
            v.0 = Vec3::new(v.0.x + f64::from((a * 2.0 - 1.0) * 0.05), v.0.y + 0.4, v.0.z + f64::from((b * 2.0 - 1.0) * 0.05));
            c.on_ground = false;
        }
        if !moving {
            commands.entity(e).insert(Moving);
        }
    }
}

/// `AbstractFish.travelInWater` of an animal in the water; out of it (partly) the walk's `travel_one` (gravity on the ground).
#[allow(clippy::type_complexity)]
fn travel(
    level: Res<Level>,
    par: ParallelCommands,
    mut q: Query<
        (Entity, &mut Pos, &mut Vel, &Yaw, &mut Contact, &mut Drive, &Body, &mut Bounds, &FluidState, &FluidParams, &Immune, &SwimParams),
        (With<Moving>, Without<Overlapping>, With<SwimMode>),
    >,
) {
    for (e, mut p, mut v, y, mut c, mut d, b, mut bb, fs, fp, imm, swp) in &mut q {
        let mut fall = 0.0_f32;
        let was_in_air = !c.on_ground;
        if fs.in_water {
            if !c.on_ground && v.0.y < 0.0 {
                v.0.y *= b.fall_factor;
            }
            // `LivingEntity.aiStep`: the tiny motions are dropped.
            let u = v.0;
            v.0 = Vec3::new(if u.x.abs() < 0.003 { 0.0 } else { u.x }, if u.y.abs() < 0.003 { 0.0 } else { u.y }, if u.z.abs() < 0.003 { 0.0 } else { u.z });
            move_relative(&mut v, y.0, swp.push, d.xxa, d.zza);
            do_move(&level, &mut p, &mut v, &mut c, b, &mut bb, &mut fall);
            let m = v.0;
            // `Vec3.add(0.0, -0.005, 0.0)`: a fish with no target sinks a little.
            v.0 = Vec3::new(m.x * 0.9, m.y * 0.9 - 0.005, m.z * 0.9);
        } else {
            travel_one(&level, fp, &mut p, &mut v, y, &mut c, &mut d, b, &mut bb, fs, false, &mut fall);
        }
        if was_in_air && c.on_ground {
            par.command_scope(|mut cmd| {
                cmd.queue(move |w: &mut World| w.resource_mut::<crate::goals::Events>().landed.push(e));
            });
        }
        if fall > 0.0 && !imm.has(IMMUNE_FALL) {
            par.command_scope(|mut cmd| {
                cmd.queue(move |w: &mut World| w.resource_mut::<DamageQueue>().add(e, fall, Cause::Fall));
            });
        }
    }
}

#[allow(dead_code)]
fn _marks(_: WalkMode) {}
