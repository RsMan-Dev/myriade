//! Prepare the vanilla function library against the new packs before publishing any data.
use super::{Game, functions::PackStack};
use myriade_engine::Result;
use myriade_engine::datapack::DataStore;

/// Everything one successful reload publishes: the vanilla function library's store.
pub(super) struct PreparedServerReload {
    pub(super) store: Box<DataStore>,
}

/// Loads the vanilla function library from `stack`, logging per-file failures.
fn functions_store(
    stack: &PackStack,
    conditions: &horizon_world::conditions::LoadConditions,
) -> DataStore {
    let packs: Vec<_> = stack.builtin.iter().chain(stack.enabled_packs()).collect();
    let store = DataStore::load_with_conditions(&packs, conditions);
    for (id, e) in &store.errors {
        log::warn!("Horizon: failed to load function {id}: {e}");
    }
    store
}

impl Game<'_> {
    pub(super) fn prepare_server_reload(&self, stack: &PackStack) -> Result<PreparedServerReload> {
        let conditions = horizon_world::conditions::LoadConditions::vanilla();
        let store = functions_store(stack, &conditions);
        conditions.check_errors()?;
        Ok(PreparedServerReload {
            store: Box::new(store),
        })
    }
    pub(super) fn apply_server_reload(&mut self, prepared: PreparedServerReload, stack: &PackStack) {
        self.admit_store(*prepared.store, stack);
        self.structure_templates.clear();
    }
}
