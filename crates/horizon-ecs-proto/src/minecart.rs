//! Minecarts on the old track behaviour (`OldMinecartBehavior`): the physics of a cart's tick on sampled facts. The host reads the rail
//! under the cart (`Rail`: its two exits and whether it is a powered rail), the blocks around it and the entities; it applies the move
//! (collisions, border) and keeps the rest (fire, cactus, lava, TNT fuse and blast, loot, command, spawner, passengers). Position and
//! velocity are `Vec3`, as in `boat.rs`. Constants read in the jar: the speeds 0.4 and 0.2, the slide 0.0078125. The others are Horizon's
//! and are listed in the notes as doubts.
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

pub const MAX_LAND: f64 = 0.4;
pub const MAX_WATER: f64 = 0.2;
pub const SLIDE: f64 = 0.0078125;
pub const RIDDEN_FRICTION: f64 = 0.997;
pub const TRACK_FRICTION: f64 = 0.96;
pub const WATER_DRAG: f64 = 0.95;
const WIDTH: f64 = 0.98_f32 as f64;
const HEIGHT: f64 = 0.7_f32 as f64;

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

/// What the host read under the cart. `exits` are the two track exits of the rail shape (`[x, y, z]`, y is -1 for the low end of a
/// rising rail); `powered` is `Some(lit)` on a powered rail and `None` on any other rail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rail {
    pub exits: [[i32; 3]; 2],
    pub powered: Option<bool>,
}

impl Rail {
    /// A rising rail: one exit lies a block lower.
    pub fn slope(&self) -> bool {
        self.exits[0][1] != 0 || self.exits[1][1] != 0
    }
    /// The axis of a flat straight rail (`EastWest` is x, `NorthSouth` is z); `None` for curves and slopes.
    fn straight_axis(&self) -> Option<bool> {
        let [a, b] = self.exits;
        if a[1] != 0 || b[1] != 0 {
            return None;
        }
        if a[2] == 0 && b[2] == 0 && a[0] != 0 && b[0] != 0 {
            Some(true)
        } else if a[0] == 0 && b[0] == 0 && a[2] != 0 && b[2] != 0 {
            Some(false)
        } else {
            None
        }
    }
}

/// The box of a cart whose position is `p`.
pub fn bounds_at(p: Vec3) -> horizon_core::geometry::Aabb {
    let w = WIDTH / 2.0;
    horizon_core::geometry::Aabb::new(Vec3::new(p.x - w, p.y, p.z - w), Vec3::new(p.x + w, p.y + HEIGHT, p.z + w))
}

