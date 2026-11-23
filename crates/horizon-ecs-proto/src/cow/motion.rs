//! The move control, the push and crowding.
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
    MobWorld,
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
use crate::mount::{Ridden, Riding};
use crate::rules::pushable::Unpushable;

pub(crate) fn wrap(a: f32) -> f32 {
    let mut a = a % 360.0;
    if a >= 180.0 {
        a -= 360.0;
    }
    if a < -180.0 {
        a += 360.0;
    }
    a
}

pub(crate) fn rotlerp(a: f32, b: f32, max: f32) -> f32 {
    let mut r = a + wrap(b - a).clamp(-max, max);
    if r < 0.0 {
        r += 360.0;
    } else if r > 360.0 {
        r -= 360.0;
    }
    r
}

/// `MoveControl.Operation.STRAFE` (`MoveControl.strafe(forwards, right)`): the speed is a quarter of the movement speed; a side
/// step whose target block is not walkable turns into a step forward. The caller (a goal) does it every tick it strafes;
/// the walk of the path (`move_control`) sets the sideways input back to nothing.
pub(crate) fn strafe_drive(level: &Level, pos: &Pos, yaw: &Yaw, body: &Body, d: &mut Drive, forwards: f32, right: f32, walkable: &mut Option<((i32, i32, i32), bool, u64)>, now: u64) {
    let speed = (0.25 * body.speed) as f32;
    let (mut xa, mut za) = (forwards, right);
    let dist = (xa * xa + za * za).sqrt().max(1.0);
    let k = speed / dist;
    (xa, za) = (xa * k, za * k);
    let angle = f64::from(yaw.0 * trig::DEG_TO_RAD);
    let (s, co) = (trig::sin(angle), trig::cos(angle));
    let (dx, dz) = (xa * co - za * s, za * co + xa * s);
    let (x, y, z) = ((pos.0.x + f64::from(dx)).ifloor(), pos.0.y.ifloor(), (pos.0.z + f64::from(dz)).ifloor());
    // The kind of the block is read once per block and kept for 10 ticks (a strafe step is 0.1 block: the same block for many ticks).
    let ok = match *walkable {
        Some((b, ok, at)) if b == (x, y, z) && now < at + 10 => ok,
        _ => {
            crate::pathsearch::adjacent_reset();
            let ok = crate::pathsearch::path_type_static(level.mw.0, x, y, z) == horizon_client_lib::mobs::path::PathType::Walkable;
            *walkable = Some(((x, y, z), ok, now));
            ok
        }
    };
    let (forwards, right) = if ok { (forwards, right) } else { (1.0, 0.0) };
    d.speed = speed;
    d.zza = forwards;
    d.xxa = right;
}

/// `MoveControl.tick`, operation MOVE_TO (the jump over a step is not here: the slab is flat).
pub(crate) fn move_control_one(level: &Level, pos: &Pos, yaw: &mut Yaw, m: &mut MoveTo, d: &mut Drive, body: &Body, landed: bool) {
    const ARRIVED: f64 = 2.500_000_3e-7_f32 as f64;
    d.jump = false;
    d.xxa = 0.0;
    if d.jumping_op {
        // Operation JUMPING: the speed follows the wanted spot's modifier, the heading is kept, and the next
        // wanted spot does not change the operation, until the cow is on the ground (or in a liquid).
        let speed = (m.speed_modifier * body.speed) as f32;
        d.speed = speed;
        d.zza = speed;
        m.active = false;
        if landed {
            d.jumping_op = false;
        }
        return;
    }
    if !m.active {
        d.zza = 0.0;
        return;
    }
    m.active = false;
    let (xd, yd, zd) = (m.x - pos.0.x, m.y - pos.0.y, m.z - pos.0.z);
    if xd * xd + yd * yd + zd * zd < ARRIVED {
        d.zza = 0.0;
        return;
    }
    if let Some((turn, outside)) = d.smooth_land {
        // The land branch of `SmoothSwimmingMoveControl`: the speed is scaled by `outside` and by how far the body still has to turn; no jump.
        let y_rot = trig::atan2_degrees(zd, xd) - 90.0;
        yaw.0 = rotlerp(yaw.0, y_rot, turn);
        let factor = 1.0 - ((wrap(yaw.0 - y_rot).abs() - 10.0) / 50.0).clamp(0.0, 1.0);
        d.speed = (m.speed_modifier * body.speed) as f32 * outside * factor;
        d.zza = d.speed;
        d.jump = false;
        d.jumping_op = false;
        return;
    }
    yaw.0 = rotlerp(yaw.0, trig::atan2_degrees(zd, xd) - 90.0, 90.0);
    let speed = (m.speed_modifier * body.speed) as f32;
    d.speed = speed;
    d.zza = speed;
    // `MoveControl` MOVE_TO: a target above the step height and near makes the cow jump.
    d.jump = yd > f64::from(body.step_height) && xd * xd + zd * zd < f64::from(body.width.max(1.0)) || stands_in_low_block(level, pos);
    d.jumping_op = d.jump;
}

