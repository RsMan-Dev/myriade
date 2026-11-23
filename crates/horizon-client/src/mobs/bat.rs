//! Bats (`Bat`): hanging upside down under a solid block until a player comes
//! near, then fluttering about towards random nearby air (`customServerAiStep`),
//! and `BatModel`'s flying and resting keyframe animations. `extra.flag` is
//! `isResting`, `extra.positions[0]` the `targetPosition`; animation states:
//! flying, resting.
use super::{MobEvent, MobWorld, data::REDSTONE_CONDUCTOR, living::wrap};
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
