//! The restock clock of a merchant: the game time of its next check, and the ticks left until it.

/// Le prochain instant où `should_restock` peut changer de réponse : 2401 ticks après le dernier réapprovisionnement s'il y en a eu un
/// aujourd'hui, sinon le changement de jour (12001) ; un échange réveille le marchand de lui-même.
pub fn next_check_at(last_restock: i64, restocks_today: i32, game: i64) -> i64 {
    let at = last_restock + if restocks_today == 1 { 2401 } else { 12001 };
    at.max(game + 1)
}

/// Ticks from `game` to the check at `check`, at least 1.
pub fn ticks_until(check: i64, game: i64) -> i32 {
    (check - game).clamp(1, i64::from(i32::MAX)) as i32
}
