//! What a start loads before the game takes over: what the background loader publishes.
use super::*;

/// The data the background loader produces for `Game::new`.
pub(super) struct Loaded {
    pub(super) root: PathBuf,
    pub(super) resources: Arc<Resources>,
    pub(super) pack: PackResources,
}
