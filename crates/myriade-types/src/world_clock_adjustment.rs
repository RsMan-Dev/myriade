use crate::*;


/// A move of a level's clock (NeoForge's `ClockAdjustment`). The game resolves it once every
/// listener has run. Case order is part of the wasm link contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClockAdjustment {
    /// To this clock time, in ticks.
    Absolute(i64),
    /// This many ticks on from the clock's time now.
    Relative(i64),
    /// To the next time the level's clock reaches this time marker of its timelines, such as
    /// `minecraft:wake_up_from_sleep`.
    Marker(Identifier),
}

impl ClockAdjustment {
    /// The clock time this adjustment moves to from `current`, with `marker` answering when a
    /// marker is next reached; None for a marker the clock does not define and for a `Relative`
    /// time past the ends of `i64` (it never wraps).
    pub fn resolve(
        &self,
        current: i64,
        marker: impl FnOnce(&Identifier) -> Option<i64>,
    ) -> Option<i64> {
        match self {
            Self::Absolute(time) => Some(*time),
            Self::Relative(ticks) => current.checked_add(*ticks),
            Self::Marker(id) => marker(id),
        }
    }
}
