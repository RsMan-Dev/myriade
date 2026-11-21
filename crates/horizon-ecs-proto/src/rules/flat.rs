//! The numbers of the prototype herd: the gravity of a cow and the friction of its speed on one axis.

/// The gravity of a tick: the vertical speed falls by 0.08, then keeps 98 % of it.
pub fn gravity_step(vy: f64) -> f64 {
    (vy - 0.08) * 0.98
}

/// The friction of one horizontal axis: the speed keeps 0.546 on the ground and 0.91 in the air, and is nil under 0.003.
pub fn friction_axis(v: f64, on_ground: bool) -> f64 {
    let f = if on_ground { 0.91 * 0.6 } else { 0.91 };
    let v = v * f;
    if v.abs() < 0.003 { 0.0 } else { v }
}
