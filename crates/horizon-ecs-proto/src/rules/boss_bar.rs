//! Calculations of the boss bar: its progress.

/// The bar's progress: the health over its maximum, within 0 and 1.
pub fn bar_progress(health: f32, max: f32) -> f32 {
    (health / max).clamp(0.0, 1.0)
}
