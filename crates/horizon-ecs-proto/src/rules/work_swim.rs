//! `WorkSwim` (`brain/behaviours/work_swim.rs`): the chance of a jump at each tick in water.

/// Whether the swimmer jumps this tick: the uniform draw `draw` is below 0.8.
pub fn jumps(draw: f64) -> bool {
    draw < 0.8
}
