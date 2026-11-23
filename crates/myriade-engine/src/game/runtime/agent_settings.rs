//! The control endpoint. A window's control socket is the client's and is not opened here; the
//! server opens the one `horizon-server` asks for (`--control-socket`), answered in `dedicated/status.rs`.
use super::Game;
use myriade_engine::Result;

impl Game<'_> {
    /// At startup: opens the endpoint the command line asked for; failing to listen is an error.
    pub(super) fn start_agent_control(&mut self) -> Result<()> {
        if let Some(path) = &self.options.control {
            let server = myriade_engine::control::Server::start(path)
                .map_err(|e| format!("control socket {}: {e}", path.display()))?;
            log::info!("Horizon: control socket {}", server.path().display());
            self.control = Some(server);
        }
        Ok(())
    }
}
