//! Distance test of `hostiles_near`: a creature counts when it lies within its table's limit of the entity.

/// The squared distance from `at` to `q` when it is within `limit` blocks, `None` beyond.
pub fn within_limit(at: [f64; 3], q: [f64; 3], limit: f64) -> Option<f64> {
    let d2 = (q[0] - at[0]).powi(2) + (q[1] - at[1]).powi(2) + (q[2] - at[2]).powi(2);
    (d2 <= limit * limit).then_some(d2)
}
