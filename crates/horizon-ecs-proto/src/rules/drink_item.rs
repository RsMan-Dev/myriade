//! The trader's drink: when it is drunk (dark or day, invisible or not) and the pitch of its finish sound.

/// The potion is drunk in the dark while not invisible, the milk by day while invisible.
pub fn drink_now(dark: bool, day: bool, invisible: bool) -> bool {
    if dark { !day && !invisible } else { day && invisible }
}

/// The pitch of the finish sound, from one draw of the creature's stream.
pub fn finish_pitch(draw: f32) -> f32 {
    draw * 0.2 + 0.9
}
