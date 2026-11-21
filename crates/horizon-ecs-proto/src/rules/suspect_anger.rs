//! Calculs de la colère par suspect (`AngerManagement`) : la colère qui reste à l'instant donné.

/// The anger left at `now` of a suspect set at `at` with `anger`, losing one point per `step` ticks; None when it has run out.
pub fn remaining(anger: u16, at: u64, now: u64, step: u64) -> Option<u16> {
    let steps = now.saturating_sub(at) / step;
    (steps < u64::from(anger)).then(|| anger - steps as u16)
}
