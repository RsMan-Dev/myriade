//! Entities a transaction adds (arismoko/horizon#796). An abort takes out every entity that joined
//! since the transaction opened, with its residency and saved memory. Construction
//! (`entity_constructing`, a mob's finalize) stays at the spawn call. A transaction journals the
//! level it was opened in only: another level's spawns are kept at once, and an abort leaves them.
//!
//! Join numbers, not ids, name what a transaction added: every kind takes one as it is put in its
//! list, the kinds whose id comes late too, so an aborted spawn just burns its id. Bolts and
//! thrown potions have none and are kept by their lists' lengths. A traveller that arrived from
//! another level inside the transaction is left where it arrived: travel is not journaled.
use super::super::{Dim, Game, commands::Target};
use super::Level;
use myriade_engine::join_seq::{JoinSeq, Joined as _};
use std::collections::HashSet;

/// A raw record of `persist.anon` (kind and position bits) as it was: what an aborted spawn
/// puts back.
pub(in crate::game) type KeptAnon = (
    (String, [u64; 3]),
    Option<(horizon_core::geometry::Vec3, horizon_core::nbt::Tag)>,
);

/// What a transaction did to entities, for its close.
#[derive(Default)]
pub(super) struct EntityJournal {
    /// Below the join number of every entity added in the transaction (`JoinSeq::upcoming` as
    /// it opened).
    mark: JoinSeq,
    /// How many thrown potions there were as it opened.
    potions: usize,
    /// The travellers that arrived from another level, by the ids they arrived under.
    arrivals: HashSet<u32>,
    /// Each raw record a spawn wrote over in `persist.anon` (by kind and position: an end
    /// crystal standing there keeps its own there), as it was before the first such spawn.
    anon: Vec<KeptAnon>,
}

/// The innermost open transaction of `dimension`: it journals what happens in that level only,
/// whatever other levels' transactions are open inside it (a command run `in` another level can
/// open one). What happens in a level with none open is not journaled.
pub(super) fn open_in(transactions: &mut [Level], dimension: Dim) -> Option<&mut Level> {
    transactions
        .iter_mut()
        .rev()
        .find(|level| level.dimension == Some(dimension))
}

impl EntityJournal {
    /// A nested commit: the parent's mark is earlier and stays.
    pub(super) fn merge(&mut self, child: Self) {
        self.arrivals.extend(child.arrivals);
        for kept in child.anon {
            self.keep_anon(kept);
        }
    }
    /// A nested abort: its arrivals stay out of the parent's abort too.
    pub(super) fn keep_arrivals(&mut self, arrivals: HashSet<u32>) {
        self.arrivals.extend(arrivals);
    }
    /// Keeps the first record each key had: a later spawn there wrote over this one's.
    fn keep_anon(&mut self, kept: KeptAnon) {
        if !self.anon.iter().any(|(key, _)| *key == kept.0) {
            self.anon.push(kept);
        }
    }
}

impl Game<'_> {
    /// The journal a transaction opening now starts with.
    pub(super) fn entity_journal_open(&self) -> EntityJournal {
        EntityJournal {
            mark: JoinSeq::upcoming(),
            potions: self.potions.thrown.len(),
            ..EntityJournal::default()
        }
    }
    /// A spawn wrote over `kept` in `persist.anon`: an abort puts it back.
    pub(in crate::game) fn journal_anon(&mut self, kept: KeptAnon) {
        let dim = self.dim;
        if let Some(level) = open_in(&mut self.transactions, dim) {
            level.lifetimes.keep_anon(kept);
        }
    }
    /// Entity `id` arrived from another level: an abort leaves it where it is.
    pub(in crate::game) fn journal_arrival(&mut self, id: u32) {
        let dim = self.dim;
        if let Some(level) = open_in(&mut self.transactions, dim) {
            level.lifetimes.arrivals.insert(id);
        }
    }
    /// Whether an open transaction removes `target`: none does, since removals run at once.
    pub(in crate::game) fn removal_pending(&self, _target: &Target) -> bool {
        false
    }
    /// [`Self::removal_pending`] of the entity in `dimension`.
    pub(in crate::game) fn removal_pending_in(&self, _dimension: Dim, _target: &Target) -> bool {
        false
    }
    /// Undoes `journal`'s entity changes, after the rest of its transaction rolled back: the
    /// entities it added go out. Its arrivals come back.
    pub(super) fn roll_back_entity_journal(&mut self, journal: EntityJournal) -> HashSet<u32> {
        self.potions.thrown.truncate(journal.potions);
        // One load when nothing joined anywhere since the transaction opened.
        if JoinSeq::latest() >= journal.mark {
            self.discard_joined_since(journal.mark, &journal.arrivals);
        }
        // After the discard: forgetting an aborted entity reads nothing keyed by position.
        for kept in journal.anon {
            self.persist.put_back_anon(kept);
        }
        journal.arrivals
    }
    /// Takes every entity that joined at `mark` or later out of its list, but the `arrivals`,
    /// and forgets what was kept of it.
    fn discard_joined_since(&mut self, mark: JoinSeq, arrivals: &HashSet<u32>) {
        use super::super::entity::{Entity as _, with_entity};
        let added = |id: u32, join: JoinSeq| join >= mark && !arrivals.contains(&id);
        let mut gone = Vec::new();
        for handle in self.entity_handles_rev() {
            let (id, join) = with_entity!(self, handle, e => (e.id(), e.join_seq()));
            if added(id, join) {
                self.discard_entity(handle);
                gone.push(id);
            }
        }
        macro_rules! apart {
            ($($list:expr),* $(,)?) => {$(
                $list.retain(|e| {
                    let out = added(e.id, e.join_seq());
                    if out {
                        gone.push(e.id);
                    }
                    !out
                });
            )*};
        }
        for id in self.ecs.ids_joined_since(mark) {
            if !arrivals.contains(&id) {
                if let Some(e) = self.ecs.entity_of(id) {
                    self.ecs.despawn(e);
                }
                gone.push(id);
            }
        }
        for id in gone.into_iter().filter(|&id| id != 0) {
            self.forget_entity(id);
        }
    }
}
