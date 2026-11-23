//! `server.toml` `[entities] villager_lobotomize` (docs/server.md): a villager that cannot move
//! skips its brain. Vanilla has no such rule (Purpur's `villager.lobotomize` is the model); off by
//! default, it changes nothing.
//!
//! A villager is *stuck* when it rides something, or when no horizontal neighbour is walkable (see
//! [`is_stuck`]). The rule is rechecked every `check_interval` ticks, staggered by the villager's id
//! so a hall of villagers does not all recheck on one tick. A stuck villager skips
//! `Villager.customServerAiStep`'s brain (sensors, behaviours, activity changes, gossip, sleep,
//! farming, breeding, panic, POI searches), but keeps gravity, damage, despawn, trading and its XP,
//! and `Villager.restock` on `WorkAtPoi`'s schedule (see [`skips_brain`]).
use super::super::villager_brain;
use super::super::{MobEvent, MobWorld};
use horizon_core::geometry::Vec3;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);
static INTERVAL: AtomicU32 = AtomicU32::new(DEFAULT_INTERVAL);

/// `villager_lobotomize_check_interval`'s default, in ticks.
pub const DEFAULT_INTERVAL: u32 = 100;
/// `WorkAtPoi`: a villager that works tries at most every 300 ticks.
const RESTOCK_EVERY: i64 = 300;

/// Sets the server's `[entities]` settings (the dedicated server reads them at start).
pub fn configure(enabled: bool, check_interval: u32) {
    ENABLED.store(enabled, Ordering::Relaxed);
    INTERVAL.store(check_interval.max(1), Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// The most cells a stuck villager can reach. A trading hall's cell often holds a bed or a
/// workstation's neighbour, so "no free neighbour" is too strict: a villager that can only shuffle
/// around a small pocket is as good as stuck.
pub const MAX_ROOM: usize = 6;







