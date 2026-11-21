//! Values of a homing projectile's steering (`homing_projectile`): the step's delta, its growth and the turn of the velocity,
//! and the xorshift draw of its own stream.
use horizon_core::geometry::Vec3;

/// The delta of a step toward `d` (the offset to the goal), at `speed` per tick; zero when the goal is reached.
pub fn unit_delta(d: [f64; 3], speed: f64) -> [f64; 3] {
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len == 0.0 { [0.0; 3] } else { d.map(|v| v / len * speed) }
}

/// One component of the delta after a tick: grown by `grow`, held to `[-max, max]`.
pub fn grow_delta(d: f64, grow: f64, max: f64) -> f64 {
    (d * grow).clamp(-max, max)
}

/// The velocity after a tick: each component turns toward the delta by `turn`.
pub fn turn_velocity(v: Vec3, delta: [f64; 3], turn: f64) -> Vec3 {
    Vec3::new(v.x + (delta[0] - v.x) * turn, v.y + (delta[1] - v.y) * turn, v.z + (delta[2] - v.z) * turn)
}

/// The xorshift step of the projectile's stream: the new state and a draw in `0..bound` (at least 1).
pub fn xorshift_draw(seed: u64, bound: i32) -> (u64, i32) {
    let mut s = seed;
    s ^= s >> 12;
    s ^= s << 25;
    s ^= s >> 27;
    let v = s.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33;
    (s, (v % bound.max(1) as u64) as i32)
}
