//! The calm down's rule: the villager stays scared while it is hurt, a hostile is near, or its hurter is within `range_sqr` (squared).

/// Whether the villager stays scared: `hurt` or `hostile` is set, or the hurter's squared distance `hurter_d2` is within `range_sqr`.
pub fn keeps_scared(hurt: bool, hostile: bool, hurter_d2: Option<f64>, range_sqr: f64) -> bool {
    hurt || hostile || hurter_d2.is_some_and(|d| d <= range_sqr)
}
