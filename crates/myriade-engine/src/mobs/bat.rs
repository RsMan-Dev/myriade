use horizon_core::{
    geometry::{Vec3, trig},
    random::RandomSource,
};

/// `Math.signum`: zero stays zero (Rust's `signum` gives 1).
fn signum(v: f64) -> f64 {
    if v == 0.0 || v.is_nan() {
        v
    } else {
        v.signum()
    }
}
