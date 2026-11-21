/// The seed of a raider spawned at block `pos` in wave `wave` of a raid: the block's x and z, the wave, and a golden-ratio constant.
pub fn spawn_seed(pos: [i32; 3], wave: i32) -> u64 {
    u64::from(pos[0] as u32) << 32 ^ u64::from(pos[2] as u32) ^ (wave as u64) << 8 ^ 0x9E37_79B9_7F4A_7C15
}
