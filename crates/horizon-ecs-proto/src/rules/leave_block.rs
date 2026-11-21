//! Cell and timing of an enderman setting its block down (`EndermanLeaveBlockGoal`).
use horizon_core::geometry::{Aabb, Vec3};

/// The wait of a draw: twice the geometric draw.
pub fn leave_wait(geometric: i32) -> i32 {
    2 * geometric
}

/// The tick the armed draw is due: `wait` ticks (at least one) after `now`.
pub fn due_at(now: u64, wait: i32) -> u64 {
    now + wait.max(1) as u64
}

/// The cell set down at: the 2 x 2 x 2 box around `p`, from three draws `u` in `[0, 1)`.
pub fn cell_of(p: Vec3, u: [f64; 3]) -> [i32; 3] {
    let x = (p.x - 1.0 + u[0] * 2.0).floor() as i32;
    let y = (p.y + u[1] * 2.0).floor() as i32;
    let z = (p.z - 1.0 + u[2] * 2.0).floor() as i32;
    [x, y, z]
}

/// The box of the unit cell at `[x, y, z]`.
pub fn cell_box(c: [i32; 3]) -> Aabb {
    let [x, y, z] = c;
    Aabb::new(Vec3::new(f64::from(x), f64::from(y), f64::from(z)), Vec3::new(f64::from(x + 1), f64::from(y + 1), f64::from(z + 1)))
}
