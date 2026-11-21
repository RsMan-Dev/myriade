//! The rules of the dragon's death: its length, the experience of the death, the blasts' jitter and the share of the orbs.
use horizon_core::random::{LegacyRandom, RandomSource};

/// The ticks of the death (`tick_dragon_death`).
pub const DEATH_TICKS: i32 = 200;

/// The experience of the death: the fight's first kill gives 12 000, any other 500.
pub fn death_xp(is_fight_dragon: bool, previously_killed: bool) -> u32 {
    if is_fight_dragon && !previously_killed { 12_000 } else { 500 }
}

/// Whether a blast of the death is set off on tick `t`.
pub fn is_blast_tick(t: i32) -> bool {
    (180..=DEATH_TICKS).contains(&t)
}

/// Whether orbs are dropped on tick `t`: from tick 150, every 5 ticks.
pub fn is_orb_tick(t: i32) -> bool {
    t > 150 && t % 5 == 0
}

/// A draw in -0.5..0.5 times `k` of the dragon's own generator (`(next_float() - 0.5) * k`).
pub fn jitter(r: &mut LegacyRandom, k: [f32; 3]) -> [f64; 3] {
    k.map(|k| f64::from((r.next_float() - 0.5) * k))
}

/// The share of the experience `xp` given by the orbs of one drop: `xp * share`, truncated.
pub fn orb_share(xp: u32, share: f32) -> u32 {
    (xp as f32 * share) as u32
}
