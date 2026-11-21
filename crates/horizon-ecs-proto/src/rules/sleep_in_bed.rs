use horizon_core::geometry::Vec3;

/// The feet are close to the bed's centre (the cell `p`): less than 2 blocks away.
pub fn bed_close(at: &Vec3, p: [i32; 3]) -> bool {
    let c = [f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5, f64::from(p[2]) + 0.5];
    (at.x - c[0]).powi(2) + (at.y - c[1]).powi(2) + (at.z - c[2]).powi(2) < 4.0
}
