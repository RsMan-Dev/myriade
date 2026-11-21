//! The turtle's trip in water: the spot drawn around it, kept above the sea, and the aim of the first leg.
use horizon_core::geometry::Vec3;

/// The spot of a trip: the drawn offset from `p`, its height reset to 0 when it would rise above the sea.
pub fn travel_point(p: Vec3, offset: [i32; 3], sea: i32) -> [i32; 3] {
    let yt = if f64::from(offset[1]) + p.y > f64::from(sea - 1) { 0 } else { offset[1] };
    [(f64::from(offset[0]) + p.x).floor() as i32, (f64::from(yt) + p.y).floor() as i32, (f64::from(offset[2]) + p.z).floor() as i32]
}

/// The point the first leg leads toward: the spot's block centre on x and z, its floor on y.
pub fn travel_aim(point: [i32; 3]) -> [f64; 3] {
    [f64::from(point[0]) + 0.5, f64::from(point[1]), f64::from(point[2]) + 0.5]
}
