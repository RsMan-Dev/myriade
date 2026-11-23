//! Opening and closing a local world: entering it on the server thread, and saving and leaving
//! it on exit.
use super::*;

impl Game<'_> {
    /// Load authoritative world state synchronously, without client entry or
    /// integrated-server selection: a test-only convenience over the same two
    /// steps production startup calls separately (`prepare_world_and_rules`,
    /// then `finish_open_world`).
    #[cfg(test)]
    pub(super) fn load_world(&mut self, request: world_loading::WorldRequest) -> Result<()> {
        let prepared = self.prepare_world_and_rules(request)?;
        self.finish_open_world(prepared)
    }
}
