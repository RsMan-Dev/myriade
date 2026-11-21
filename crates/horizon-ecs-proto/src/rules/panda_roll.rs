//! Calculations of the panda's roll (`PandaRollGoal`, `mobs/panda.rs:991-1031`): the cell in front of the nose, one below.

/// The block the roll test reads, `(x, y, z)`: the cell in front of the nose (the yaw in degrees, as the `Mob`'s), one below.
pub fn front_below(x: f64, y: f64, z: f64, yaw_deg: f64) -> (i32, i32, i32) {
    let a = yaw_deg.to_radians();
    let step = |d: f64| if d.abs() > 0.5 { d.signum() as i32 } else { 0 };
    let fx = x.floor() as i32 + step(-a.sin());
    let fz = z.floor() as i32 + step(a.cos());
    (fx, y.floor() as i32 - 1, fz)
}
