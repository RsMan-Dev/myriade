//! The rules of the dragon's fireball: its speed, the launch velocity, the box it searches for a target and the place of its burst.
use horizon_core::geometry::{Aabb, Vec3};

/// The launch speed, which is also the acceleration of each tick (`accelerationPower` 0.1, D:3109-3110).
pub const SPEED: f64 = 0.1;

/// `Vec3.normalize`: a vector shorter than 1e-5 is zero.
pub fn normalize(v: Vec3) -> Vec3 {
    let l = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    if l < 1.0e-5 {
        Vec3::new(0.0, 0.0, 0.0)
    } else {
        Vec3::new(v.x / l, v.y / l, v.z / l)
    }
}

/// The launch velocity toward `dir` (normalised twice, as Horizon does), at `SPEED`.
pub fn velocity(dir: Vec3) -> Vec3 {
    let n = normalize(normalize(dir));
    Vec3::new(n.x * SPEED, n.y * SPEED, n.z * SPEED)
}

/// The box a burst at `at` (the bottom of the fireball's box) searches for targets: its box inflated by (4, 2, 4).
pub fn search_box(at: Vec3) -> Aabb {
    let fireball_box = Aabb::new(Vec3::new(at.x - 0.5, at.y, at.z - 0.5), Vec3::new(at.x + 0.5, at.y + 1.0, at.z + 0.5));
    fireball_box.inflate(Vec3::new(4.0, 2.0, 4.0))
}

/// `fireball_cloud_origin` (D:102-114; jar `DragonFireball.onHit`): the first target whose box meets the fireball's box inflated
/// by (4, 2, 4) and whose position lies within 4 blocks of `feet` (the fireball's position) gives the cloud's place; else `feet`.
pub fn burst_origin(feet: [f64; 3], targets: &[(Aabb, Vec3)]) -> [f64; 3] {
    let area = Aabb::new(
        Vec3::new(feet[0] - 4.5, feet[1] - 2.0, feet[2] - 4.5),
        Vec3::new(feet[0] + 4.5, feet[1] + 3.0, feet[2] + 4.5),
    );
    for &(bounds, pos) in targets {
        let (dx, dy, dz) = (pos.x - feet[0], pos.y - feet[1], pos.z - feet[2]);
        if bounds.intersects(area) && dx * dx + dy * dy + dz * dz < 16.0 {
            return [pos.x, pos.y, pos.z];
        }
    }
    feet
}
