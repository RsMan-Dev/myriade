//! What a start loads before the game takes over: what the background loader
//! publishes (`Loaded`, `LoaderMsg`), a pending world entry, and the smoke
//! run's report (`write_report`).
use super::*;

/// The data the background loader produces for `Game::new`.
pub(super) struct Loaded {
    pub(super) root: PathBuf,
    pub(super) pack_error: Option<String>,
    pub(super) resources: Arc<Resources>,
    pub(super) pack: PackResources,
}

/// What the background loader publishes.
pub(super) enum LoaderMsg {
    Done(std::result::Result<Loaded, String>),
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

pub(super) fn write_report(game: &Game<'_>) -> Result<()> {
    if let Some(path) = &game.options.report {
        let report = serde_json::json!({
            "frames": game.frames,
            "elapsed_seconds": game.started.elapsed().as_secs_f64(),
            "chunks": game.world.chunks.len(),
            "generated": game.streaming.generated_total,
            "player": game.player.as_ref().map(|p| [p.position.x, p.position.y, p.position.z]),
            "error": game.error.as_ref().map(|e| e.to_string()),
        });
        std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    }
    Ok(())
}
