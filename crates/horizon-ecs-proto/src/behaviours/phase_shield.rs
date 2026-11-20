//! The wither's hurt filter (`WitherBoss.accepts`, `hurtServer`), as a predicate the damage path asks before it applies a hurt
//! (hook `accept` to add to `Module`, see the notes). Refused: any hurt while charging unless its kind bypasses invulnerability;
//! a direct arrow while powered (at or below half health); an undead attacker, or the animal itself. An accepted hurt arms the
//! block breaker and the side heads' idle counters.
use super::*;
use crate::behaviours::{block_breaker, invulnerable_charge, side_heads};
use crate::cow::*;

/// What the damage path tells of the hurt.
#[derive(Clone, Copy, Debug)]
pub struct Hurt {
    pub bypass_invulnerable: bool,
    pub direct_arrow: bool,
    pub attacker: Option<Entity>,
    pub undead_attacker: bool,
}
/// `WitherBoss.isPowered`: health at or below half.
pub fn powered(w: &World, e: Entity) -> bool {
    let (Some(h), Some(m)) = (w.get::<Health>(e), w.get::<MaxHealth>(e)) else { return false };
    h.0 <= m.0 / 2.0
}
pub fn accepts(w: &mut World, e: Entity, hurt: Hurt) -> bool {
    if invulnerable_charge::charging(w, e) && !hurt.bypass_invulnerable {
        return false;
    }
    if powered(w, e) && hurt.direct_arrow {
        return false;
    }
    if hurt.undead_attacker || hurt.attacker == Some(e) {
        return false;
    }
    block_breaker::hurt(w, e);
    side_heads::hurt(w, e);
    true
}
