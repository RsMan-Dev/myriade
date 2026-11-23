//! What a client's copy of a mob makes on its own, in the
//! `level().isClientSide()` branches of `aiStep`: nothing of it reaches the
//! server or goes over the network, so the window's game runs it as it follows
//! a server.
use super::{MobEvent, Species, bee};
use horizon_core::geometry::trig::Mth;
use horizon_core::random::RandomSource;
use std::f32::consts::PI;




#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;
    use horizon_core::geometry::Vec3;



    /// `Dolphin.tick` on a client: facing south (yaw 0, level), a swimming
    /// dolphin leaves two pairs of particles 0.3 to either side, set back
    /// 0.5 to 1.2 behind it on the same height.
    #[test]
    fn a_swimming_dolphin_leaves_four_trail_particles_behind_it() {
        let mut m = dolphin();
        let at = trail(&mut m);
        assert_eq!(at.len(), 4);
        let back = 20.0 - at[0][2];
        assert!((0.5..=1.2).contains(&back), "set back {back}");
        for (i, p) in at.iter().enumerate() {
            let side = if i % 2 == 0 { 0.3 } else { -0.3 };
            assert!((p[0] - (10.0 + side)).abs() < 1e-6, "{p:?}");
            assert_eq!((p[1], p[2]), (60.0, at[0][2]));
        }
    }

    /// No trail out of water, at a squared speed of 0.03 or less, or with NoAI.
    #[test]
    fn a_slow_dry_or_no_ai_dolphin_leaves_no_trail() {
        let mut m = dolphin();
        m.fluids.in_water = false;
        assert!(trail(&mut m).is_empty());
        let mut m = dolphin();
        m.velocity = Vec3::new(0.0, 0.0, 0.17);
        assert!(trail(&mut m).is_empty());
        let mut m = dolphin();
        m.no_ai = true;
        assert!(trail(&mut m).is_empty());
    }
}
