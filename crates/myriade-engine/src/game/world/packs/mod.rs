//! The server's data packs: the vanilla root they read from, and the reload of their converted files (`/reload`).
//! The client's resource pack selection and reload left with the client.
use super::Game;
use std::path::PathBuf;

impl Game<'_> {
    pub(super) fn vanilla_root(&self) -> PathBuf {
        if self.session.vanilla_root.as_os_str().is_empty() {
            self.options.resources.clone()
        } else {
            self.session.vanilla_root.clone()
        }
    }
}
