//! Boats and rafts (`AbstractBoat`), the physics of their own tick: the status from the fluids and the ground under the hull, the float
//! and the paddle controls, the seats. The host samples the box (fluid over the hull, ground friction, clearance of the lift) and keeps the
//! rest (fire, bubble columns, lava, passengers, leash, chest) in its own modules. Position, velocity and yaw are `cow::{Pos, Vel, Yaw}`.
pub use crate::boat_state::{BoatMarker, CHEST_SLOTS, Chest, Hull, Hurt, PendingLoot, Wobble, Wood};
pub use crate::rules::boat::{HEIGHT, Status, Under, WIDTH, bounds_at, classify, max_passengers, ride_height, seat_z};
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

/// The configuration of a kind: a raft (bamboo) or a boat, with a chest or without.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kind {
    pub raft: bool,
    pub chest: bool,
}

/// The state the physics keeps for a boat.
#[derive(Component, Clone, Copy, Debug)]
pub struct Boat {
    pub status: Status,
    pub old_status: Status,
    /// False until a status has been set (`AbstractBoat.oldStatus` is null before the first tick).
    pub known: bool,
    /// The water surface under the hull (`waterLevel`), set with the status.
    pub water_level: f64,
    /// `landFriction`: the ground's, halved by each controlled tick on land.
    pub land_friction: f32,
    pub delta_rotation: f32,
    /// Left and right paddle row moving this tick.
    pub paddling: [bool; 2],
}

impl Default for Boat {
    fn default() -> Self {
        Self {
            status: Status::InAir,
            old_status: Status::InAir,
            known: false,
            water_level: 0.0,
            land_friction: 0.0,
            delta_rotation: 0.0,
            paddling: [false; 2],
        }
    }
}

/// Records the tick's status and keeps the old one. Returns true on the tick the boat reaches the water from the air: the host then
/// calls `enter_water` and the float is skipped for that tick.
pub fn set_status(b: &mut Boat, now: Status, friction: f32) -> bool {
    let entered = b.known && b.status == Status::InAir && !matches!(now, Status::InAir | Status::OnLand);
    b.known = true;
    b.old_status = b.status;
    b.status = now;
    if now == Status::OnLand {
        b.land_friction = friction;
    }
    entered
}

/// The tick of `enter_water`: the water's top less the hull's height plus 0.101, when the box there is clear (`clear`), else the boat
/// stays where it is. The boat is in the water from now on.
pub fn enter_water(b: &mut Boat, level_above: f32, clear: bool, y: &mut f64, vy: &mut f64) {
    b.water_level = *y + HEIGHT;
    if clear {
        *y = f64::from(level_above - HEIGHT as f32) + 0.101;
        *vy = 0.0;
    }
    b.status = Status::InWater;
}

/// `floatBoat`, then `controlBoat` when a rider steers: the drag and the buoyancy of the status, then the turn and the push along the yaw.
/// `y` is the hull's height now and `gravity` its `getGravity()` (0 with `NoGravity`).
pub fn float(b: &mut Boat, vel: &mut Vec3, yaw: &mut f32, y: f64, gravity: f64, keys: Option<[bool; 4]>) {
    let mut vspeed = -gravity;
    let mut buoyancy = 0.0;
    let inv: f32 = match b.status {
        Status::InWater => {
            buoyancy = (b.water_level - y) / HEIGHT;
            0.9
        }
        Status::UnderFlowingWater => {
            vspeed = -7.0e-4;
            0.9
        }
        Status::UnderWater => {
            buoyancy = f64::from(0.01_f32);
            0.45
        }
        Status::InAir => 0.9,
        Status::OnLand => {
            let friction = b.land_friction;
            if keys.is_some() {
                b.land_friction /= 2.0;
            }
            friction
        }
    };
    let f = f64::from(inv);
    vel.x *= f;
    vel.y += vspeed;
    vel.z *= f;
    b.delta_rotation *= inv;
    if buoyancy > 0.0 {
        vel.y = (vel.y + buoyancy * (0.04 / 0.65)) * 0.75;
    }
    if let Some(keys) = keys {
        control(b, keys, vel, yaw);
    }
}

/// `controlBoat`: the left and right keys turn the boat, up pushes it along its yaw and down pulls it back. Each row paddles on its turn.
fn control(b: &mut Boat, keys: [bool; 4], vel: &mut Vec3, yaw: &mut f32) {
    let [left, right, up, down] = keys;
    let mut acceleration = 0.0_f32;
    if left {
        b.delta_rotation -= 1.0;
    }
    if right {
        b.delta_rotation += 1.0;
    }
    if right != left && !up && !down {
        acceleration += 0.005;
    }
    *yaw += b.delta_rotation;
    if up {
        acceleration += 0.04;
    }
    if down {
        acceleration -= 0.005;
    }
    let r = yaw.to_radians();
    vel.x += f64::from((-r).sin() * acceleration);
    vel.z += f64::from(r.cos() * acceleration);
    b.paddling = [(right && !left) || up, (left && !right) || up];
}
