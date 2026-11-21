//! The numbers of a block search and its stay: the cells a search walks, the box it reaches, the point it aims at and when it is reached.

/// The first cell `valid` accepts, walking outward from `base` by rings, the heights alternating from `vertical_start` to `vertical_range`
/// (each cell is tested one block below its height); `None` when none is.
pub fn first_valid(base: [i32; 3], range: i32, vertical_start: i32, vertical_range: i32, mut valid: impl FnMut([i32; 3]) -> bool) -> Option<[i32; 3]> {
    let next = |v: i32| if v > 0 { -v } else { 1 - v };
    let mut y = vertical_start;
    while y <= vertical_range {
        for r in 0..range {
            let mut x = 0;
            while x <= r {
                let mut z = if x < r && x > -r { r } else { 0 };
                while z <= r {
                    let p = [base[0] + x, base[1] + y - 1, base[2] + z];
                    if valid(p) {
                        return Some(p);
                    }
                    z = next(z);
                }
                x = next(x);
            }
        }
        y = next(y);
    }
    None
}
/// The box of cells a search from `base` reaches, or `None` when it walks none.
pub fn search_zone(base: [i32; 3], range: i32, vertical_start: i32, vertical_range: i32) -> Option<([i32; 3], [i32; 3])> {
    if range <= 0 || vertical_start > vertical_range {
        return None;
    }
    let reach = range - 1;
    let height = vertical_start.saturating_abs().max(vertical_range.saturating_abs()).saturating_add(1);
    Some((
        [base[0].saturating_sub(reach), base[1].saturating_sub(height).saturating_sub(1), base[2].saturating_sub(reach)],
        [base[0].saturating_add(reach), base[1].saturating_add(height), base[2].saturating_add(reach)],
    ))
}
/// Whether one of `blocks` lies in the box `lo` to `hi` (both included).
pub fn any_in_box(lo: [i32; 3], hi: [i32; 3], blocks: &[(i32, i32, i32)]) -> bool {
    blocks.iter().any(|&(x, y, z)| (lo[0]..=hi[0]).contains(&x) && (lo[1]..=hi[1]).contains(&y) && (lo[2]..=hi[2]).contains(&z))
}
/// `canContinueToUse`'s tiredness and length: the stay is not over (`try_ticks` counts up, `max_stay` is the allowance).
pub fn can_continue_ticks(try_ticks: i32, max_stay: i32) -> bool {
    try_ticks >= -max_stay && try_ticks <= 1200
}
/// The point a walk aims at: the middle of the block above `block`.
pub fn target_point(block: [i32; 3]) -> [f64; 3] {
    [f64::from(block[0]) + 0.5, f64::from(block[1] + 1), f64::from(block[2]) + 0.5]
}
/// The point the reach is measured to: the middle of the block above `block`, half a block up.
pub fn reach_point(block: [i32; 3]) -> [f64; 3] {
    [f64::from(block[0]) + 0.5, f64::from(block[1] + 1) + 0.5, f64::from(block[2]) + 0.5]
}
/// Whether `pos` is at least `accepted` from `point` (not reached).
pub fn is_far(pos: [f64; 3], point: [f64; 3], accepted: f64) -> bool {
    let d2 = (pos[0] - point[0]).powi(2) + (pos[1] - point[1]).powi(2) + (pos[2] - point[2]).powi(2);
    d2 >= accepted * accepted
}
