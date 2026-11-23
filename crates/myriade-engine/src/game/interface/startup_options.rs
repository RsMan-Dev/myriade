//! The game-owned, side-effect-free startup validation boundary.
use super::*;

impl Options {
    /// The complete game-owned command-line boundary, before leases or native hooks.
    /// A bad command line is a [`launch_api::UsageError`] (exit code 2, no crash report).
    pub(crate) fn validate(args: impl Iterator<Item = String>) -> Result<Option<Self>> {
        let Some(mut o) =
            Self::parse(args).map_err(|e| launch_api::UsageError(e.to_string()))?
        else {
            return Ok(None);
        };
        // Paths given on the command line stay relative to where the game was started.
        if o.game_dir.is_some() {
            o.control = o.control.map(std::path::absolute).transpose()?;
            o.resources = std::path::absolute(&o.resources)?;
        }
        // Refused here, before anything loads.
        if let Some(path) = &o.control {
            myriade_engine::control::check_path(path).map_err(|e| {
                launch_api::UsageError(format!("--control-socket {}: {e}", path.display()))
            })?;
        }
        Ok(Some(o))
    }
}
