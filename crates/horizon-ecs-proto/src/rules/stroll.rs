//! Calculs de la promenade (`RandomStrollGoal`) : la graine du tirage d'un essai après un coup.

/// The seed of the stroll draw armed after a blow: the animal's id and the tick, mixed with a fixed constant, odd.
pub fn after_hurt_seed(uid: u64, now: u64) -> u64 {
    (uid << 32 ^ now ^ 0x9e37_79b9_7f4a_7c15) | 1
}
