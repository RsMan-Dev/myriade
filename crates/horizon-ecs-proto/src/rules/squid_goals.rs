//! Calculs des buts du calmar (`SquidRandomMovementGoal`, `SquidFleeGoal`) : le vecteur de nage d'une marche et d'une fuite.
use horizon_core::geometry::trig::Mth;

/// The swim vector of a random move: a horizontal push of 0.2 along the angle `a` (radians), and the vertical push `y`.
pub fn wander_vector(a: f32, y: f32) -> [f64; 3] {
    [f64::from(a.mcos() * 0.2), f64::from(y), f64::from(a.msin() * 0.2)]
}

/// The swim vector of a flight from the offset `f` to the attacker: 3 blocks away, less beyond 5, no height in the air, over 20 ticks.
pub fn flee_vector(mut f: [f64; 3], air: bool) -> [f64; 3] {
    let length = (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt();
    if length > 0.0 {
        let mut avoid = 3.0;
        if length > 5.0 {
            avoid -= (length - 5.0) / 5.0;
        }
        if avoid > 0.0 {
            f = f.map(|c| c * avoid);
        }
    }
    if air {
        f[1] = 0.0;
    }
    [f[0] / 20.0, f[1] / 20.0, f[2] / 20.0]
}
