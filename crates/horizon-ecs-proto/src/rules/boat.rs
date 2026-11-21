//! The boat's rules: the status of the hull, the box and the seats of a boat, the riding height. Values only: the components stay in `boat.rs`.
use horizon_core::geometry::{Aabb, Vec3};

/// `AbstractBoat.Status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    InWater,
    UnderWater,
    UnderFlowingWater,
    OnLand,
    InAir,
}

/// What the hull finds above it: a still or a flowing fluid, or nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Under {
    Dry,
    Still,
    Flowing,
}

pub const WIDTH: f64 = 1.375;
pub const HEIGHT: f64 = 0.5625;

/// The box of a boat whose position is `p`.
pub fn bounds_at(p: Vec3) -> Aabb {
    let w = WIDTH / 2.0;
    Aabb::new(Vec3::new(p.x - w, p.y, p.z - w), Vec3::new(p.x + w, p.y + HEIGHT, p.z + w))
}

/// `isUnderwater`, `checkInWater` and `getGroundFriction` in their order: a flowing fluid over the hull wins, then a still one, then the
/// water the hull sits in, then the ground (friction above 0), else the air. `friction` is only read when the hull is on nothing wet.
pub fn classify(under: Under, in_water: bool, friction: f32) -> Status {
    match under {
        Under::Flowing => Status::UnderFlowingWater,
        Under::Still => Status::UnderWater,
        Under::Dry if in_water => Status::InWater,
        Under::Dry if friction > 0.0 => Status::OnLand,
        Under::Dry => Status::InAir,
    }
}

/// `getMaxPassengers`: two, one beside a chest.
pub fn max_passengers(chest: bool) -> usize {
    if chest { 1 } else { 2 }
}

/// `rideHeight`: a third of a boat's height, 8/9 of a raft's.
pub fn ride_height(raft: bool) -> f64 {
    let h = HEIGHT as f32;
    f64::from(if raft { h * 0.888_888_9 } else { h / 3.0 })
}

/// The z offset of the seat of passenger `index` of `count` (`getPassengerAttachmentPoint`): the back seat of a chest boat, the front and
/// back seats of two riders, an animal's sitting 0.2 further forward.
pub fn seat_z(chest: bool, index: usize, count: usize, animal: bool) -> f32 {
    let mut z = if chest { 0.15 } else { 0.0 };
    if count > 1 {
        z = if index == 0 { 0.2 } else { -0.6 };
        if animal {
            z += 0.2;
        }
    }
    z
}
