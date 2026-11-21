//! The fox's stroll through a village: the block and the section of a point, and the score of a spot by its sections to a village point.

/// The block a position stands in.
pub fn block_pos(p: [f64; 3]) -> [i32; 3] {
    [p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32]
}
/// The section (16 blocks wide) of a block.
pub fn section_of(p: [i32; 3]) -> [i32; 3] {
    [p[0] >> 4, p[1] >> 4, p[2] >> 4]
}
/// The score of a spot: the fewer sections to a village point, the higher (`-sections`).
pub fn village_score(sections: i32) -> f64 {
    -f64::from(sections)
}
