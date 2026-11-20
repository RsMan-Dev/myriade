//! The flight of a dragon (`EnderDragon.aiStep` and `Dragon` in `D:2646-2856`): ambient and flap sounds, the growl, the steering to the
//! phase's target, the velocity slide, and the 64-sample history the parts read. Constants are `Params`; the dragon's name is not here.
//! The phase machine runs between the two systems (`pre`, then the phases, then `steer`), as in `tick_dragon`.
use crate::behaviours::{params, Cap, Inst, Module};
use crate::cow::{Health, Pos, Vel, Yaw};
use crate::dragon::{crystals, data, death, fight::NearestCrystal, hurt, nodes, parts, phases};
use crate::goals::Events;
use crate::host_out::HostOut;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_core::random::{LegacyRandom, RandomSource};
use horizon_core::geometry::{trig, Aabb};
use std::f32::consts::TAU;

/// The dragon's two generators: `.0` is its own (`d.random`: the rolls, the growl delay, the deaths' rays), `.1` stands for the level's
/// (`world.random`) in the flap and growl pitches and the orbs, so that `.0` draws in Horizon's order. Seeded by the spawner.
#[derive(Component)]
pub struct DragonRng(pub LegacyRandom, pub LegacyRandom);

/// `Mob.isNoAi`: the dragon only flaps (to 0.5), nothing else moves. Rare, so a marker in sparse storage.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct NoAi;

/// `Mob.setNoAi`: a marker, so only the dragons that have it pay for it.
pub fn set_no_ai(w: &mut World, e: Entity, on: bool) {
    if on {
        w.entity_mut(e).insert(NoAi);
    } else {
        w.entity_mut(e).remove::<NoAi>();
    }
}

/// A sound and its volume.
#[derive(Clone, Copy, Debug)]
pub struct Snd {
    pub name: &'static str,
    pub volume: f32,
}

/// The flight's constants. `DEFAULT` is the End dragon's (`Dragon` at `D:123`, `D:2646-2856`).
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub ambient: Snd,
    pub flap: Snd,
    pub growl: Snd,
    pub ambient_interval: i32,
    pub growl_base: i32,
    pub growl_span: i32,
}
impl Params {
    pub const DEFAULT: Params = Params {
        ambient: Snd { name: "minecraft:entity.ender_dragon.ambient", volume: 5.0 },
        flap: Snd { name: "minecraft:entity.ender_dragon.flap", volume: 5.0 },
        growl: Snd { name: "minecraft:entity.ender_dragon.growl", volume: 2.5 },
        ambient_interval: 80,
        growl_base: 200,
        growl_span: 200,
    };
}

/// The phase's order for this tick, written by the phase machine: the fly target (none: no steering), the top speed, whether the
/// landing's turn applies, and whether the dragon sits (no growl, a steady flap).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct FlyOrder {
    pub target: Option<Vec3>,
    pub speed: f32,
    pub landing: bool,
    pub sitting: bool,
}

/// The order of the current phase (`Phase::target`, `fly_speed`, `sitting`, D:358-388), read where it is used: the phase's state is its
/// only copy. The ids are `EnderDragonPhase`'s.
pub fn order_of(w: &World, e: Entity) -> FlyOrder {
    let id = w.get::<phases::PhaseId>(e).map_or(10, |p| p.0);
    let target = w.get::<phases::PhaseData>(e).and_then(|d| d.0.target);
    let speed = match id {
        phases::LANDING => 1.5,
        phases::CHARGING | 9 => 3.0,
        10 => 1.0,
        _ => 0.6,
    };
    FlyOrder { target, speed, landing: id == phases::LANDING, sitting: matches!(id, 5..=7 | 10) }
}

/// What the flight keeps between ticks.
#[derive(Component)]
pub struct Flight {
    pub cfg: Params,
    /// Ticks lived (`d.ticks`).
    pub ticks: u32,
    pub flap: f32,
    pub old_flap: f32,
    pub growl: i32,
    pub ambient: i32,
    pub in_wall: bool,
    /// Set by `pre` when the crystal scan is due (one roll in ten); the healing reads it.
    pub scan_due: bool,
}

