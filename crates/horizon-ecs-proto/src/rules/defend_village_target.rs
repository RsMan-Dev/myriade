//! The golem's pick among the players in reach: the nearest, then the lowest id.

/// The nearest of `players` (squared distance, id), the lowest id on a tie; `None` when there is none.
pub fn nearest_player(players: Vec<(f64, u32)>) -> Option<u32> {
    players.into_iter().min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1))).map(|(_, id)| id)
}
