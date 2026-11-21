//! The look's countdown: the ticks a look waits for when it starts counting, from `min` and the draw of `spread`.

/// The ticks to count: when nothing is left (`left` is 0) the draw `min + draw() - 1` starts the count, else the `left` ticks go on.
pub fn wait_ticks(left: i32, min: i32, draw: impl FnOnce() -> i32) -> i32 {
    if left == 0 { min + draw() - 1 } else { left }
}
