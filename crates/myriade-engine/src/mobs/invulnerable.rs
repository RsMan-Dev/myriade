//! Who may hurt a mob: `LivingEntity.isInvulnerableTo`'s `Entity.isInvulnerableToBase` term,
//! and the dead-or-removed guard every damage hook runs after (`LivingIncomingDamageEvent`).
use super::{Damage, DamageKind, Who};

