//! The generator of client-only draws (sound pitches, particle chances): it is no part of the
//! level's stream, so the server's random never moves for them.
use horizon_core::random::{LegacyRandom, LevelRandom};

/// A cosmetic generator, seeded from the clock.
pub(in crate::game) fn cosmetic_random() -> LevelRandom {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as i64);
    LevelRandom::for_level(LegacyRandom::new(nanos))
}
