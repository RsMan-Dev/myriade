//! Mob metadata and live attributes, built on the owning server thread.
//! With multiple tick lanes, metadata is deferred until after the snapshot pass; it is still
//! built serially because `Mob` contains non-`Sync` interior-mutable state.
use super::*;
use horizon_client::mobs::villager::ResolvedProfessions;

/// One mob's entity data (`meta`, and the `defaults` its entries can go back to); whether its
/// position is sent precisely.
#[allow(clippy::too_many_arguments)]
fn mob_state(
    m: &Mob,
    sync: &SyncData,
    professions: &ResolvedProfessions,
    effects: Vec<(u8, i32, Value)>,
    player_entity: Option<i32>,
    game_time: i64,
    meta: &mut Vec<(u8, i32, Value)>,
    defaults: &mut Vec<(u8, i32, Value)>,
) -> bool {
    super::super::villager_client::server_meta_sync(
        m,
        sync,
        effects,
        (player_entity, game_time),
        meta,
        defaults,
        professions,
    );
    mob_freeze_meta(m, meta, defaults);
    // HappyGhast.aiStep: requiresPrecisePosition while on a still timeout.
    m.species() == horizon_client::mobs::Species::HappyGhast
        && horizon_client::mobs::happy_ghast::on_still_timeout(m)
}

#[cfg(test)]
thread_local! {
    /// A test's choice of deferred or immediate metadata construction.
    static DEFER: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Whether metadata waits until after the snapshot pass.
fn deferring() -> bool {
    #[cfg(test)]
    if let Some(defer) = DEFER.with(std::cell::Cell::get) {
        return defer;
    }
    horizon_client::parallel::lanes() > 1
}

/// What a waiting mob needs besides the mob: (its slot in the states, its index in the mobs, the
/// player entity its lead may name, its effect entries, which the hub makes).
type Waiting = (usize, usize, Option<i32>, Vec<(u8, i32, Value)>);

/// The mobs' states of one pass, with metadata optionally deferred until after the pass.
pub(super) struct Later {
    waiting: Option<Vec<Waiting>>,
    game_time: i64,
    speeds: super::super::lan_attributes::SpeedByType,
}

impl Later {
    pub(super) fn new(game_time: i64) -> Self {
        Self {
            waiting: deferring().then(Vec::new),
            game_time,
            speeds: Default::default(),
        }
    }

    /// Mob `index`'s state `e`, in `slot` of the states: built now, or left for [`Self::fill`].
    /// The attributes are always built now.
    // `mob_attributes` can initialize `m.extra.live` through a shared read.
    // Its `std::cell::OnceCell` is not `Sync`; neither attributes nor metadata
    // may share `&Mob` with helper threads, even if the live map was built first.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn state(
        &mut self,
        game: &Game<'_>,
        (slot, index): (usize, usize),
        m: &Mob,
        player_entity: Option<i32>,
        hub: &Hub,
        e: &mut EntityState,
    ) {
        let effects = mob_effect_meta(m, hub);
        if let Some(waiting) = &mut self.waiting {
            waiting.push((slot, index, player_entity, effects));
        } else {
            e.precise = mob_state(
                m,
                hub.sync_data(),
                &game.stations().professions,
                effects,
                player_entity,
                self.game_time,
                &mut e.meta,
                &mut e.defaults,
            );
        }
        game.mob_attributes(m, &mut self.speeds, &mut e.attributes);
    }

    /// The waiting metadata, built serially on the owning server thread and put in place.
    pub(super) fn fill(self, game: &Game<'_>, mobs: &[Mob], sync: &SyncData, out: &mut States) {
        let Some(waiting) = self.waiting else {
            return;
        };
        let (game_time, professions) = (self.game_time, &game.stations().professions);
        let states = out.as_mut_slice();
        for (slot, index, player, effects) in waiting {
            let e = &mut states[slot];
            e.precise = mob_state(
                &mobs[index],
                sync,
                professions,
                effects,
                player,
                game_time,
                &mut e.meta,
                &mut e.defaults,
            );
        }
    }
}

