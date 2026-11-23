//! `EndCrystalRenderer.getY`: the crystal's bob.
use horizon_core::geometry::trig::Mth;

/// `EndCrystalRenderer.getY`: the crystal's bob below its resting height.
pub fn y(age: f32) -> f32 {
    let hh = (age * 0.2).msin() / 2.0 + 0.5;
    (hh * hh + hh) * 0.4 - 1.4
}
