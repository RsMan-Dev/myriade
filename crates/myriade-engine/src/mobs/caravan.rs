use horizon_core::geometry::Vec3;

const CARAVAN_LIMIT: i32 = 8;

/// `LlamaFollowCaravanGoal`.
#[derive(Clone, Debug)]
pub struct FollowCaravan {
    speed: f64,
    dist_check: i32,
}
impl FollowCaravan {
    pub fn new(speed: f64) -> Self {
        Self {
            speed,
            dist_check: 0,
        }
    }
}

#[cfg(test)]
mod defend_trader_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};
    use crate::mobs::{Species, sensor_scan_tests::kind};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;

}
