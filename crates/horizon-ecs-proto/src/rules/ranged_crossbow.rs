//! Calculs de l'arbalète à distance (`RangedCrossbowAttackGoal`, pillard) : marche ou arrêt, vitesse chargée, visée de la flèche.
use horizon_core::geometry::Vec3;

/// The crossbow walks toward the target while it is out of `radius` (squared distance `d2`) or unseen for 5 ticks, and not in its attack delay.
pub fn needs_to_move(d2: f64, radius: f64, see_time: i32, attack_delay: i32) -> bool {
    (d2 > radius * radius || see_time < 5) && attack_delay == 0
}

/// The walking speed: half of `speed` while the crossbow is charged.
pub fn walk_speed(speed: f64, uncharged: bool) -> f64 {
    if uncharged { speed } else { speed * 0.5 }
}

/// The unit direction of the bolt from `start_y`: a third of the target's `height` above its `target_y`, and 0.2 per horizontal block up.
pub fn aim_dir(xd: f64, zd: f64, target_y: f64, height: f64, start_y: f64) -> Vec3 {
    let dist = (xd * xd + zd * zd).sqrt();
    let yd = target_y + height / 3.0 - start_y + dist * f64::from(0.2_f32);
    Vec3::new(xd, yd, zd).normalize()
}
