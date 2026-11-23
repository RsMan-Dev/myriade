//! Polar bears (`PolarBear`): a neutral animal that defends its cubs,
//! fights back when hurt, and rears up before each swing. A cub flees what
//! hurts it and calls the adults.
//!
//! `extra.flag` is `DATA_STANDING_ID`, `extra.counter` the warning sound's
//! delay, `extra.anger` the `NeutralMob` anger left and `extra.flag2` whether
//! the player is who it is angry at. `anim.v[STAND]` is
//! `clientSideStandAnimation`.
use horizon_core::{geometry::Vec3, random::RandomSource};


/// `anim.v` slot of `clientSideStandAnimation`.
pub const STAND: usize = 0;
/// `STAND_ANIMATION_TICKS`.
const STAND_TICKS: f32 = 6.0;










#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;
    use std::sync::Arc;



    /// `PolarBear.getDefaultDimensions`: the box of a bear that is not rearing
    /// is the type's, one that is rearing is `scale(1, 1 + animation / 6)`.
    #[test]
    fn a_rearing_bears_box_grows_taller_but_not_wider() {
        let mut m = bear();
        let (w0, h0, e0) = m.dims();
        let (base_h, base_e) = (h0 as f32, e0 as f32);
        assert_eq!(stand_box_scale(&m), 1.0, "no rearing, no extra height");
        // Three ticks: half the animation, so half again as tall.
        for _ in 0..3 {
            rear(&mut m);
        }
        let (w1, h1, e1) = m.dims();
        assert_eq!(w1, w0, "the width never changes");
        assert_eq!(h1, f64::from(base_h * 1.5_f32));
        assert_eq!(e1, f64::from(base_e * 1.5_f32));
        // Six ticks: the full animation, twice as tall.
        for _ in 0..3 {
            rear(&mut m);
        }
        assert_eq!(m.anim.v[STAND], STAND_TICKS);
        let (w2, h2, e2) = m.dims();
        assert_eq!(w2, w0);
        assert_eq!(h2, f64::from(base_h * 2.0));
        assert_eq!(e2, f64::from(base_e * 2.0));
        // Standing down runs the animation back and the box with it.
        for _ in 0..STAND_TICKS as i32 {
            set_standing(&mut m, false);
            client_tick(&mut m);
        }
        assert_eq!(m.dims(), (w0, h0, e0));
    }

    /// The box follows the animation, not the synched standing flag: a bear
    /// that has just started to rear is still its resting height.
    #[test]
    fn the_box_follows_the_animation_and_not_the_flag() {
        let mut m = bear();
        let (_, h0, _) = m.dims();
        let base_h = h0 as f32;
        set_standing(&mut m, true);
        assert_eq!(m.dims().1, h0, "the flag alone does not grow the box");
        client_tick(&mut m);
        assert_eq!(m.dims().1, f64::from(base_h * (1.0 + 1.0 / STAND_TICKS)));
    }
}

#[cfg(test)]
mod freeze_panic_tests {
    use super::*;

}
