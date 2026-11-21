//! The patrol's walk: the target of a patrol, the hop of a patrol in the plane, its speed, and the box of its companions.
use horizon_core::geometry::Vec3;

/// `PatrollingMonster.findPatrolTarget`: the target `dx, dz` blocks away from `at`.
pub fn patrol_target(at: [i32; 3], dx: i32, dz: i32) -> [i32; 3] {
    [at[0] + dx, at[1], at[2] + dz]
}
/// The hop in the plane: 10 blocks from `own` toward the target, the target curved a quarter turn round it (0.4 of the way).
pub fn hop_xz(own: Vec3, target: [i32; 3]) -> (f64, f64) {
    let to = Vec3::new(f64::from(target[0]) + 0.5, f64::from(target[1]), f64::from(target[2]) + 0.5);
    let d = Vec3::new(own.x - to.x, own.y - to.y, own.z - to.z);
    // `Vec3.yRot(90)`: the angle is in radians and the goal passes 90, as in the `Mob`.
    let (c, s) = (f64::from(90.0_f32.cos()), f64::from(90.0_f32.sin()));
    let r = Vec3::new(d.x * c + d.z * s, d.y, d.z * c - d.x * s);
    let to = Vec3::new(r.x * 0.4 + to.x, r.y * 0.4 + to.y, r.z * 0.4 + to.z);
    let (hx, hy, hz) = (to.x - own.x, to.y - own.y, to.z - own.z);
    let len = (hx * hx + hy * hy + hz * hz).sqrt().max(1.0e-9);
    (hx / len * 10.0 + own.x, hz / len * 10.0 + own.z)
}
/// The speed of a hop: the leader's, or the companion's.
pub fn hop_speed(leader: bool, speed: f64, leader_speed: f64) -> f64 {
    if leader { leader_speed } else { speed }
}
/// A patrol companion is in the box of 16.5 blocks across and 17 high (the offsets `dx, dy, dz` to the leader).
pub fn in_companion_box(dx: f64, dy: f64, dz: f64) -> bool {
    dx.abs() <= 16.5 && dy.abs() <= 17.0 && dz.abs() <= 16.5
}