/// The second clause of the `MoveControl` jump: the cow stands in a block with a collision shape that has not
/// reached its top (a cactus, a slab, a carpet): it jumps, unless the block is a door or a fence.
pub(crate) fn stands_in_low_block(level: &Level, pos: &Pos) -> bool {
    let mw = level.mw.0;
    let (bx, by, bz) = (pos.0.x.ifloor(), pos.0.y.ifloor(), pos.0.z.ifloor());
    let state = mw.world.block(bx, by, bz);
    let top = mw.data.state(state).top;
    if top <= 0.0 {
        return false;
    }
    let name = &mw.facts.get(state).name;
    pos.0.y < f64::from(top) + f64::from(by) && !(name.ends_with("_door") || name.ends_with("fence"))
}

pub(crate) fn move_control(cfg: Res<Config>, level: Res<Level>, mut q: Query<(&Pos, &mut Yaw, &mut MoveTo, &mut Drive, &Body, &Contact, &FluidState), (Without<Stilled>, Or<(With<Following>, With<crate::locomotion::climb::Climbing>)>, Or<(With<crate::locomotion::WalkMode>, With<crate::locomotion::LavaMode>, Without<crate::locomotion::Modes>)>)>) {
    let landed = |c: &Contact, f: &FluidState| c.on_ground || f.in_water || f.in_lava;
    if cfg.parallel {
        q.par_iter_mut().for_each(|(p, mut y, mut m, mut d, b, c, f)| move_control_one(&level, p, &mut y, &mut m, &mut d, b, landed(c, f)));
    } else {
        for (p, mut y, mut m, mut d, b, c, f) in &mut q {
            move_control_one(&level, p, &mut y, &mut m, &mut d, b, landed(c, f));
        }
    }
}

/// `LivingEntity.travel` of a cow on the ground (air branch): `Entity.moveRelative`, the real block sweep,
/// gravity and the drags. Block friction is the stone's 0.6.
/// `Entity.moveRelative`.
pub(crate) fn move_relative(vel: &mut Vel, yaw: f32, speed: f32, xxa: f32, yya: f32, zza: f32) {
    let input = Vec3::new(f64::from(xxa), f64::from(yya), f64::from(zza));
    let len_sq = input.x * input.x + input.y * input.y + input.z * input.z;
    if len_sq < 1.0e-7 {
        return;
    }
    let l = len_sq.sqrt();
    let i = if len_sq > 1.0 { Vec3::new(input.x / l, input.y / l, input.z / l) } else { input };
    let i = Vec3::new(i.x * f64::from(speed), i.y * f64::from(speed), i.z * f64::from(speed));
    let angle = f64::from(yaw * trig::DEG_TO_RAD);
    let (s, co) = (f64::from(trig::sin(angle)), f64::from(trig::cos(angle)));
    let u = vel.0;
    vel.0 = Vec3::new(u.x + (i.x * co - i.z * s), u.y + i.y, u.z + (i.z * co + i.x * s));
}

