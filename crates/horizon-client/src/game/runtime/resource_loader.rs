//! What a start loads before the game takes over: what the background loader
//! publishes (`Loaded`, `LoaderMsg`), a pending world entry, and the smoke
//! run's report (`write_report`).
use super::*;

/// The data the background loader produces for `Game::new`.
pub(super) struct Loaded {
    pub(super) root: PathBuf,
    pub(super) resources: Arc<Resources>,
    pub(super) pack: PackResources,
}

pub(super) enum PendingWorldEntry {
    Local {
        folder: String,
        name: Option<String>,
        seed: Option<i64>,
        creative: bool,
        create: Option<Box<crate::game::world_options::WorldOptions>>,
        /// The save must still exist when the entry opens (a title's Play or a request's Yes): nothing is
        /// created for a folder that has gone.
        existing_only: bool,
    },
    Server {
        address: String,
        name: String,
        lan: bool,
        realm: bool,
        /// A transfer's carried-over server cookies (`TransferState`).
        cookies: Option<horizon_client::net::Cookies>,
    },
}
