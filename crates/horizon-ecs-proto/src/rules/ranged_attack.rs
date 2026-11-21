//! Calculs du tir à distance (`RangedAttackGoal`, golem de neige, lama, sorcière, noyé) : cadence, portée d'arrêt, visée, départ, dispersion.
use horizon_core::geometry::{trig, Vec3};

/// The ticks to the next shot after one at squared distance `d2`: `min` at the radius, `max` at the point of the target.
pub fn gap_after_shot(d2: f64, radius: f32, min: i32, max: i32) -> i32 {
    let dist = (d2.sqrt() as f32) / radius;
    (dist * (max - min) as f32 + min as f32).floor() as i32
}

/// The ticks before the first shot of a cycle, from the squared distance `d2` (the `Mob` attack counter, in `f64`).
pub fn gap_before_first_shot(d2: f64, radius: f32, min: i32, max: i32) -> i32 {
    let f = d2.sqrt() / f64::from(radius);
    (f64::from(min) + f * f64::from(max - min)).floor() as i32
}

/// The goal stops walking once the target is within `radius` (squared distance `d2`) and seen for 5 ticks.
pub fn in_stop_range(d2: f64, radius: f32, see_time: i32) -> bool {
    d2 <= f64::from(radius * radius) && see_time >= 5
}

/// The horizontal distance of a shot, from its `xd` and `zd` offsets.
pub fn ground_dist(xd: f64, zd: f64) -> f64 {
    (xd * xd + zd * zd).sqrt()
}

/// The height aimed at: the box's bottom plus `aim_fraction` of its height, or the eyes minus `aim_down` when it is 0.
pub fn aim_height(aim_fraction: f64, bottom: f64, top: f64, eye: f64, aim_down: f64) -> f64 {
    if aim_fraction > 0.0 { bottom + (top - bottom) * aim_fraction } else { eye - aim_down }
}

/// The start of a shot: `front` blocks ahead of the body along its yaw (degrees) when `front > 0`, else over the body's column.
pub fn muzzle(pos: Vec3, eye: f64, start_down: f64, front: f64, width: f64, yaw: f64) -> Vec3 {
    if front > 0.0 {
        let (yaw, reach) = (yaw.to_radians(), (width + 1.0) * front);
        Vec3::new(pos.x - reach * f64::from(trig::sin(yaw)), pos.y + eye - start_down, pos.z + reach * f64::from(trig::cos(yaw)))
    } else {
        Vec3::new(pos.x, pos.y + eye - start_down, pos.z)
    }
}

/// The potion a witch throws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WitchBrew {
    Slowness,
    Poison,
    Weakness,
    Harm,
}

/// `Witch.performRangedAttack`: slowness from afar, poison on the healthy, weakness up close (a quarter of the time), else harming.
/// `weak_roll` is drawn only when the weakness is still possible (close, not yet weakened).
pub fn witch_brew(dist: f64, health: f64, slow: bool, poison: bool, weak: bool, weak_roll: impl FnOnce() -> f32) -> WitchBrew {
    if dist >= 8.0 && !slow {
        WitchBrew::Slowness
    } else if health >= 8.0 && !poison {
        WitchBrew::Poison
    } else if dist <= 3.0 && !weak && weak_roll() < 0.25 {
        WitchBrew::Weakness
    } else {
        WitchBrew::Harm
    }
}

/// The speed of a thrown potion: 0.45 within 2 blocks, 0.75 beyond.
pub fn witch_speed(dist: f64) -> f64 {
    if dist <= 2.0 { 0.45 } else { 0.75 }
}

/// The unit direction of a shot from the start height `start_y`, the offsets `xd`, `zd` and `yd`, and the lift per horizontal block.
pub fn aim_dir(xd: f64, zd: f64, yd: f64, lift: f64, start_y: f64) -> Vec3 {
    let yo = ground_dist(xd, zd) * lift;
    Vec3::new(xd, yd + yo - start_y, zd).normalize()
}

/// The spread of a shot for an uncertainty in degrees (`Projectile.shoot`).
pub fn spread_of(uncertainty: f64) -> f64 {
    0.017_227_5 * uncertainty
}

/// The velocity of a shot: the direction spread by a triangle distribution (two draws per axis, x then y then z) at `speed`.
pub fn shot_velocity(dir: Vec3, spread: f64, speed: f64, mut draw: impl FnMut() -> f64) -> Vec3 {
    let mut tri = || 0.0 + spread * (draw() - draw());
    let x = (dir.x + tri()) * speed;
    let y = (dir.y + tri()) * speed;
    let z = (dir.z + tri()) * speed;
    Vec3::new(x, y, z)
}