/// `Entity.move`: the sweep over the blocks (Horizon's real kernel), then the collision flags, the landing and the
/// restitution (the vertical velocity is lost after a landing).
pub(crate) fn do_move(level: &Level, pos: &mut Pos, vel: &mut Vel, c: &mut Contact, body: &Body, bounds: &mut Bounds, fall_damage: &mut f32) {
    let mut wanted = vel.0;
    if c.stuck != 0 {
        // `Entity.move`: a cobweb, a bush or powder snow it is in scales the movement, then the velocity is lost.
        let m = crate::inside_effects::stuck_factor(c.stuck);
        wanted = Vec3::new(wanted.x * m.x, wanted.y * m.y, wanted.z * m.z);
        vel.0 = Vec3::ZERO;
        c.stuck = 0;
    }
    let p = pos.0;
    let subject = body.lava_walker.then(|| horizon_client_render::collisions::PowderSnowSubject { lava_walker: true, ..Default::default() });
    let scene = SceneCollisions { world: level.mw.0.world, binding: level.mw.0.collision, subject };
    // `Entity.collide`: the world border's walls stop a mob too; the occupancy cache has no walls, so a level with a border takes the kernel.
    let border = level.mw.0.border.map(|b| (b, [p.x, p.z]));
    let moved = match border {
        Some(_) => scene.resolve_in_border_with(bounds.0, wanted, body.step_height, c.on_ground, border, &[]).unwrap_or(Vec3::ZERO),
        None => level.occ.resolve(scene, bounds.0, wanted, body.step_height, c.on_ground).unwrap_or(Vec3::ZERO),
    };
    // `Entity.move`: a move shorter than 1.0E-7 (squared) that a collision cut is not applied (a creeping walker on the ground).
    let sq = |v: Vec3| v.x * v.x + v.y * v.y + v.z * v.z;
    if sq(moved) > 1.0e-7 || sq(wanted) - sq(moved) < 1.0e-7 {
        pos.0 = Vec3::new(p.x + moved.x, p.y + moved.y, p.z + moved.z);
        if moved != Vec3::ZERO {
            bounds.0 = bounds.0.move_by(moved);
        }
    }
    let differs = |a: f64, b: f64| (b - a).abs() >= f64::from(1.0e-5_f32);
    c.horizontal = differs(wanted.x, moved.x) || differs(wanted.z, moved.z);
    let vertical = wanted.y != moved.y;
    let below = vertical && wanted.y < 0.0;
    c.on_ground = below;
    if !c.on_ground && moved.y < 0.0 {
        c.fall_distance -= moved.y;
    }
    if c.on_ground {
        if c.fall_distance > 0.0 {
            // `LivingEntity.calculateFallDamage`: the fall beyond the safe distance, times the multiplier, floored; a stalagmite tip replaces the ordinary landing.
            let own = crate::inside_effects::fall_damage_safe(c.fall_distance, body.safe_fall, body.fall_mult);
            *fall_damage = crate::inside_effects::fall_on(level, pos.0, c.fall_distance).unwrap_or(own);
        }
        c.fall_distance = 0.0;
    }
    // `Entity.move` ends by losing the velocity along the axes that collided (a landing, a ceiling, a wall).
    if vertical {
        vel.0.y = 0.0;
    }
    if differs(wanted.x, moved.x) {
        vel.0.x = 0.0;
    }
    if differs(wanted.z, moved.z) {
        vel.0.z = 0.0;
    }
    // `Entity.move`'s last step, after the collisions: the horizontal velocity scaled by the block's speed factor (`getBlockSpeedFactor`).
    let f = crate::cow::block_speed(level, pos, body, vel, false);
    vel.0 = Vec3::new(vel.0.x * f64::from(f), vel.0.y, vel.0.z * f64::from(f));
}

