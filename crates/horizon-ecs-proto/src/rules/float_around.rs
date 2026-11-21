//! The ghast's wanted spot: when it is wanted again, and the offset of a drawn point around the ghast.
use horizon_core::geometry::Vec3;

/// The ghast wants a spot unless the spot is 1 to 60 blocks away (squared distance in 1..=3600).
pub fn wants_spot(spot: [f64; 3], p: Vec3) -> bool {
    let d = (spot[0] - p.x).powi(2) + (spot[1] - p.y).powi(2) + (spot[2] - p.z).powi(2);
    !(1.0..=3600.0).contains(&d)
}

/// The offset of one axis of a drawn point: a draw in -1..1 scaled to 16 blocks.
pub fn spot_offset(draw: f32) -> f64 {
    f64::from(draw * 2.0 - 1.0) * 16.0
}
