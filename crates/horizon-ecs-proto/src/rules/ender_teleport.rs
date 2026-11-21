//! The teleport spots of the creatures that teleport: the random offset, the spread toward a spot, and the box of a body.
use horizon_core::geometry::{Aabb, Vec3};

/// `Enderman.teleportRandom`: from its draws, 32 blocks around in x and z, and -32..32 in y.
pub fn random_offset(x: f64, y: i32, z: f64) -> (f64, i32, f64) {
    ((x - 0.5) * 64.0, y - 32, (z - 0.5) * 64.0)
}

/// `Enderman.teleportTowards`' spread, from its draws: 4 blocks in x and z, and -8..8 in y.
pub fn spread_offset(x: f64, y: i32, z: f64) -> (f64, i32, f64) {
    ((x - 0.5) * 8.0, y - 8, (z - 0.5) * 8.0)
}

/// The spot 16 blocks from `pos` away from `at` (along the unit line from `at` to the eye of `pos`), moved by `offset`.
pub fn towards_spot(pos: Vec3, at: Vec3, height: f64, eye_y: f64, offset: (f64, i32, f64)) -> Vec3 {
    let mut d = [pos.x - at.x, pos.y + height * 0.5 - eye_y, pos.z - at.z];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1.0e-9);
    d = d.map(|c| c / len);
    let (rx, ry, rz) = offset;
    Vec3::new(pos.x + rx - d[0] * 16.0, pos.y + f64::from(ry) - d[1] * 16.0, pos.z + rz - d[2] * 16.0)
}

/// The box of a body of `width` by `height` standing at `y`, centred on `(x, z)`.
pub fn spot_box(x: f64, y: f64, z: f64, width: f64, height: f64) -> Aabb {
    let half = width / 2.0;
    Aabb::new(Vec3::new(x - half, y, z - half), Vec3::new(x + half, y + height, z + half))
}