/// The commands collected by `OrdCmds` on one thread (with the emitting entity) and the empty queues kept for the next scopes (no allocation per scope).
#[derive(Default)]
pub struct Pending {
    pub(crate) done: Vec<(Entity, bevy_ecs::world::CommandQueue)>,
    pub(crate) spare: Vec<bevy_ecs::world::CommandQueue>,
}

/// `ParallelCommands` with a DETERMINISTIC order: the commands each entity's visit emits are kept with the entity, and `flush` (at the end of
/// the system) applies them sorted by entity, whatever thread ran the visit. The order commands are applied in decides the rows of the tables
/// (an entity that changes table takes the last row), hence the iteration order of every later system: with the per-thread queues of
/// `ParallelCommands` two runs, or two thread counts, gave different rows and so different games (suivi/ecs-echelle.md, G').
/// One `Pending` per thread (no lock: the first version took a mutex twice per visit, +50 instr per visit measured).
#[derive(bevy_ecs::system::SystemParam)]
pub struct OrdCmds<'w, 's> {
    pub(crate) pending: Local<'s, bevy_utils::Parallel<Pending>>,
    pub(crate) gathered: Local<'s, Vec<(Entity, bevy_ecs::world::CommandQueue)>>,
    pub(crate) par: ParallelCommands<'w, 's>,
    pub(crate) allocator: &'w bevy_ecs::entity::EntityAllocator,
    pub(crate) entities: &'w bevy_ecs::entity::Entities,
}

impl OrdCmds<'_, '_> {
    /// Commands emitted by the visit of `by` (the sort key); callable from the threads of a parallel iteration.
    pub fn command_scope(&self, by: Entity, f: impl FnOnce(Commands)) {
        self.pending.scope(|p| {
            let mut q = p.spare.pop().unwrap_or_default();
            f(Commands::new_from_entities(&mut q, self.allocator, self.entities));
            if q.is_empty() {
                p.spare.push(q);
            } else {
                p.done.push((by, q));
            }
        });
    }
    /// Hands the collected commands to the system's buffer, sorted by emitter (stable: one entity's commands keep their order).
    pub fn flush(&mut self) {
        let all = &mut *self.gathered;
        for p in self.pending.iter_mut() {
            all.append(&mut p.done);
        }
        if all.is_empty() {
            return;
        }
        // Sorted by emitter; the sort is stable and each thread's list is in its visit order, so one entity's commands keep their order.
        all.sort_by_key(|c| c.0);
        self.par.command_scope(|mut cmd| {
            for (_, q) in all.iter_mut() {
                cmd.append(q);
            }
        });
        // The emptied queues go back to the threads' pools (round robin), their allocations kept.
        let mut pools: Vec<&mut Vec<bevy_ecs::world::CommandQueue>> = self.pending.iter_mut().map(|p| &mut p.spare).collect();
        let n = pools.len().max(1);
        for (i, (_, q)) in all.drain(..).enumerate() {
            pools[i % n].push(q);
        }
    }
}

/// A velocity a cow that did not move is owed by a push (applied by `apply_pushes`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct PushIn(pub Vec3);

