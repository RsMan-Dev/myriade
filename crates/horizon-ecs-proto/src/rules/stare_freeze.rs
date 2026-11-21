//! Calculs du regard figeant (`Creaking.checkCanMove`) : direction normée, cône de vue, hauteurs visées et portée du réveil.

/// The unit vector of `v`, zero when it is too short to have a direction.
pub fn normalize(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < f64::from(1.0e-5_f32) { [0.0; 3] } else { [v[0] / len, v[1] / len, v[2] / len] }
}

/// The three heights tested on the target: its eyes, its middle, and its feet + 0.5 (`isLookingAtMe`).
pub fn sight_heights(eye_y: f64, foot_y: f64) -> [f64; 3] {
    [eye_y, foot_y + 0.5, (eye_y + foot_y) / 2.0]
}

/// The view cone: the gaze `look` (unit) and the offset `offset` to the target make a dot above `dot_min`.
pub fn in_cone(look: [f64; 3], offset: [f64; 3], dot_min: f64) -> bool {
    let d = normalize(offset);
    look[0] * d[0] + look[1] * d[1] + look[2] * d[2] > dot_min
}

/// The squared distance between two points.
pub fn dist_sq(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}
