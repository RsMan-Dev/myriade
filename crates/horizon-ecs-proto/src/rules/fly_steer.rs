//! The flying steer of a happy ghast: the velocity one tick gives from the first rider's input along its yaw.
use horizon_core::geometry::Vec3;

/// The velocity of one tick: forward and strafe input along `yaw_deg` at `speed`; a jump lifts at `climb`, otherwise it sinks at `sink`.
pub fn steer_velocity(forward: f64, strafe: f64, jump: f64, yaw_deg: f64, speed: f64, climb: f64, sink: f64) -> Vec3 {
    let rad = yaw_deg.to_radians();
    let (sin, cos) = (rad.sin(), rad.cos());
    let x = (-sin * forward + cos * strafe) * speed;
    let y = if jump > 0.0 { climb } else { -sink };
    let z = (cos * forward + sin * strafe) * speed;
    Vec3::new(x, y, z)
}
