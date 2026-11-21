//! The trader's reach and hurt tests for a trade: how far the player may stand, and how long a hit keeps the merchant from trading.

/// ASSUMED : `invulnerableTime` de vanilla après un coup, 10 ticks ; pas de constante nommée côté ECS (`cow.rs` le documente).
pub const HURT_INVULNERABLE_TICKS: u64 = 10;
/// `TradeWithPlayer` : le joueur à 4 blocs au plus (`distanceToSqr > 16.0` au jar, comme le `Mob`).
pub const TRADE_RANGE2: f64 = 16.0;

/// Whether the merchant was hurt less than `HURT_INVULNERABLE_TICKS` ago, at `now`.
pub fn still_invulnerable(hurt_at: u64, now: u64) -> bool {
    hurt_at + HURT_INVULNERABLE_TICKS > now
}

/// Whether the player at squared distance `d2` is within trading reach.
pub fn in_trade_range(d2: f64) -> bool {
    d2 <= TRADE_RANGE2
}