/// `LivingEntity.pushEntities` / `Entity.push` for the boxes that moved this step (the only ones that can start
/// overlapping something), in parallel: each moved cow reads the boxes of the neighbours it overlaps (the index gives
/// the candidates, the boxes are components, final for this step) and changes ONLY ITS OWN velocity (its half); the
/// neighbour that did not move gets its half as a command (a `PushIn`), which also wakes its physics.
#[allow(clippy::type_complexity)]
pub(crate) fn push(
    cfg: Res<Config>,
    level: Res<Level>,
    idx: Res<Index>,
    mut par: crate::cow::OrdCmds,
    // The neighbours include the creatures asleep: a `Mob` that is not ticked is still in the snapshot, it pushes the ones that
    // overlap it and is pushed by them (`Entity.push` moves both), only it does not move until it is ticked again.
    boxes: Query<(Ref<Bounds>, Has<Disabled>), Allow<Disabled>>,
    objects: Query<&Body>,
    ordered: Query<(), With<Overlapping>>,
    touching: Query<(), With<Touching>>,
    sleepers: Query<(), (With<crate::behaviours::bed::Sleeping>, Allow<Disabled>)>,
    // `Creaking.isPushable` / `push`: not while it stands still under a stare (`canMove` false); a dead one (crumbling, `Dying`) is not pushable either.
    // `AbstractHorse.isPushable` (`!isVehicle`): a mount with its rider; `Warden.isPushable` (`!isDiggingOrEmerging`) reads the act below.
    fixed: Query<(), Or<(With<Stilled>, crate::tear_down::Dead, (With<crate::rules::pushable::Unpushable>, With<Ridden>))>>,
    acting: Query<&crate::behaviours::act_phase::ActPhase>,
    riding: Query<&crate::mount::Riding>,
    // `AbstractCubeMob.push(Entity)`: a body of contact damage and its victim, asked only for overlapping boxes.
    (carriers, victims): (
        Query<(), (With<crate::behaviours::contact_damage::ContactDamage>, Allow<Disabled>)>,
        Query<(), (With<crate::behaviours::contact_damage::ContactVictim>, Allow<Disabled>)>,
    ),
    // `Player.aiStep` touches a body that moved onto a player standing still: a moved carrier asks the players near it.
    online: Option<Res<crate::goals::Online>>,
    mut movers: Query<(Entity, &Bounds, &mut Vel, Has<Moving>, Has<Touching>), (Or<(Changed<Bounds>, With<Touching>)>, Without<Overlapping>, Without<PushFree>)>,
) {
    // `Entity.isPushable`: an object (an item on the ground) neither pushes nor is pushed; read only where two boxes overlap.
    let is_object = |e: Entity| objects.get(e).is_ok_and(|b| b.object);
    // `Warden.isDiggingOrEmerging`: not pushable while it comes out of the ground or is in it.
    let digging = |e: Entity| acting.get(e).is_ok_and(|a| matches!(a.phase, crate::behaviours::act_phase::Phase::Emerging | crate::behaviours::act_phase::Phase::Burrowing));
    let one = |e: Entity, b: &Bounds, v: &mut Vel, moving: bool, was_touching: bool| {
        let (mn, mx) = (b.0.min_position(), b.0.max_position());
        let (cx, cz) = ((mn.x + mx.x) / 2.0, (mn.z + mx.z) / 2.0);
        let mut dv = (0.0_f64, 0.0_f64);
        let mut overlapping = 0;
        // An object (an item on the ground) neither pushes nor is pushed: it scans nothing.
        // (read lazily: a creature alone in its cells never asks).
        let mut e_object: Option<bool> = None;
        let mut e_fixed: Option<bool> = None;
        if let Some(on) = online.as_deref().filter(|_| carriers.contains(e)) {
            let r = (mx.x - mn.x).max(mx.z - mn.z) / 2.0 + 1.3;
            on.for_near(cx, cz, r, |p| {
                if b.0.intersects(crate::behaviours::contact_damage::touch_area(&p)) {
                    let id = p.id;
                    par.command_scope(e, |mut cmd| {
                        cmd.queue(move |w: &mut World| crate::behaviours::contact_damage::touch_player(w, e, id));
                    });
                }
            });
        }
        // `LivingEntity.pushEntities` takes the players too (`getPushableEntities`): a creature standing on a player's box is pushed
        // off it, twice per tick as the pair's two calls give. The player's side is the host's, not written here.
        if let Some(on) = online.as_deref().filter(|_| !is_object(e) && !fixed.contains(e) && !digging(e)) {
            let r = (mx.x - mn.x).max(mx.z - mn.z) / 2.0 + 0.3;
            on.for_near(cx, cz, r, |p| {
                let pb = crate::targeting::player_box(&p);
                if b.0.intersects(pb) {
                    let away = crate::rules::push_rules::push_impulse(Vec3::new(p.x, 0.0, p.z), Vec3::new(cx, 0.0, cz));
                    if let Some(v) = away {
                        dv.0 += 2.0 * v.x;
                        dv.1 += 2.0 * v.z;
                    }
                }
            });
        }
        {
        idx.for_near_touching(cx, cz, (mn.x, mn.z), (mx.x, mx.z), |o| {
            if o == e {
                return;
            }
            // `Entity.push` returns at once for `isPassengerOfSameVehicle`: the same root vehicle (a mount and its rider share it).
            let root = |x: Entity| riding.get(x).map_or(x, |r| r.0);
            if root(o) == root(e) {
                return;
            }
            if *e_object.get_or_insert_with(|| is_object(e)) {
                return;
            }
            let Ok((ob, asleep)) = boxes.get(o) else { return };
            if !b.0.intersects(ob.0) || is_object(o) {
                return;
            }
            overlapping += 1;
            for (cube, victim) in [(o, e), (e, o)] {
                if carriers.contains(cube) && victims.contains(victim) {
                    par.command_scope(e, |mut cmd| {
                        cmd.queue(move |w: &mut World| crate::behaviours::contact_damage::touch_victim(w, cube, victim));
                    });
                }
            }
            // `LivingEntity.push(Entity)` does nothing when the receiver sleeps in a bed: the call `pushEntities` makes on a sleeper
            // (`doPush` → `sleeper.push(this)`) moves neither side, the sleeper's own call moves both. One sleeper: one call.
            let in_bed = (sleepers.contains(e), sleepers.contains(o));
            if in_bed.0 && in_bed.1 {
                return;
            }
            let one_call = in_bed.0 != in_bed.1;
            let (omn, omx) = (ob.0.min_position(), ob.0.max_position());
            let (mut xa, mut za) = ((omn.x + omx.x) / 2.0 - cx, (omn.z + omx.z) / 2.0 - cz);
            let mut dd = xa.abs().max(za.abs());
            if dd < f64::from(0.01_f32) {
                return;
            }
            dd = dd.sqrt();
            xa /= dd;
            za /= dd;
            let pow = (1.0_f64 / dd).min(1.0);
            // `Entity.push` is called for the pair from both sides every tick (each entity runs `pushEntities`): each of the
            // two receives its impulse twice per tick.
            let (bx, bz) = (xa * pow * f64::from(0.05_f32), za * pow * f64::from(0.05_f32));
            // The neighbour `o` is ticked before this animal (a lower index) or after it.
            let lower = o.index().index() < e.index().index();
            let gain = |o_lower: bool| match () {
                () if one_call => 1.0,
                () if cfg.push_form == 0 => 2.0 * cfg.push_k,
                () => 1.0 + if o_lower { cfg.push_k } else { 1.0 },
            };
            let (gx, gz) = (bx * gain(lower), bz * gain(lower));
            // What the neighbour receives of this animal's call: its own call is the other one.
            xa = bx * gain(!lower);
            za = bz * gain(!lower);
            if !*e_fixed.get_or_insert_with(|| fixed.contains(e) || digging(e)) {
                dv.0 -= gx;
                dv.1 -= gz;
            }
            // An overlap begins: in the ordered mode this one and its neighbour are handled by `ordered_pass` from the
            // next tick (this tick's impulse was given above).
            if cfg.ordered_push {
                par.command_scope(e, |mut cmd| {
                    cmd.entity(e).insert(Overlapping);
                    cmd.entity(o).insert(Overlapping);
                });
            }
            if cfg.keep_touching {
                par.command_scope(e, |mut cmd| {
                    cmd.entity(o).insert(Touching);
                });
            }
            if ((!ob.is_changed() && !(cfg.keep_touching && touching.contains(o))) || ordered.contains(o)) && !fixed.contains(o) && !digging(o) {
                par.command_scope(e, |mut cmd| {
                    // A creature at rest keeps the push (its velocity changes) and moves when it is ticked again, not before.
                    let (push, wake) = (Vec3::new(xa, 0.0, za), !asleep);
                    cmd.queue(move |w: &mut World| add_push(w, o, push, wake));
                });
            }
        });
        }
        if cfg.keep_touching {
            if overlapping > 0 {
                par.command_scope(e, |mut cmd| {
                    cmd.entity(e).insert(Touching);
                });
            } else if was_touching {
                par.command_scope(e, |mut cmd| {
                    cmd.entity(e).remove::<Touching>();
                });
            }
        }
        // A pile: this one and the ones it overlaps are looked at by the cramming module (each tick until they are not).
        if cramming_limit(&level).is_some_and(|max| overlapping + 1 >= max) {
            par.command_scope(e, |mut cmd| {
                cmd.entity(e).insert(Crowded);
            });
            idx.for_near(cx, cz, |o| {
                if o != e && boxes.get(o).is_ok_and(|(ob, _)| !is_object(o) && b.0.intersects(ob.0)) {
                    par.command_scope(e, |mut cmd| {
                        cmd.entity(o).insert(Crowded);
                    });
                }
            });
        }
        if dv != (0.0, 0.0) {
            v.0 = Vec3::new(v.0.x + dv.0, v.0.y, v.0.z + dv.1);
            if !moving {
                par.command_scope(e, |mut cmd| {
                    cmd.entity(e).insert(Moving);
                });
            }
        }
    };
    if cfg.parallel {
        movers.par_iter_mut().for_each(|(e, b, mut v, m, t)| one(e, b, &mut v, m, t));
    } else {
        for (e, b, mut v, m, t) in &mut movers {
            one(e, b, &mut v, m, t);
        }
    }
    par.flush();
}

