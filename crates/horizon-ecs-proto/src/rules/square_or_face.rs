//! Calculs de la visée du fantôme heureux (`happy_ghast.rs` `look_control`) : le cap au quart de tour le plus proche, ou le cap du mouvement.
use horizon_core::geometry::trig;

/// The yaw brought to the nearest quarter turn (degrees), the head squared to the world.
pub fn square_yaw(yaw: f32) -> f32 {
    let mut close = yaw % 90.0;
    if close >= 45.0 {
        close -= 90.0;
    }
    if close < -45.0 {
        close += 90.0;
    }
    yaw - close
}

/// The yaw (degrees) of the motion heading `x`, `z`, from `atan2` in radians.
pub fn heading_yaw(x: f64, z: f64) -> f32 {
    let to_deg = 180.0_f32 / std::f32::consts::PI;
    -(trig::atan2(x, z) as f32) * to_deg
}
