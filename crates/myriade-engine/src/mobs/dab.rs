use horizon_core::geometry::Vec3;

/// The four settings. `start` is in blocks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dab {
    start_squared: i64,
    max_tick_freq: i64,
    dist_mod: u32,
}

impl Dab {
    /// Pufferfish's defaults are 12, 20 and 8. A frequency below 1 counts as 1, a shift past 62 as 62.
    pub fn new(start_distance: f64, max_tick_freq: i64, activation_dist_mod: u32) -> Self {
        let start = start_distance.max(0.0);
        Self {
            start_squared: (start * start) as i64,
            max_tick_freq: max_tick_freq.max(1),
            dist_mod: activation_dist_mod.min(62),
        }
    }

    /// How many ticks pass between two AI ticks of a mob whose squared distance to the nearest
    /// player is `distance_squared`: 1 within the start distance, else `d² >> mod` between 1 and
    /// the maximum (Pufferfish's formula, on whole squared distances).
    pub fn interval(&self, distance_squared: f64) -> i64 {
        let d2 = distance_squared as i64; // saturates for huge and NaN values
        if d2 <= self.start_squared {
            return 1;
        }
        (d2 >> self.dist_mod).clamp(1, self.max_tick_freq)
    }
}

/// The host's setting: `HORIZON_DAB` on (`1`, `on`, `true`), with `HORIZON_DAB_START_DISTANCE`,
/// `HORIZON_DAB_MAX_TICK_FREQ` and `HORIZON_DAB_ACTIVATION_DIST_MOD` over the defaults. Read once.
pub fn settings() -> Option<Dab> {
    static ON: std::sync::OnceLock<Option<Dab>> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        let var = |name: &str| std::env::var(name).ok();
        if !matches!(var("HORIZON_DAB").as_deref(), Some("1" | "on" | "true")) {
            return None;
        }
        let num = |name: &str, default: f64| {
            var(name)
                .and_then(|v| v.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite())
                .unwrap_or(default)
        };
        Some(Dab::new(
            num("HORIZON_DAB_START_DISTANCE", 12.0),
            num("HORIZON_DAB_MAX_TICK_FREQ", 20.0) as i64,
            num("HORIZON_DAB_ACTIVATION_DIST_MOD", 8.0) as u32,
        ))
    })
}

/// A mob hurt this recently keeps its full AI: ~2 s (`Mob.getLastHurtByMobTimestamp`).
const HURT_TICKS: i32 = 40;







#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::Who;
    use std::sync::Arc;

    const DEFAULTS: Dab = Dab {
        start_squared: 144,
        max_tick_freq: 20,
        dist_mod: 8,
    };

    #[test]
    fn interval_follows_pufferfishs_formula() {
        let d = |blocks: f64| DEFAULTS.interval(blocks * blocks);
        assert_eq!(d(5.0), 1);
        assert_eq!(d(12.0), 1); // at the start distance
        assert_eq!(d(13.0), 1); // 169 >> 8 = 0, raised to 1
        assert_eq!(d(60.0), 14); // 3600 >> 8
        assert_eq!(d(200.0), 20); // 40000 >> 8 = 156, capped
        assert_eq!(DEFAULTS.interval(f64::INFINITY), 20);
        assert_eq!(Dab::new(12.0, 0, 99).interval(1e9), 1);
    }


    #[test]
    fn a_far_mob_runs_its_ai_one_tick_in_n_and_a_near_one_every_tick() {
        let mut m = evoker();
        // 60 blocks: interval 14, so ticks 0, 14, ..., 266 of 280.
        assert_eq!(deciding(&mut m, 60.0), 20);
        assert_eq!(deciding(&mut m, 5.0), 280);
    }


    #[test]
    fn an_odd_id_far_mob_still_gets_full_selector_updates() {
        let mut m = evoker(); // id 1
        // 60 blocks: interval 14, every AI turn on an even age.
        let turns = full_selector_turns(&mut m, 60.0);
        assert_eq!(turns.len(), 20);
        assert_eq!(turns.iter().filter(|full| **full).count(), 10);
    }

    #[test]
    fn a_far_mob_swimming_keeps_its_ai_every_tick() {
        let mut m = evoker();
        m.fluids.in_lava = true;
        assert_eq!(deciding(&mut m, 60.0), 280);
    }

    #[test]
    fn a_mob_with_a_target_keeps_its_ai_every_tick() {
        let mut m = evoker();
        m.ai.target = Some(Who::Player);
        assert_eq!(deciding(&mut m, 60.0), 280);
    }
}
