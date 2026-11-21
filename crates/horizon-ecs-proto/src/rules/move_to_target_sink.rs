//! `MoveToTargetSink` (`brain/behaviours/move_to_target_sink.rs`): the distance the sink's last point must move to count as moved.

/// Whether the squared distance `d` (in block cells) between the last and the current point is beyond the sink's 4.
pub fn moved_far(d: i64) -> bool {
    d > 4
}