/// The game rule `maxEntityCramming` of the world: the others overlapping that crush an animal. `None` when it is 0 or less (never).
pub(crate) fn cramming_limit(level: &Level) -> Option<usize> {
    let max = level.mw.0.max_entity_cramming;
    (max > 0).then_some(max as usize)
}

/// Overlapping at least `cramming_limit - 1` others when last looked at: the cramming module visits it each tick
/// (`LivingEntity.pushEntities` counts the overlapping entities of every entity, every tick) until fewer overlap it.
#[derive(Component, Default)]
#[component(storage = "SparseSet")]
pub struct Crowded;

/// `LivingEntity.pushEntities`'s cramming: with `maxEntityCramming` or more others overlapping, one tick in four hurts 6.
/// The entities are taken one after the other and one that this hit kills no longer counts for the next (as the `Mob`
/// pass sees the deaths of the entities ticked before it): a pile of 25 loses one animal, not six, when the hurt windows end.
#[allow(clippy::type_complexity)]
pub(crate) fn cramming(
    clock: Res<Clock>,
    level: Res<Level>,
    idx: Res<Index>,
    boxes: Query<&Bounds, Allow<Disabled>>,
    riders: Query<(Has<Riding>, Has<Unpushable>, Has<Ridden>), Allow<Disabled>>,
    mut q: Query<(Entity, &Bounds, &mut Rng, &Health, &HurtAt), With<Crowded>>,
    mut queue: ResMut<DamageQueue>,
    mut commands: Commands,
) {
    let limit = cramming_limit(&level).unwrap_or(usize::MAX);
    let mut dying: Vec<Entity> = Vec::new();
    for (e, b, mut rng, health, hurt) in &mut q {
        let (mn, mx) = (b.0.min_position(), b.0.max_position());
        let mut n = 0;
        idx.for_near((mn.x + mx.x) / 2.0, (mn.z + mx.z) / 2.0, |o| {
            // `getPushableEntities` drops a mount that has a rider (`isPushable`), and the count drops a passenger (`isPassenger`).
            let out = riders.get(o).is_ok_and(|(riding, unpushable, ridden)| riding || (unpushable && ridden));
            if o != e && !out && !dying.contains(&o) && boxes.get(o).is_ok_and(|ob| b.0.intersects(ob.0)) {
                n += 1;
            }
        });
        if n < limit {
            commands.entity(e).remove::<Crowded>();
        } else if rng.next_int(4) == 0 {
            queue.add(e, 6.0, Cause::Cramming);
            // The hit is applied at the start of the next tick.
            let in_window = clock.of(e) + 1 < hurt.at + 10 && hurt.at != 0;
            let applied = if in_window { (6.0 - hurt.last).max(0.0) } else { 6.0 };
            if health.0 - applied <= 0.0 {
                dying.push(e);
            }
        }
    }
}

