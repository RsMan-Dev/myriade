//! Minecarts on the old track behaviour (`OldMinecartBehavior`): the physics of a cart's tick on sampled facts. The host reads the rail
//! under the cart (`Rail`: its two exits and whether it is a powered rail), the blocks around it and the entities; it applies the move
//! (collisions, border) and keeps the rest (fire, cactus, lava, TNT fuse and blast, loot, command, spawner, passengers). Position and
//! velocity are `Vec3`, as in `boat.rs`. Constants read in the jar: the speeds 0.4 and 0.2, the slide 0.0078125. The others are Horizon's
//! and are listed in the notes as doubts.
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
pub use crate::rules::minecart::{FUSE, GRAVITY, MAX_LAND, MAX_WATER, RIDDEN_FRICTION, Rail, SLIDE, TRACK_FRICTION, WATER_DRAG, blast_power, bounds_at, can_park, command_due, container_signal, container_slowdown, fall_step, furnace_max_speed, furnace_slowdown, fuse_step, hopper_box, hopper_enabled_on_activator, ignites_by_collision, off_track_drag, off_track_velocity, push_from, rail_point, settle, slowdown, tnt_primed_by};

/// The variant of a cart: configuration only, the state of a variant lives in its own component.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Rideable,
    Chest,
    Hopper,
    Furnace,
    Tnt,
    CommandBlock,
    Spawner,
}

/// The track state a cart keeps between ticks.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Track {
    /// `Entity.isInWater` as the last tick left it: the move goes by it.
    pub in_water: bool,
    pub on_ground: bool,
}

/// Furnace minecarts: fuel ticks and the push (`MinecartFurnace`). Only furnaces carry it.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Furnace {
    pub fuel: i32,
    pub push: Vec3,
}

/// `moveAlongTrack`, before the move: the slide of a rising rail, the velocity turned onto the rail's line (flipped to the way it goes),
/// the rider's push, the halt of an unpowered powered rail, then the cart set on the line. `intent` is the rider's (x, z) keys. Returns the
/// move's delta: the velocity scaled by 0.75 with a rider, clamped to the speed of the cart's medium.
pub fn along_track(t: Track, pos: &mut Vec3, vel: &mut Vec3, cell: [i32; 3], rail: &Rail, intent: [f64; 2], ridden: bool) -> Vec3 {
    let [e0, e1] = rail.exits;
    let slide = if t.in_water { SLIDE * 0.2 } else { SLIDE };
    // A rising rail slides the cart back down: `uphill` runs from the low exit to the high one.
    if rail.slope() {
        let (low, high) = if e0[1] < 0 { (e0, e1) } else { (e1, e0) };
        vel.x -= slide * f64::from(high[0] - low[0]);
        vel.z -= slide * f64::from(high[2] - low[2]);
    }
    let mut y = f64::from(cell[1]);
    if rail.slope() {
        y += 1.0;
    }
    let (mut xd, mut zd) = (f64::from(e1[0] - e0[0]), f64::from(e1[2] - e0[2]));
    let length = xd.hypot(zd);
    if vel.x * xd + vel.z * zd < 0.0 {
        xd = -xd;
        zd = -zd;
    }
    let speed = vel.horizontal().length().min(2.0);
    *vel = Vec3::new(speed * xd / length, vel.y, speed * zd / length);
    let mut halt = rail.powered == Some(false);
    if ridden && (intent[0] != 0.0 || intent[1] != 0.0) && vel.x * vel.x + vel.z * vel.z < 0.01 {
        vel.x += intent[0] * 0.001;
        vel.z += intent[1] * 0.001;
        halt = false;
    }
    if halt {
        if vel.horizontal().length() < 0.03 {
            *vel = Vec3::ZERO;
        } else {
            *vel = Vec3::new(vel.x * 0.5, 0.0, vel.z * 0.5);
        }
    }
    let (fx, fz) = (f64::from(cell[0]), f64::from(cell[2]));
    let (x0, z0) = (fx + 0.5 + f64::from(e0[0]) * 0.5, fz + 0.5 + f64::from(e0[2]) * 0.5);
    let (x1, z1) = (fx + 0.5 + f64::from(e1[0]) * 0.5, fz + 0.5 + f64::from(e1[2]) * 0.5);
    let (xd, zd) = (x1 - x0, z1 - z0);
    let progress = if xd == 0.0 {
        pos.z - fz
    } else if zd == 0.0 {
        pos.x - fx
    } else {
        ((pos.x - x0) * xd + (pos.z - z0) * zd) * 2.0
    };
    *pos = Vec3::new(x0 + xd * progress, y, z0 + zd * progress);
    let scale = if ridden { 0.75 } else { 1.0 };
    let max = if t.in_water { MAX_WATER } else { MAX_LAND };
    Vec3::new((scale * vel.x).clamp(-max, max), 0.0, (scale * vel.z).clamp(-max, max))
}

/// The fall distance of a cart in the air. Absent when it is not falling.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Fall(pub f64);

/// The fuse of a TNT cart: -1 unprimed. Absent on the other genres.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fuse(pub i32);