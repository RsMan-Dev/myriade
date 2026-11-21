use horizon_core::position::BlockPos;

/// `TrialSpawner`'s target of a wave: `base + per_player * additional`, floored (the players beyond the first add to it).
pub fn wave_target(base: f32, per_player: f32, additional: i32) -> i32 {
    (base + per_player * additional as f32).floor() as i32
}

/// Whether the detection runs at this tick for the spawner at `pos`: `(pack(pos) + now) % 20 == 0`.
pub fn aligned(pos: [i32; 3], now: u64) -> bool {
    BlockPos::new(pos[0], pos[1], pos[2]).pack().wrapping_add(now as i64) % 20 == 0
}

/// The first aligned tick after `now` (the detection's ticks).
pub fn next_aligned(pos: [i32; 3], now: u64) -> u64 {
    (now + 1..).find(|&t| aligned(pos, t)).unwrap_or(now + 1)
}

/// The centre of the block at `pos`.
pub fn center(pos: [i32; 3]) -> [f64; 3] {
    pos.map(|v| f64::from(v) + 0.5)
}
