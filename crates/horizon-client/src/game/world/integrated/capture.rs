//! The server thread's side of a capture (docs/perf-platform.md §4.1): its ticks and phases.
use super::super::dedicated::report::Recording;
use super::super::{Game, tick_profile};
use horizon_client::server_perf::{self, CaptureEnd, Role, ServerPerf, TickDetail};
use horizon_client::spans;
use horizon_perf_format::{files, v1::StopReason};
use std::io;
use std::time::Instant;

/// The server thread's health history and the capture it writes, if any.
pub(in crate::game) struct ServerSide {
    perf: ServerPerf,
    epoch: Instant,
    recording: Option<Recording>,
    /// When the capture's window ends on this thread's clock, and why it stops then.
    window: Option<(f64, StopReason)>,
    spans_were_on: bool,
}

impl ServerSide {
    pub(in crate::game) fn new(slow_ms: f32) -> Self {
        Self {
            perf: ServerPerf::with_role(Role::INTEGRATED, 0.0, slow_ms),
            epoch: Instant::now(),
            recording: None,
            window: None,
            spans_were_on: false,
        }
    }

    fn now(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }

    /// `instant` on this thread's clock, negative when it is before the clock began.
    fn at(&self, instant: Instant) -> f64 {
        match instant.checked_duration_since(self.epoch) {
            Some(after) => after.as_secs_f64(),
            None => -self.epoch.duration_since(instant).as_secs_f64(),
        }
    }

}

impl Game<'_> {
    /// Writes the spans, workers and world and hands the file off to finish; false when no capture runs here.
    fn end_server_capture(&self, side: &mut ServerSide, reason: StopReason) -> bool {
        if !side.perf.capturing() {
            return false;
        }
        // The spans from between ticks are not a tick's, and the price check counts from none.
        spans::take_raw();
        let end = side
            .recording
            .take()
            .map_or_else(CaptureEnd::default, |r| self.capture_end(&r));
        spans::record_here(side.spans_were_on);
        side.window = None;
        let t = side.now();
        side.perf.begin_stop(t, reason, end);
        true
    }

    /// One tick of the server thread. No capture runs on the server: the tick is only timed by `tick_server_timed`.
    pub(super) fn tick_server_measured(&mut self, paused: bool) -> crate::Result<()> {
        self.tick_server_timed(paused)
    }

    /// The server is closing: no capture of its side is open.
    pub(in crate::game) fn finish_server_capture(&mut self) {}
}

/// A capture of the server side finished: logs its file and the oldest ones it removed.
fn server_saved(saved: io::Result<server_perf::Saved>) {
    match saved {
        Ok(saved) => {
            for path in &saved.deleted {
                log::info!("Horizon: removed the oldest capture {}", path.display());
            }
            log::info!("Horizon: {}", files::saved_line(&saved.path));
        }
        Err(e) => log::error!("Horizon: could not write the performance capture: {e}"),
    }
}