/// The half of a push a cow that did not move is owed: its velocity changes, its physics is active.
/// Adds an impulse to the neighbour `o`'s pending push: the impulses of all its neighbours are SUMMED (`Entity.push` adds to the velocity), not
/// overwritten by the last one, so the total does not depend on the order the pairs were visited in.
pub(crate) fn add_push(w: &mut World, o: Entity, push: Vec3, wake: bool) {
    let Ok(mut em) = w.get_entity_mut(o) else { return };
    match em.get_mut::<PushIn>() {
        Some(mut p) => p.0 = Vec3::new(p.0.x + push.x, p.0.y + push.y, p.0.z + push.z),
        None => {
            em.insert(PushIn(push));
        }
    }
    if wake {
        em.insert(Moving);
    }
}

#[allow(clippy::type_complexity)]
pub(crate) fn apply_pushes(
    mut q: Query<(Entity, &PushIn, &mut Vel)>,
    mut commands: Commands,
    mut on: ResMut<crate::goals::Online>,
    clock: Res<Clock>,
    mounts: Query<(Entity, &crate::mount::Mount, &Yaw, &Body, &Pos, Option<&crate::mount::PassengerSeats>, Option<&crate::cow::GrowAt>, Option<&crate::attr_mod::AttrMods>, Option<&crate::behaviours::posture::Posture>, Option<&crate::behaviours::random_stand::StandAnim>), With<crate::mount::Ridden>>,
    mut riders: Query<(&mut Pos, Option<&mut Yaw>), (With<crate::mount::Riding>, Without<crate::mount::Ridden>)>,
    rider_seats: Query<(&Body, Option<&crate::mount::RiderSeat>, Option<&crate::cow::GrowAt>, Option<&crate::attr_mod::AttrMods>)>,
) {
    for (e, p, mut v) in &mut q {
        v.0 = Vec3::new(v.0.x + p.0.x, v.0.y, v.0.z + p.0.z);
        commands.entity(e).remove::<PushIn>();
    }
    // The seats: no system of their own, so no cost when nothing is ridden (the queries are sparse sets).
    crate::mount::seats(&mounts, &mut riders, &rider_seats, clock.game_now(), &mut on, &mut commands);
}

/// Puts the entity at `at`: its box follows, and the events that can put it into blocks or a fluid are raised.
pub fn teleport(world: &mut World, e: Entity, at: Vec3) {
    // A thing without a body (an orb, a stack) is only put at the place: its own systems move it.
    let Some(b) = world.get::<Body>(e).copied() else {
        let Some(mut p) = world.get_mut::<Pos>(e) else { return };
        let by = at.subtract(p.0);
        p.0 = at;
        if let Some(mut bb) = world.get_mut::<Bounds>(e) {
            bb.0 = bb.0.move_by(by);
        }
        return;
    };
    world.get_mut::<Pos>(e).unwrap().0 = at;
    world.get_mut::<Vel>(e).unwrap().0 = Vec3::ZERO;
    world.get_mut::<Bounds>(e).unwrap().0 = box_at(at, b.width, b.height);
    world.entity_mut(e).insert((Recheck, Moving));
}
