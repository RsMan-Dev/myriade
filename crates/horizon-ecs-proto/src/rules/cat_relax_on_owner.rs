//! Calculations of the cat's relaxing beside its sleeping owner: the foot of the bed, the goal as floats, and the count on the bed.
use crate::rules::cat_state::reduced;

/// The foot of a bed: the block behind the bed's `facing` (`back`), from the block `at` of the owner.
pub fn goal_behind_bed(at: [i32; 3], facing: Option<&str>) -> [i32; 3] {
    let back = match facing {
        Some("north") => [0, 0, 1],
        Some("south") => [0, 0, -1],
        Some("east") => [-1, 0, 0],
        Some("west") => [1, 0, 0],
        Some("up") => [0, -1, 0],
        Some("down") => [0, 1, 0],
        _ => [0, 0, 0],
    };
    [at[0] + back[0], at[1] + back[1], at[2] + back[2]]
}

/// The goal block as the position the walk goes to.
pub fn goal_f(g: [i32; 3]) -> [f64; 3] {
    [f64::from(g[0]), f64::from(g[1]), f64::from(g[2])]
}

/// Whether the cat lies down after `on_bed` ticks on the bed: more than `reduced(asks)`.
pub fn lies_down(on_bed: i32, asks: i32) -> bool {
    on_bed > reduced(asks)
}
