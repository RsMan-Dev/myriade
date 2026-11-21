//! A compiled CONFIGURATION handler entered inline by the real LAN connection thread.
use horizon_protocol::packets::configuration as cfg;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const TEST: &str = "game::payload_transport::wasm_config_tests::compiled_guest_runs_on_configuration_connection_thread";
const CHANNEL: &str = "wasm_fixture:configuration";

// Release the real main thread before Game shutdown, including assertion unwinding.
struct Release(mpsc::Sender<()>);
impl Drop for Release {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}
