//! `TradeShowOffers` (`brain/behaviours/trade_show_offers.rs`): the reach of the player the offers are shown to.

/// Whether a player at squared distance `dist2` is shown the offers: 17 squared or less.
pub fn in_reach(dist2: f64) -> bool {
    dist2 <= 17.0
}
