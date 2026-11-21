//! Agent control as a setting. The control socket drives a window (horizon-ctl), so it is the client's and is not
//! opened by the server: only the setting's value is kept.
use super::Game;
use horizon_client::Result;

impl Game<'_> {
    /// At startup: the setting's value is kept; no control socket is opened.
    pub(super) fn start_agent_control(&mut self) -> Result<()> {
        self.agent_control_on = self.settings.agent_control;
        Ok(())
    }
}