/// `getPos`: the point of the rail's line under `p`, for the rail at `cell`. A rising rail's high end lifts the point by one block.
pub fn rail_point(rail: &Rail, cell: [i32; 3], p: Vec3) -> Vec3 {
    let [e0, e1] = rail.exits;
    let at = |e: [i32; 3]| {
        let c = [f64::from(cell[0]), f64::from(cell[1]), f64::from(cell[2])];
        let (x, y, z) = (c[0] + 0.5 + f64::from(e[0]) * 0.5, c[1] + 0.0625 + f64::from(e[1]) * 0.5, c[2] + 0.5 + f64::from(e[2]) * 0.5);
        (x, y, z)
    };
    let (x0, y0, z0) = at(e0);
    let (x1, y1, z1) = at(e1);
    let (xd, yd, zd) = (x1 - x0, (y1 - y0) * 2.0, z1 - z0);
    let (fx, fz) = (f64::from(cell[0]), f64::from(cell[2]));
    let progress = if xd == 0.0 {
        p.z - fz
    } else if zd == 0.0 {
        p.x - fx
    } else {
        ((p.x - x0) * xd + (p.z - z0) * zd) * 2.0
    };
    let y = y0 + yd * progress + if yd < 0.0 { 1.0 } else if yd > 0.0 { 0.5 } else { 0.0 };
    Vec3::new(x0 + xd * progress, y, z0 + zd * progress)
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

/// `natural_slowdown` of a plain cart (and the tracks' 0.96): riding carts keep 0.997, the water drags 0.95 more.
pub fn slowdown(vel: Vec3, ridden: bool, in_water: bool) -> Vec3 {
    let f = if ridden { RIDDEN_FRICTION } else { TRACK_FRICTION };
    let drag = if in_water { WATER_DRAG } else { 1.0 };
    Vec3::new(vel.x * f * drag, 0.0, vel.z * f * drag)
}

/// The slowdown of a container cart (`AbstractMinecartContainer`): it replaces the base one whole, and its keep leans on how empty it is.
/// `signal` is the comparator level of its contents (0 to 15), `loot_pending` a loot table not yet rolled.
pub fn container_slowdown(vel: Vec3, signal: i32, loot_pending: bool, in_water: bool) -> Vec3 {
    let mut keep = 0.98_f32;
    if !loot_pending {
        keep += (15 - signal) as f32 * 0.001_f32;
    }
    if in_water {
        keep *= 0.95_f32;
    }
    let k = f64::from(keep);
    Vec3::new(vel.x * k, 0.0, vel.z * k)
}

/// The furnace's slowdown (`MinecartFurnace.applyNaturalSlowdown`): with a push the velocity follows it at 0.8 and the push is turned
/// along the velocity; without one the velocity keeps 0.98. Returns the new velocity and the push kept for next tick.
pub fn furnace_slowdown(vel: Vec3, push: Vec3, in_water: bool) -> (Vec3, Vec3) {
    if push.length_squared() > 1.0e-7 {
        let mut v = Vec3::new(vel.x * 0.8 + push.x, 0.0, vel.z * 0.8 + push.z);
        if in_water {
            v = Vec3::new(v.x * 0.1, 0.0, v.z * 0.1);
        }
        (v, push)
    } else {
        (Vec3::new(vel.x * 0.98, 0.0, vel.z * 0.98), push)
    }
}

/// The speed a furnace cart is held to: half the base speed, three quarters in water (`MinecartFurnace.getMaxSpeed`).
pub fn furnace_max_speed(base: f64, in_water: bool) -> f64 {
    base * if in_water { 0.75 } else { 0.5 }
}

/// `moveAlongTrack` after the move: the rising rail's exit lifts the cart, the friction of the slowdown, the height change of the rail's
/// line scales the speed (`(old_y - new_y) * 0.05`), and a cart that left its block keeps only the speed along the new one. `cell` is the
/// rail the tick began on, `old_rail_y` / `new_rail_y` the rail's line height before and after (`None` when the cart left the rails).
/// `conductors` are the two sides of a powered rail with no speed yet: the side that conducts pushes the cart away from it (0.02).
pub fn settle(
    rail: &Rail,
    cell: [i32; 3],
    pos: &mut Vec3,
    vel: &mut Vec3,
    slowdown_vel: Vec3,
    old_rail_y: Option<f64>,
    new_rail_y: Option<f64>,
    conductors: (bool, bool),
) {
    let [e0, e1] = rail.exits;
    let at = |e: [i32; 3], p: Vec3| p.x.floor() as i32 - cell[0] == e[0] && p.z.floor() as i32 - cell[2] == e[2];
    if e0[1] != 0 && at(e0, *pos) {
        pos.y += f64::from(e0[1]);
    } else if e1[1] != 0 && at(e1, *pos) {
        pos.y += f64::from(e1[1]);
    }
    *vel = slowdown_vel;
    if let (Some(old), Some(new)) = (old_rail_y, new_rail_y) {
        let speed = (old - new) * 0.05;
        let h = vel.horizontal().length();
        if h > 0.0 {
            vel.x *= (h + speed) / h;
            vel.z *= (h + speed) / h;
        }
        pos.y = new;
    }
    let (xn, zn) = (pos.x.floor() as i32, pos.z.floor() as i32);
    if xn != cell[0] || zn != cell[2] {
        let h = vel.horizontal().length();
        *vel = Vec3::new(h * f64::from(xn - cell[0]), vel.y, h * f64::from(zn - cell[2]));
    }
    if rail.powered == Some(true) {
        let h = vel.horizontal().length();
        if h > 0.01 {
            vel.x += vel.x / h * 0.06;
            vel.z += vel.z / h * 0.06;
        } else if let Some(x_axis) = rail.straight_axis() {
            let (near, far) = conductors;
            if x_axis {
                if near {
                    vel.x = 0.02;
                } else if far {
                    vel.x = -0.02;
                }
            } else if near {
                vel.z = 0.02;
            } else if far {
                vel.z = -0.02;
            }
        }
    }
}

/// True when a cart on rails at rest needs no tick: flat rail, no speed, no rider, no push, not powered. Such a cart sleeps until an event
/// wakes it (a push, a rider, the rail's power, a block under it).
pub fn can_park(rail: Option<&Rail>, vel: Vec3, ridden: bool, push: Vec3) -> bool {
    let Some(rail) = rail else { return false };
    vel.length_squared() == 0.0 && !rail.slope() && !ridden && push.length_squared() == 0.0 && rail.powered.is_none()
}
