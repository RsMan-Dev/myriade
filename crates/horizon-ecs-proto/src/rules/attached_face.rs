//! Lid of a body fixed to a block face (`attached_face`): its easing toward the raw peek and the physical peek it stands at.

/// The lid's rate of easing per tick, the same for every lid.
pub const LID_SPEED: f32 = 0.05;

/// One tick of easing: the lid moves `LID_SPEED` toward `raw / 100` (`raw` the raw peek), never past it.
pub fn eased_step(old: f32, raw: i32) -> f32 {
    let target = raw as f32 * 0.01;
    if old > target { (old - LID_SPEED).max(target) } else { (old + LID_SPEED).min(target).max(0.0) }
}

/// `getPhysicalPeek`: how far the lid stands from the body's wall.
pub fn physical_peek(eased: f32) -> f32 {
    0.5 - ((0.5 + eased) * std::f32::consts::PI).sin() * 0.5
}
