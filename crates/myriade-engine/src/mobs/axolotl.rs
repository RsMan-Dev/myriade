use horizon_core::random::RandomSource;
pub const PLAY_DEAD_TICKS: i32 = 200;
pub const HUNTING_COOLDOWN: i64 = 2400;
/// `Axolotl.getMaxAirSupply`.
pub const MAX_AIR: i32 = 6000;

/// AxolotlAttackablesSensor: always-hostile targets bypass the hunting cooldown.
pub fn prey_eligible(
    distance: f64,
    water: bool,
    hostile: bool,
    hunt: bool,
    cooldown: bool,
    attackable: bool,
) -> bool {
    distance <= 64.0 && water && (hostile || hunt && !cooldown) && attackable
}
/// Preserve the ordered random draws in Axolotl.hurtServer, even out of water.
fn roll_play_dead(
    random: &mut impl RandomSource,
    no_ai: bool,
    health: f32,
    max: f32,
    damage: f32,
    water: bool,
    entity_source: bool,
    already: bool,
) -> bool {
    !no_ai
        && random.next_int_bound(3).unwrap_or(0) == 0
        && ((random.next_int_bound(3).unwrap_or(0) as f32) < damage || health / max < 0.5)
        && damage < health
        && water
        && entity_source
        && !already
}
#[cfg(test)]
mod axolotl_parity_tests {
    use super::*;
    use horizon_core::random::LegacyRandom;
    #[test]
    fn prey_water_range_and_cooldown_match_native() {
        assert!(prey_eligible(64.0, true, false, true, false, true));
        assert!(!prey_eligible(64.01, true, false, true, false, true));
        assert!(!prey_eligible(4.0, false, true, true, false, true));
        assert!(!prey_eligible(4.0, true, false, true, true, true));
        assert!(prey_eligible(4.0, true, true, false, true, true));
        assert!(!prey_eligible(4.0, true, false, false, false, true));
        assert!(!prey_eligible(4.0, true, true, true, false, false));
    }
    #[test]
    fn damage_roll_matches_native_draw_order_and_gates() {
        for seed in 0..100 {
            for (water, entity, already, no_ai) in [
                (true, true, false, false),
                (false, true, false, false),
                (true, false, false, false),
                (true, true, true, false),
                (true, true, false, true),
            ] {
                let mut expected = LegacyRandom::new(seed);
                let mut actual = LegacyRandom::new(seed);
                let result = !no_ai
                    && expected.next_int_bound(3).unwrap() == 0
                    && ((expected.next_int_bound(3).unwrap() as f32) < 2.0 || 4.0f32 / 14.0 < 0.5)
                    && 2.0 < 4.0
                    && water
                    && entity
                    && !already;
                assert_eq!(
                    roll_play_dead(&mut actual, no_ai, 4.0, 14.0, 2.0, water, entity, already),
                    result
                );
                assert_eq!(actual.next_int(), expected.next_int());
            }
        }
    }
    #[test]
    fn playing_dead_eases_and_selects_exclusive_baby_pose() {
        let mut x = Animation {
            playing_dead: true,
            ..Animation::default()
        };
        for _ in 0..10 {
            x.tick(true, true, true, true, 20.0);
        }
        assert_eq!(x.factor(0, 1.0), 1.0);
        assert_eq!(x.factor(1, 1.0), 0.0);
        assert_eq!(x.states.iter().filter(|s| s.started.is_some()).count(), 1);
        assert!(x.states[6].started.is_some());
        x.playing_dead = false;
        for _ in 0..10 {
            x.tick(true, true, false, true, 30.0);
        }
        assert_eq!(x.factor(0, 1.0), 0.0);
        assert_eq!(x.factor(1, 1.0), 1.0);
        assert!(x.states[1].started.is_some());
    }
}
