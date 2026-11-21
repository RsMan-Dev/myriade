//! The view of a flier's head as the direction of its random walk: the heading and the pitch, in the unit of the horizontal plane.
use horizon_core::geometry::trig;

/// `Mob.view_vector`: the (x, z) direction of the view for a yaw and a pitch in degrees.
pub fn view_vector(yaw: f32, pitch: f32) -> (f64, f64) {
    let (yr, pr) = (-yaw * trig::DEG_TO_RAD, pitch * trig::DEG_TO_RAD);
    (f64::from(trig::sin(f64::from(yr)) * trig::cos(f64::from(pr))), f64::from(trig::cos(f64::from(yr)) * trig::cos(f64::from(pr))))
}
