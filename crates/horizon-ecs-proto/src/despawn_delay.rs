//! `WanderingTrader.despawnDelay` (48000, set by the spawner) and `TraderLlama`'s own (47999, `DEFAULT_DESPAWN_DELAY`). Each count is a
//! deadline of the wheel (`Due::DespawnDelay`), not a tick counter. The trader's count is suspended while it trades: `close` pushes the
//! deadline by the time the trade took (`trade_ended`). The trader's leashed llamas go one tick before it (`TraderLlama.maybeDespawn`
//! reads the trader's delay minus one), and a llama held by a trader follows the trader's delay, not its own.
use crate::cow::*;
use crate::leash::{holder_of, Holder, Leashes};
use bevy_ecs::prelude::*;

/// `WanderingTrader`'s delay at its spawn.
pub const DELAY: u64 = 48000;

/// The tick the trader (or the llama on its own) goes at: sparse, only the carriers have it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct DespawnDelay {
    pub due: u64,
}

/// `WanderingTraderSpawner`: the trader gets its delay at its spawn.
pub fn start(w: &mut World, e: Entity) {
    start_for(w, e, DELAY);
}

/// The delay of `ticks` from now: the species data's own delay at the birth (`spawn_species`), or the spawner's for the trader.
pub fn start_for(w: &mut World, e: Entity, ticks: u64) {
    let due = now_of(w, e) + ticks;
    w.entity_mut(e).insert(DespawnDelay { due });
    arm(w, e, due);
}

/// The restored entity (`DespawnDelay` in the save): its delay is the ticks left from now.
pub fn restore(w: &mut World, e: Entity, left: i32) {
    let due = now_of(w, e) + u64::try_from(left).unwrap_or(0);
    w.entity_mut(e).insert(DespawnDelay { due });
    arm(w, e, due);
}

/// The wheel comes one tick before the due (the llamas) and on the due (the trader or the llama).
fn arm(w: &mut World, e: Entity, due: u64) {
    arm_at(w, e, due.saturating_sub(1), Due::DespawnDelay);
    arm_at(w, e, due, Due::DespawnDelay);
}

/// A trade ended at `now`, begun at `since`: the trader's count did not run during it, so its deadline moves by the trade's length.
pub fn trade_ended(w: &mut World, e: Entity, since: u64) {
    let now = now_of(w, e);
    let Some(mut d) = w.get_mut::<DespawnDelay>(e) else { return };
    d.due += now.saturating_sub(since);
    let due = d.due;
    drop(d);
    arm(w, e, due);
}

/// The deadline. A trader: its held llamas go one tick before it, and it goes at its due unless it is trading (the close of the trade
/// moves the due). A llama on its own goes at its due, if it may and no trader holds it.
pub fn due(w: &mut World, e: Entity) {
    let Some(d) = w.get::<DespawnDelay>(e).copied() else { return };
    let now = now_of(w, e);
    if !leads_llamas(w, e) {
        if now >= d.due && can_despawn(w, e) && !leashed_to_trader(w, e) {
            crate::despawn::remove(w, e);
        }
        return;
    }
    if now + 1 == d.due {
        let llamas: Vec<Entity> = w.get_resource::<Leashes>().map_or_else(Vec::new, |l| l.held_by(Holder::Entity(e)).to_vec());
        for llama in llamas {
            if can_despawn(w, llama) {
                crate::despawn::remove(w, llama);
            }
        }
    } else if now >= d.due && w.get::<crate::behaviours::trade_session::Trading>(e).is_none() {
        crate::despawn::remove(w, e);
    }
}

/// The species data's `leads_llamas`, read from the entity's species.
fn leads_llamas(w: &World, e: Entity) -> bool {
    let Some(s) = w.get::<SpeciesId>(e) else { return false };
    let name = w.resource::<SpeciesTable>().0[s.0 as usize].name;
    crate::species::registry().by_name(name).is_ok_and(|r| r.leads_llamas)
}

fn leashed_to_trader(w: &World, e: Entity) -> bool {
    matches!(holder_of(w, e), Some(Holder::Entity(h)) if leads_llamas(w, h))
}

/// `TraderLlama.canDespawn` for the parts the port holds: not tamed, not locked by age, not persistent.
fn can_despawn(w: &World, e: Entity) -> bool {
    w.get::<crate::spring_trap::Tamed>(e).is_none() && w.get::<crate::age_lock::AgeLocked>(e).is_none() && w.get::<crate::despawn::Persistent>(e).is_none()
}
