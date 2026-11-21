//! Calculs du suivi du maître (`stay_close_to_owner`) : la distance de Manhattan entre cases, à comparer à l'arrêt.

/// The owner is close enough to stop: the Manhattan distance of their blocks (floored) is at most `stop`.
pub fn near_owner(owner: [f64; 3], me: [f64; 3], stop: f64) -> bool {
    (owner[0].floor() - me[0].floor()).abs() + (owner[1].floor() - me[1].floor()).abs() + (owner[2].floor() - me[2].floor()).abs() <= stop
}
