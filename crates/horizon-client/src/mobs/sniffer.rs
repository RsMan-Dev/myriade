//! Sniffers (`Sniffer`, `SnifferAi`): the brain's core, idle, sniff and dig
//! activities, scenting and sniffing for diggable ground, walking there,
//! digging up a seed, the explored positions, laying eggs when bred, and
//! `SnifferModel`'s keyframe animations.
use super::{
    MobEvent,
    anim::{Poses, State},
    keyframes::Animation,
};
use std::collections::HashMap;

// Sniffer state: `variant` the state (below), `scratch[0]` the state the
// animations last saw, `positions` the explored positions (newest first),
// `ints[3]` the tick the dug seed drops. AnimState states: feeling happy,
// scenting, sniffing, digging, rising.
pub const IDLING: u8 = 0;
pub const FEELING_HAPPY: u8 = 1;
pub const SCENTING: u8 = 2;
pub const SNIFFING: u8 = 3;
pub const SEARCHING: u8 = 4;
pub const DIGGING: u8 = 5;
pub const RISING: u8 = 6;

/// `Sniffer.getBabyStartAge`.
pub const BABY_START_AGE: i32 = -48000;

/// `Sniffer.canMate`'s state test.
pub fn can_mate_state(s: u8) -> bool {
    matches!(s, IDLING | SCENTING | FEELING_HAPPY)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_calm_states_can_mate() {
        assert!(can_mate_state(IDLING));
        assert!(can_mate_state(SCENTING));
        assert!(can_mate_state(FEELING_HAPPY));
        assert!(!can_mate_state(SNIFFING));
        assert!(!can_mate_state(DIGGING));
    }
}

