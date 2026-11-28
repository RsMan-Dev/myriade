//! The hop: a ground mode that moves by bounds (`CubeMobMoveControl`, `SlimeMoveControl`): on the ground the body waits, with no input; after
//! a delay it turns to its direction and jumps; in the air it keeps its speed and its forward input until it lands, then waits again. The
//! delay (a base and a random extra, cut by `aggressive_div` while it hunts), the redirection (every so many ticks on the ground the body
//! draws a new direction), the speed modifier are parameters; the impulse of the jump is the body's (`travel`'s jump). Nothing is polled:
//! the wait is a deadline of the wheel (`Due::Hop`), the flight is visited each tick (`Hopping`), and a body at rest between two hops
//! costs nothing. Combinable with the other modes: it is the ground mode of the entity (`Medium::Ground`).
use crate::{
    behaviours::{Cap, Inst, Module},
    cow::*,
};
use crate::rules::hop::{bound_power, bound_velocity, hop_speed, path_wait, redirect_ticks, wander_wait};
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The wait on the ground between two hops: `delay_base + nextInt(delay_extra)` ticks.
    pub delay_base: i32,
    pub delay_extra: i32,
    /// The wait is divided by this while the body is aggressive (`hunting`): 1 for none.
    pub aggressive_div: i32,
    /// The direction is drawn again after `reduced(redirect_base + nextInt(redirect_extra))` ticks on the ground.
    pub redirect_base: i32,
    pub redirect_extra: i32,
    /// The speed it goes at, as a modifier of its movement speed.
    pub speed_modifier: f64,
    /// The wait is multiplied by this (`MagmaCube`: 4); `size.rs` sets it from the size rule.
    pub delay_mul: i32,
    /// A body that follows a path hops only while it has one (`RabbitMoveControl`): the wait after a landing, the hop's power and its
    /// push; `None`: it wanders (`CubeMobMoveControl`).
    pub path: Option<PathHop>,
}
/// The bounds of a body that follows a path.
#[derive(Clone, Copy, Debug)]
pub struct PathHop {
    /// The wait after a landing (`move_delay`), and the shorter one when the speed modifier of the path is `fast_from` or more.
    pub delay: i32,
    pub fast_delay: i32,
    pub fast_from: f64,
    /// The vertical speed of a bound: `low` up to the speed modifier `low_to`, `mid` above, `high` against a wall or a step.
    pub low: f64,
    pub low_to: f64,
    pub mid: f64,
    pub high: f64,
    /// The push forward and up of a bound that starts slow (`moveRelative(push, (0, 1.5, 1))`).
    pub push: f32,
}
impl Params {
    /// `Slime`: 10 to 29 ticks between hops, a new direction every 20 to 49 ticks on the ground, full speed.
    pub const SLIME: Params = Params { delay_base: 10, delay_extra: 20, aggressive_div: 3, redirect_base: 40, redirect_extra: 60, speed_modifier: 1.0, delay_mul: 1, path: None };
    /// `Rabbit`: a wait of 10 ticks (3 when it flees), 0.2 / 0.3 / 0.5 up, a push of 0.1.
    pub const RABBIT: Params = Params { delay_base: 10, delay_extra: 1, aggressive_div: 1, redirect_base: 40, redirect_extra: 60, speed_modifier: 1.0, delay_mul: 1, path: Some(PathHop { delay: 10, fast_delay: 3, fast_from: 2.2, low: 0.2, low_to: 0.6, mid: 0.3, high: 0.5, push: 0.1 }) };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Hop {
    pub p: Params,
    /// The direction it goes in (degrees), drawn again when `redirect` runs out.
    pub dir: f32,
    pub redirect: i32,
    /// The wait that follows the next landing.
    pending: i32,
    /// The tick (own time) of the last jump.
    jumped_at: u64,
    /// It hunts: the wait is divided.
    pub aggressive: bool,
    /// A deadline of the wheel is pending.
    armed: bool,
}
/// In the air (or about to jump): the hop system visits it each tick.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Hopping;

pub static MODULE: Module = Module { name: "hop", requires: &[Cap::Locomotion, Cap::Goals], equip, systems, ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *crate::behaviours::params::<Params>(p);
    // A body made in the air lands first, then jumps with no wait (the counter starts at 0); the first draw of the direction is its first tick.
    w.entity_mut(e).insert((Hop { p, dir: 0.0, redirect: 1, pending: p.path.map_or(0, |x| x.delay), jumped_at: 0, aggressive: false, armed: false }, Hopping));
}
fn systems(s: &mut bevy_ecs::schedule::Schedule, _: crate::goals::GoalSlot) {
    s.add_systems((hop_air, hop_hold).chain().after(crate::cow::move_control).before(crate::cow::travel));
}
/// Each tick of a hop in the air: the speed and the forward input stay (`CubeMobMoveControl`: `setSpeed` also sets the input); when the body is
/// back on the ground the wait begins.
fn hop_air(mut q: Query<(Entity, &mut Hop, &mut Drive, &Contact, &Body, &Nav, Option<&crate::goals::Phase>, Has<crate::behaviours::float::Floating>), With<Hopping>>, clock: Res<Clock>, mut wheel: ResMut<Wheel>, mut commands: Commands) {
    for (e, mut h, mut d, c, body, nav, phase, floating) in &mut q {
        let now = clock.of(e);
        // The tick of the jump keeps the jump the deadline asked for; a path body keeps the jump of its fluid float (`RabbitJumpControl`).
        d.jump = d.jump && if h.p.path.is_some() { floating } else { now == h.jumped_at };
        if c.on_ground && now > h.jumped_at {
            d.speed = 0.0;
            d.zza = 0.0;
            d.xxa = 0.0;
            commands.entity(e).remove::<Hopping>();
            if let Some(p) = h.p.path {
                // The wait of the next bound depends on the speed of the path it follows.
                h.pending = path_wait(nav.speed, p.fast_from, p.delay, p.fast_delay);
            }
            h.armed = true;
            // `CubeMobMoveControl.tick` counts the wait down only on the ticks where `KeepOnJumping.tick` has set `MOVE_TO`, that is the
            // ticks of the selector (one in two, the goal does not ask for every tick): `pending` goal ticks make twice as many ticks, the
            // first one being the landing tick or the next.
            let due = if h.p.path.is_some() {
                now + h.pending as u64
            } else {
                let first = if now <= 1 || (now + phase.map_or(0, |p| p.0)) % 2 == 0 { now } else { now + 1 };
                first + 2 * (h.pending.max(1) as u64 - 1)
            };
            wheel.push(clock.at(e, due).max(clock.0 + 1), e, Due::Hop);
        } else if h.p.path.is_some() {
            // In the air a body that follows a path keeps what the walk gave it.
            commands.entity(e).insert(Moving);
        } else {
            let speed = hop_speed(h.p.speed_modifier, body.speed);
            d.speed = speed;
            d.zza = speed;
            commands.entity(e).insert(Moving);
        }
    }
}
/// The wait is over: the direction (drawn again if its time ran out while it waited), the turn, the jump, and the wait of the next landing.
pub fn due(w: &mut World, e: Entity) {
    let now = now_of(w, e);
    let Some(mut h) = w.get::<Hop>(e).copied() else { return };
    h.armed = false;
    w.get_mut::<Hop>(e).unwrap().armed = false;
    if let Some(p) = h.p.path {
        // `Rabbit.customServerAiStep`: with a path it bounds toward the next point, at once; without one it waits.
        if w.get::<Following>(e).is_none() {
            return;
        }
        let (speed_mod, wall) = (w.get::<Nav>(e).map_or(1.0, |n| n.speed), w.get::<Contact>(e).is_some_and(|c| c.horizontal));
        let power = bound_power(wall, speed_mod, p.low, p.low_to, p.mid, p.high);
        let yaw = w.get::<Yaw>(e).map_or(0.0, |y| y.0);
        let v = bound_velocity(w.get::<Vel>(e).unwrap().0, power, p.push, yaw);
        w.get_mut::<Vel>(e).unwrap().0 = v;
        w.get_mut::<Hop>(e).unwrap().jumped_at = now;
        w.entity_mut(e).insert((Hopping, Moving));
        return;
    }
    // `CubeRandomDirection.tick` runs on each tick of the selector on the ground: the goal ticks of the wait count, the tick of the draw included.
    let mut left = h.pending.max(1);
    while left > 0 {
        if h.redirect <= left {
            left -= h.redirect;
            let draw = crate::goals::with_rng(w, e, |r| r.next_int(h.p.redirect_extra));
            h.redirect = redirect_ticks(h.p.redirect_base, draw);
            h.dir = crate::goals::with_rng(w, e, |r| r.next_int(360)) as f32;
        } else {
            h.redirect -= left;
            left = 0;
        }
    }
    let draw = crate::goals::with_rng(w, e, |r| r.next_int(h.p.delay_extra));
    h.pending = wander_wait(h.p.delay_base, draw, h.p.delay_mul, h.aggressive, h.p.aggressive_div);
    h.jumped_at = now;
    *w.get_mut::<Hop>(e).unwrap() = h;
    // The body has turned to its direction by the jump (`rotlerp` by 90 degrees a tick, over a wait of 10 ticks at least).
    if let Some(mut y) = w.get_mut::<Yaw>(e) {
        y.0 = h.dir;
    }
    let speed = hop_speed(h.p.speed_modifier, w.get::<Body>(e).map_or(0.0, |b| b.speed));
    if let Some(mut d) = w.get_mut::<Drive>(e) {
        d.jump = true;
        d.speed = speed;
        d.zza = speed;
    }
    w.entity_mut(e).insert((Hopping, Moving));
}

/// A body that follows a path waits on the ground between two bounds with no input (`RabbitMoveControl`: `speed 0`).
fn hop_hold(mut q: Query<(&Hop, &mut Drive, &Contact), (With<Following>, Without<Hopping>)>) {
    for (h, mut d, c) in &mut q {
        if h.p.path.is_some() && c.on_ground {
            d.speed = 0.0;
            d.zza = 0.0;
            d.xxa = 0.0;
            d.jump = false;
        }
    }
}
/// A path began (`Following` was put on the body): a body that waits with no deadline pending bounds at the next tick.
pub fn following_started(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let e = ctx.entity;
    let Some(h) = world.get::<Hop>(e) else { return };
    if h.p.path.is_none() || h.armed || world.get::<Hopping>(e).is_some() {
        return;
    }
    world.commands().queue(move |w: &mut World| {
        if let Some(mut h) = w.get_mut::<Hop>(e) {
            h.armed = true;
        }
        let at = w.resource::<Clock>().0 + 1;
        w.resource_mut::<Wheel>().push(at, e, Due::Hop);
    });
}
