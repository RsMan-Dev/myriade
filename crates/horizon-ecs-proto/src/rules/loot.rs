//! The numbers of a death's loot: the table an entity rolls, the seed its death is rolled with and the experience it leaves.
use horizon_core::random::{LegacyRandom, RandomSource};

/// The table of an entity type as `Mob.getLootTable` names it (`<ns>:entities/<path>`; `minecraft` when the name has no namespace).
pub fn entity_table(kind: &str) -> String {
    let (ns, path) = kind.split_once(':').unwrap_or(("minecraft", kind));
    format!("{ns}:entities/{path}")
}
/// The seed a death's own drops are rolled with: the entity's death seed, its uid mixed in.
pub fn death_seed(seed: i64, uid: i32) -> i64 {
    seed ^ 0x2545_F491 ^ i64::from(uid)
}
/// The experience a death leaves: `min` plus a draw of `max - min + 1` values, never negative.
pub fn xp_amount(min: i32, max: i32, rng: &mut LegacyRandom) -> u32 {
    // A fixed reward (a monster's 5) draws nothing.
    if max <= min {
        return min.max(0) as u32;
    }
    (min + rng.next_int_bound(max - min + 1).unwrap_or(0)).max(0) as u32
}
