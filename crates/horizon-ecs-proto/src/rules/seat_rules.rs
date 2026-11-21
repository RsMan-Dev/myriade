//! The seats' rules: how many riders a vehicle takes, where each one sits. Values only; the riders and their components are in `seat.rs`.
use crate::rules::boat as rules_boat;
use horizon_core::geometry::{Aabb, Vec3};

/// The equines that take a rider: their heights differ, so each has its own constants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Equine {
    Horse,
    Donkey,
    Mule,
}

/// The configuration of a vehicle's seats (a rule value, never a species name).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatKind {
    Boat { chest: bool, raft: bool },
    Cart,
    Horse(Equine),
}

/// `AbstractMinecart` `sized(0.98, 0.7)` and its passenger attachment `0.1875` (`EntityTypes`). The lowered attachment
/// `(0, 0, 0)` of a lowered minecart is not wired.
pub const CART_WIDTH: f64 = 0.98;
pub const CART_HEIGHT: f64 = 0.7;
pub const CART_SEAT_HEIGHT: f64 = 0.1875;

/// The width shared by horse, donkey and mule (`sized(1.3964844, h)`, `MAGIC_HORSE_WIDTH`).
pub const MOUNT_WIDTH: f64 = 1.3964844;
pub const HORSE_HEIGHT: f64 = 1.6;
pub const HORSE_SEAT_HEIGHT: f64 = 1.44375;
pub const DONKEY_HEIGHT: f64 = 1.5;
pub const DONKEY_SEAT_HEIGHT: f64 = 1.1125;
pub const MULE_HEIGHT: f64 = 1.6;
pub const MULE_SEAT_HEIGHT: f64 = 1.2125;

impl Equine {
    fn height(self) -> f64 {
        match self {
            Equine::Horse => HORSE_HEIGHT,
            Equine::Donkey => DONKEY_HEIGHT,
            Equine::Mule => MULE_HEIGHT,
        }
    }

    fn seat_height(self) -> f64 {
        match self {
            Equine::Horse => HORSE_SEAT_HEIGHT,
            Equine::Donkey => DONKEY_SEAT_HEIGHT,
            Equine::Mule => MULE_SEAT_HEIGHT,
        }
    }
}

/// The riders a vehicle takes: a boat's `max_passengers`, one for a cart or an equine.
pub fn capacity(kind: SeatKind) -> usize {
    match kind {
        SeatKind::Boat { chest, .. } => rules_boat::max_passengers(chest),
        SeatKind::Cart | SeatKind::Horse(_) => 1,
    }
}

/// The forward offset of the seat of the rider `rank` of `count`: a boat's `seat_z`, the centre for the others.
pub fn seat_offset(kind: SeatKind, rank: usize, count: usize) -> f32 {
    match kind {
        SeatKind::Boat { chest, .. } => rules_boat::seat_z(chest, rank, count, false),
        SeatKind::Cart | SeatKind::Horse(_) => 0.0,
    }
}

/// The height of a seat above the vehicle: a boat's `ride_height`, the passenger attachment of a cart or an equine.
pub fn seat_height(kind: SeatKind) -> f64 {
    match kind {
        SeatKind::Boat { raft, .. } => rules_boat::ride_height(raft),
        SeatKind::Cart => CART_SEAT_HEIGHT,
        SeatKind::Horse(e) => e.seat_height(),
    }
}

/// The point of a seat: `off` ahead of the vehicle along its yaw, `height` above it.
pub fn seat_point(pos: Vec3, yaw: f32, off: f32, height: f64) -> Vec3 {
    let (s, c) = f64::from(yaw).sin_cos();
    let off = f64::from(off);
    Vec3::new(pos.x - s * off, pos.y + height, pos.z + c * off)
}

/// The box of a vehicle at `pos`: a boat's, else a box `width` wide and `height` tall, centred on `pos`.
pub fn vehicle_bounds(kind: SeatKind, pos: Vec3) -> Aabb {
    let (width, height) = match kind {
        SeatKind::Boat { .. } => return rules_boat::bounds_at(pos),
        SeatKind::Cart => (CART_WIDTH, CART_HEIGHT),
        SeatKind::Horse(e) => (MOUNT_WIDTH, e.height()),
    };
    let w = width / 2.0;
    Aabb::new(Vec3::new(pos.x - w, pos.y, pos.z - w), Vec3::new(pos.x + w, pos.y + height, pos.z + w))
}
