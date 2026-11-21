/// `hide_pos`: one random offset within 10 blocks horizontally and 3 vertically; the draws are taken in the order x, y, z.
pub fn hide_offset(draw: &mut impl FnMut(i32) -> i32) -> [i32; 3] {
    let dx = draw(20) - 10;
    let dy = draw(6) - 3;
    let dz = draw(20) - 10;
    [dx, dy, dz]
}

/// The village countdown after an ask: `left` while it runs, else the reset `interval`.
pub fn next_interval(left: i32, interval: i32) -> i32 {
    if left > 0 { left } else { interval }
}