/// The yaw's acceleration (`yaw_accel`), kept between ticks.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct YawAccel(pub f32);

/// The last 64 `(y, yaw)` samples, the newest at `head - 1`; the parts read them delayed.
#[derive(Component)]
pub struct FlightHistory {
    pub samples: [(f64, f32); 64],
    pub head: u8,
}
impl FlightHistory {
    /// Filled with the spawn sample, as `Dragon` does.
    pub fn new(y: f64, yaw: f32) -> Self {
        Self { samples: [(y, yaw); 64], head: 0 }
    }
    pub fn record(&mut self, y: f64, yaw: f32) {
        self.samples[usize::from(self.head)] = (y, yaw);
        self.head = (self.head + 1) % 64;
    }
    /// The sample `delay` ticks ago (0 is the newest).
    pub fn sample(&self, delay: usize) -> (f64, f32) {
        self.samples[(usize::from(self.head) + 63 - delay) % 64]
    }
}

/// `Mth.wrapDegrees(float)`.
pub fn wrap(d: f32) -> f32 {
    let mut r = d % 360.0;
    if r >= 180.0 {
        r -= 360.0;
    }
    if r < -180.0 {
        r += 360.0;
    }
    r
}

/// `Vec3.normalize` on doubles: zero below the length 1e-5.
fn unit(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < f64::from(1.0e-5_f32) {
        [0.0; 3]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Before the phases: the ambient sound, the flap and its sound, the growl, the crystal-scan roll, the yaw wrap.
pub fn pre(f: &mut Flight, order: &FlyOrder, pos: Vec3, vel: Vec3, yaw: &mut f32, rng: &mut DragonRng, host: &mut HostOut) {
    let p = f.cfg;
    f.old_flap = f.flap;
    let at = [pos.x, pos.y, pos.z];
    let roll = rng.0.next_int_bound(1000).unwrap_or(999);
    let waited = f.ambient;
    f.ambient += 1;
    if roll < waited {
        f.ambient = -p.ambient_interval;
        let pitch = (rng.0.next_float() - rng.0.next_float()) * 0.2 + 1.0;
        host.sound(p.ambient.name, at, p.ambient.volume, pitch);
    }
    let flap_was = trig::cos(f64::from(f.old_flap * TAU));
    let mut flap_speed = 0.2 / (((vel.x * vel.x + vel.z * vel.z).sqrt() as f32) * 10.0 + 1.0);
    flap_speed *= 2f64.powf(vel.y) as f32;
    f.flap += if order.sitting {
        0.1
    } else if f.in_wall {
        flap_speed * 0.5
    } else {
        flap_speed
    };
    let flap_now = trig::cos(f64::from(f.flap * TAU));
    if flap_was <= -0.3 && flap_now >= -0.3 {
        let pitch = 0.8 + rng.1.next_float() * 0.3;
        host.sound(p.flap.name, at, p.flap.volume, pitch);
    }
    if !order.sitting {
        f.growl -= 1;
        if f.growl < 0 {
            f.growl = p.growl_base + rng.0.next_int_bound(p.growl_span).unwrap_or(0);
            let pitch = 0.8 + rng.1.next_float() * 0.3;
            host.sound(p.growl.name, at, p.growl.volume, pitch);
        }
    }
    f.scan_due = rng.0.next_int_bound(10).unwrap_or(1) == 0;
    *yaw = wrap(*yaw);
}

/// After the phases: the steering toward `order.target`, the move, and the slide. `in_wall` shortens the move (0.8).
pub fn steer(order: &FlyOrder, f: &Flight, pos: &mut Vec3, vel: &mut Vec3, yaw: &mut f32, yaw_accel: &mut f32) {
    let Some(t) = order.target else { return };
    let (xd, mut yd, zd) = (t.x - pos.x, t.y - pos.y, t.z - pos.z);
    let dist2 = xd * xd + yd * yd + zd * zd;
    let max = f64::from(order.speed);
    let horizontal = (xd * xd + zd * zd).sqrt();
    if horizontal > 0.0 {
        yd = (yd / horizontal).clamp(-max, max);
    }
    vel.y += yd * 0.01;
    *yaw = wrap(*yaw);
    let aim = unit([xd, t.y - pos.y, zd]);
    let r = f64::from(*yaw * trig::DEG_TO_RAD);
    let dir = unit([f64::from(trig::sin(r)), vel.y, f64::from(-trig::cos(r))]);
    let facing = ((dot(dir, aim) as f32 + 0.5) / 1.5).max(0.0);
    if xd.abs() > f64::from(1.0e-5_f32) || zd.abs() > f64::from(1.0e-5_f32) {
        let rot = (vel.x * vel.x + vel.z * vel.z).sqrt() as f32 + 1.0;
        let turn = if order.landing { rot.min(40.0) / rot } else { 0.7 / rot.min(40.0) / rot };
        let delta = wrap(180.0 - trig::atan2(xd, zd) as f32 * trig::RAD_TO_DEG - *yaw).clamp(-50.0, 50.0);
        *yaw_accel *= 0.8;
        *yaw_accel += delta * turn;
        *yaw += *yaw_accel * 0.1;
    }
    let span = (2.0 / (dist2 + 1.0)) as f32;
    let speed = 0.06 * (facing * span + (1.0 - span));
    let r = f64::from(*yaw * trig::DEG_TO_RAD);
    vel.x += f64::from(speed) * f64::from(trig::sin(r));
    vel.z += -f64::from(speed) * f64::from(trig::cos(r));
    let step = if f.in_wall { 0.800000011920929 } else { 1.0 };
    // The move goes through the level's move gate in Horizon; here the position takes the step as it is.
    pos.x += vel.x * step;
    pos.y += vel.y * step;
    pos.z += vel.z * step;
    let actual = unit([vel.x, vel.y, vel.z]);
    let slide = 0.8 + 0.15 * (dot(actual, dir) + 1.0) / 2.0;
    vel.x *= slide;
    vel.y *= 0.9100000262260437;
    vel.z *= slide;
}

/// One tick of a dragon (`tick_dragon`, D:2677): the single wake of its flight. Its random stream is observable every tick (ambient
/// roll, scan roll), so no tick can be skipped: one wake per tick per dragon is the floor, and everything else rides on it.
pub fn tick(w: &mut World, e: Entity) {
    let Some(mut f) = w.get_entity_mut(e).ok().and_then(|mut x| x.take::<Flight>()) else { return };
    f.ticks += 1;
    w.entity_mut(e).insert(f);
    if let Some(mut c) = w.get_mut::<hurt::DragonHurt>(e) {
        c.time = (c.time - 1).max(0);
        c.window = (c.window - 1).max(0);
    }
    if w.get::<Health>(e).is_some_and(|h| h.0 <= 0.0) {
        death::tick(w, e);
        return;
    }
    heal(w, e);
    let mut f = w.entity_mut(e).take::<Flight>().expect("flight");
    let mut rng = w.entity_mut(e).take::<DragonRng>().expect("rng");
    let order = order_of(w, e);
    let (pos, vel, mut yaw) = (w.get::<Pos>(e).expect("pos").0, w.get::<Vel>(e).expect("vel").0, w.get::<Yaw>(e).expect("yaw").0);
    w.resource_scope::<Events, _>(|_, mut ev| pre(&mut f, &order, pos, vel, &mut yaw, &mut rng, &mut ev.host));
    let scan = f.scan_due;
    let no_ai = w.get::<NoAi>(e).is_some();
    if no_ai {
        f.flap = 0.5;
    }
    w.entity_mut(e).insert((f, rng, Yaw(yaw)));
    if scan {
        let reach = Aabb::new(Vec3::new(pos.x - 8.0, pos.y, pos.z - 8.0), Vec3::new(pos.x + 8.0, pos.y + 8.0, pos.z + 8.0)).inflate(Vec3::new(32.0, 32.0, 32.0));
        let near = crystals::nearest_to(w, pos, reach);
        w.entity_mut(e).insert(NearestCrystal(near));
    }
    if no_ai {
        return;
    }
    let mut hist = w.entity_mut(e).take::<FlightHistory>().expect("history");
    hist.record(pos.y, yaw);
    w.entity_mut(e).insert(hist);
    phases::tick(w, e);
    steer_sys(w, e);
    parts::place(w, e);
    hurt::contacts(w, e);
    super::walls::check(w, e);
}

/// `checkCrystals`: one health every tenth tick while a crystal is the nearest one, up to the maximum.
fn heal(w: &mut World, e: Entity) {
    let ticks = w.get::<Flight>(e).map_or(0, |f| f.ticks);
    if w.get::<NearestCrystal>(e).is_some_and(|n| n.0.is_some()) && ticks % 10 == 0 {
        if let Some(mut h) = w.get_mut::<Health>(e) {
            h.0 = (h.0 + 1.0).min(data::MAX_HEALTH);
        }
    }
}

fn steer_sys(w: &mut World, e: Entity) {
    let order = order_of(w, e);
    let f = w.entity_mut(e).take::<Flight>().expect("flight");
    let mut pos = w.get::<Pos>(e).expect("pos").0;
    let mut vel = w.get::<Vel>(e).expect("vel").0;
    let mut yaw = w.get::<Yaw>(e).expect("yaw").0;
    let mut accel = w.get::<YawAccel>(e).map_or(0.0, |a| a.0);
    steer(&order, &f, &mut pos, &mut vel, &mut yaw, &mut accel);
    w.entity_mut(e).insert((f, Pos(pos), Vel(vel), Yaw(yaw), YawAccel(accel)));
}

/// The wake of the flight: it re-arms itself for the next tick, the only per-tick wake of a dragon.
pub fn wake(w: &mut World, e: Entity) {
    tick(w, e);
    if w.get_entity(e).is_ok() {
        crate::cow::after(w, e, 1, wake);
    }
}

/// The flight starts on its first tick after the spawn (no system: a wake, so the schedule stays empty).
fn systems(_: &mut bevy_ecs::schedule::Schedule, _: crate::goals::GoalSlot) {}

/// The flight's components, from the spawn position; the spawner seeds `DragonRng` (see `respawn::spawn_new_dragon`).
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let cfg = *params::<Params>(p);
    // The dragon is no `Mob` type: its health is its own (`Dragon::new`), not the table's.
    w.entity_mut(e).insert((Health(data::MAX_HEALTH), crate::cow::MaxHealth(data::MAX_HEALTH)));
    w.init_resource::<crate::dragon::fight::EndFight>();
    w.init_resource::<crate::dragon::fight::FightEvents>();
    w.init_resource::<crate::dragon::hurt::PartHits>();
    w.entity_mut(e).insert((phases::PhaseId(crate::species::ender_dragon::START_PHASE), phases::PhaseData::default()));
    if w.get_resource::<nodes::FlightNodes>().is_none() {
        let mw = crate::blocks::world_of(w);
        // `MOTION_BLOCKING_NO_LEAVES`: the highest solid or fluid block that is no leaves, plus one.
        let ground = |x: i32, z: i32| {
            (0..256).rev().find(|&y| {
                let f = mw.facts.get(mw.world.block(x, y, z));
                (f.full_collision || f.legacy_solid || f.fluid != horizon_client_lib::blocks::Fluid::None) && !f.name.ends_with("_leaves")
            }).map_or(0, |y| y + 1)
        };
        w.insert_resource(nodes::FlightNodes::build(&nodes::LAYOUT, ground));
    }
    let (y, yaw) = (w.get::<Pos>(e).map_or(0.0, |p| p.0.y), w.get::<Yaw>(e).map_or(0.0, |y| y.0));
    w.entity_mut(e).insert((
        Flight { cfg, ticks: 0, flap: 0.0, old_flap: 0.0, growl: 100, ambient: 0, in_wall: false, scan_due: false },
        DragonRng(LegacyRandom::new(0), LegacyRandom::new(0)),
        FlightHistory::new(y, yaw),
        FlyOrder::default(),
        YawAccel(0.0),
    ));
    crate::cow::after(w, e, 1, wake);
}

pub static MODULE: Module = Module { name: "dragon_flight", requires: &[Cap::Body], equip, systems, ..Module::DEFAULT };
