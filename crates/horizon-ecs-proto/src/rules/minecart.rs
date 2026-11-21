//! The physics of a minecart's tick on sampled facts: the rail's line and its exits, the slowdowns, the fall, the push, the fuse and the blast.
use horizon_core::geometry::Vec3;

pub const MAX_LAND: f64 = 0.4;
pub const MAX_WATER: f64 = 0.2;
pub const SLIDE: f64 = 0.0078125;
pub const RIDDEN_FRICTION: f64 = 0.997;
pub const TRACK_FRICTION: f64 = 0.96;
pub const WATER_DRAG: f64 = 0.95;
const WIDTH: f64 = 0.98_f32 as f64;
const HEIGHT: f64 = 0.7_f32 as f64;

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

/// `AbstractMinecart`'s gravity: `applyGravity` takes it off the vertical speed each tick (jar: 0.04).
pub const GRAVITY: f64 = 0.04;

/// `comeOffTrack`, before the move: the speed held to `max` (the cart's medium's), halved on the ground.
pub fn off_track_velocity(vel: Vec3, on_ground: bool, max: f64) -> Vec3 {
    let v = Vec3::new(vel.x.clamp(-max, max), vel.y, vel.z.clamp(-max, max));
    if on_ground { v.scale(0.5) } else { v }
}

/// `comeOffTrack`, after the move and the block's speed factor: the air drag 0.95 unless on the ground.
pub fn off_track_drag(vel: Vec3, on_ground: bool) -> Vec3 {
    if on_ground { vel } else { vel.scale(0.95) }
}

/// `checkFallDamage` of a cart's move of `moved_y` (negative when it fell): a fall adds to the distance unless the cart is in water; on
/// the ground the distance is spent, and a TNT cart that fell three blocks or more returns the squared speed of its blast, (distance/10)^2.
pub fn fall_step(fall: &mut f64, moved_y: f64, in_water: bool, on_ground: bool, tnt: bool) -> Option<f64> {
    if moved_y < 0.0 && !in_water {
        *fall -= f64::from(moved_y as f32);
    }
    if !on_ground {
        return None;
    }
    let blast = (tnt && *fall >= 3.0).then(|| (*fall / 10.0).powi(2));
    *fall = 0.0;
    blast
}

/// `AbstractMinecart.push` of a non-cart entity at `feet`: the horizontal push the entity gets from a cart at `cart`, 0.05 at most (the
/// cart keeps a quarter of it). `None` when the entity stands on the cart's centre.
pub fn push_from(feet: Vec3, cart: Vec3) -> Option<[f64; 2]> {
    let (xa, za) = (feet.x - cart.x, feet.z - cart.z);
    let dd = xa * xa + za * za;
    if dd < 1.0e-4 {
        return None;
    }
    let d = dd.sqrt();
    let k = f64::from(0.1_f32) * 0.5;
    let pow = (1.0 / d).min(1.0);
    Some([xa / d * pow * k, za / d * pow * k])
}

/// The box a hopper cart sucks items from when it takes nothing above: the cart's own box, widened 0.25 sideways.
pub fn hopper_box(lo: Vec3, hi: Vec3) -> ([f64; 3], [f64; 3]) {
    ([lo.x - 0.25, lo.y, lo.z - 0.25], [hi.x + 0.25, hi.y, hi.z + 0.25])
}

/// `activateMinecart` of an activator rail, by variant. A hopper cart is enabled by an unpowered rail.
pub fn hopper_enabled_on_activator(powered: bool) -> bool {
    !powered
}

/// A powered activator primes an unprimed TNT cart: the fuse (80, as Horizon; the gamerule is the host's check) or none.
pub fn tnt_primed_by(powered: bool, fuse: i32) -> Option<i32> {
    (powered && fuse < 0).then_some(FUSE)
}

/// A powered activator runs a command cart's command, at most every four ticks (`ACTIVATION_DELAY`, jar: 4).
pub fn command_due(powered: bool, now: i32, last: i32) -> bool {
    powered && now - last >= 4
}

/// `MinecartTNT.primeFuse`: the fuse a primed cart starts with (jar: 80, as Horizon).
pub const FUSE: i32 = 80;

/// `MinecartTNT.tick`'s fuse, returned as (new fuse, explodes now, smokes). It counts down while positive; at zero the cart explodes.
pub fn fuse_step(fuse: i32) -> (i32, bool, bool) {
    if fuse > 0 {
        (fuse - 1, false, true)
    } else {
        (fuse, fuse == 0, false)
    }
}

/// A horizontal collision at 0.01 of squared speed or more sets a TNT cart off (jar: 0.009999999776482582).
pub fn ignites_by_collision(horizontal_collision: bool, speed_sqr: f64) -> bool {
    horizontal_collision && speed_sqr >= 0.009_999_999_776_482_582
}

/// The power of a TNT cart's blast: 4.0 plus its speed factor (1.0) times a random draw `r` in [0, 1) times 1.5 times the speed (at most
/// 5 blocks a tick). Jar constants: 4.0, 1.5 and 5.0.
pub fn blast_power(speed_sqr: f64, r: f64) -> f32 {
    (f64::from(4.0_f32) + f64::from(1.0_f32) * r * 1.5 * speed_sqr.sqrt().min(5.0)) as f32
}

/// `container_signal`: the comparator level of a container's slots, from each filled slot's fill fraction (count over its stack's max)
/// and the slot count.
pub fn container_signal(fractions: &[f32], slots: usize) -> i32 {
    let total = fractions.iter().sum::<f32>() / slots.max(1) as f32;
    (total * 14.0).floor() as i32 + i32::from(total > 0.0)
}

