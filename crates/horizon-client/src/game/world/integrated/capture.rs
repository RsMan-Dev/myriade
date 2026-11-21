//! The server thread's side of a capture (docs/perf-platform.md §4.1): it writes into the window's file, or into one of its own.
use super::super::dedicated::{SLOW_TICK_MS, report::Recording};
use super::super::perf_remote::Done;
use super::super::profile_capture::{reply_text, shown};
use super::super::{Game, tick_profile};
use horizon_client::client_perf::{
    Join,
    request::{Failed, ProfileSide, StartOptions, Stopped, Threads},
};
use horizon_client::server_perf::{
    self, CaptureEnd, CaptureOptions, MAX_WINDOW_S, Role, SamplerOptions, ServerPerf, TickDetail,
};
use horizon_client::spans;
use horizon_perf_format::{
    files,
    v1::{GpuMode, Start, StopReason, Trigger},
};
use std::io;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
    mpsc::Sender,
};
use std::time::Instant;

/// How far the server thread has got with the side of a capture the window asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::game) enum Phase {
    /// Asked for, not yet picked up between two ticks.
    Asked,
    Running,
    Ended,
    /// Refused or failed to begin; nothing of this side is in the file.
    Failed,
}

/// A [`Phase`] the window and the server thread share.
pub(in crate::game) struct Live(AtomicU8);

impl Live {
    pub(in crate::game) fn new() -> Arc<Self> {
        Arc::new(Self(AtomicU8::new(Phase::Asked as u8)))
    }

    pub(in crate::game) fn get(&self) -> Phase {
        match self.0.load(Ordering::Relaxed) {
            0 => Phase::Asked,
            1 => Phase::Running,
            2 => Phase::Ended,
            _ => Phase::Failed,
        }
    }

    pub(in crate::game) fn set(&self, phase: Phase) {
        self.0.store(phase as u8, Ordering::Relaxed);
    }

    /// Whether this side is, or is about to be, writing.
    pub(in crate::game) fn active(&self) -> bool {
        matches!(self.get(), Phase::Asked | Phase::Running)
    }

    /// Whether the file holds, or will hold, this side.
    pub(in crate::game) fn wrote(&self) -> bool {
        self.get() != Phase::Failed
    }
}

/// Where a server capture is written.
pub(in crate::game) enum Target {
    /// The window's file, which the window finishes.
    Join(Join),
    /// A file of its own, as a server-only capture needs.
    Own {
        options: StartOptions,
        trigger: Trigger,
        dir: PathBuf,
    },
}

/// The server thread's health history and the capture it writes, if any.
pub(in crate::game) struct ServerSide {
    perf: ServerPerf,
    epoch: Instant,
    recording: Option<Recording>,
    /// When the capture's window ends on this thread's clock, and why it stops then.
    window: Option<(f64, StopReason)>,
    /// Where this side is, for the window to read.
    live: Option<Arc<Live>>,
    /// Whether the file is this side's to finish.
    owner: bool,
    /// The `profile stop` waiting for the file, on an owner.
    reply: Option<Sender<String>>,
    spans_were_on: bool,
}

impl ServerSide {
    pub(in crate::game) fn new(slow_ms: f32) -> Self {
        Self {
            perf: ServerPerf::with_role(Role::INTEGRATED, 0.0, slow_ms),
            epoch: Instant::now(),
            recording: None,
            window: None,
            live: None,
            owner: false,
            reply: None,
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

    fn ended(&self) {
        if let Some(live) = &self.live {
            live.set(Phase::Ended);
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
        if !side.owner {
            side.ended();
        }
        true
    }



    /// One tick of the server thread, timed for the health history and the capture.
    pub(super) fn tick_server_measured(&mut self, paused: bool) -> crate::Result<()> {
        self.serve_perf_asked();
        let Some(mut side) = self.profile.server.take() else {
            return self.tick_server_timed(paused);
        };
        let slow_ms = side.perf.slow_threshold_ms();
        let capturing = side.perf.capturing();
        let sampled = side.recording.as_ref().is_some_and(Recording::sample_next);
        tick_profile::set_sampling(sampled);
        let mut detail = None;
        let started = Instant::now();
        let result = self.tick_server_spans(paused, |timing| {
            if capturing || timing.wall_ms > slow_ms {
                detail = Some(TickDetail {
                    cpu_ms: timing.cpu_ms,
                    spans: timing.raw,
                });
            }
        });
        let wall_ms = started.elapsed().as_secs_f32() * 1000.0;
        tick_profile::set_sampling(false);
        if let Some(recording) = &mut side.recording {
            recording.add(sampled);
        }
        let t = side.now();
        side.perf.tick(
            t,
            wall_ms,
            detail,
            || self.perf_world_counts(),
            || self.wire_bytes(),
        );
        if let Some((end, reason)) = side.window
            && t >= end
        {
            self.end_server_capture(&mut side, reason);
        }
        let done: Vec<Done> = side
            .perf
            .take_saved(false)
            .into_iter()
            .filter_map(|saved| server_saved(&mut side, saved))
            .collect();
        self.profile.server = Some(side);
        self.perf_captures_done(done);
        result
    }

    /// The server is closing: ends the capture, and waits for the file when it is this side's.
    pub(in crate::game) fn finish_server_capture(&mut self) {
        let Some(mut side) = self.profile.server.take() else {
            return;
        };
        self.end_server_capture(&mut side, StopReason::Shutdown);
        let mut done = Vec::new();
        if side.owner {
            for saved in side.perf.take_saved(true) {
                done.extend(server_saved(&mut side, saved));
            }
        }
        side.ended();
        drop(side);
        self.perf_captures_done(done);
    }
}

/// A capture of this side finished: an owner logs the file and tells the window that waits for it.
fn server_saved(side: &mut ServerSide, saved: io::Result<server_perf::Saved>) -> Option<Done> {
    if !side.owner {
        if let Err(e) = saved {
            log::error!("Horizon: could not write the server side of the capture: {e}");
        }
        return None;
    }
    side.ended();
    let reply = side.reply.take();
    match saved {
        Ok(saved) => {
            for path in &saved.deleted {
                log::info!("Horizon: removed the oldest capture {}", path.display());
            }
            log::info!("Horizon: {}", files::saved_line(&saved.path));
            let removed: Vec<String> = saved.deleted.iter().map(|p| shown(p)).collect();
            if let Some(reply) = reply {
                let _ = reply.send(reply_text(&Stopped {
                    ok: true,
                    sides: vec![ProfileSide::Server],
                    path: shown(&saved.path),
                    open_at: horizon_client::client_perf::request::viewer(),
                    removed: removed.clone(),
                }));
            }
            Some(Done::Saved {
                path: shown(&saved.path),
                file: saved.path.clone(),
                removed,
            })
        }
        Err(e) => {
            log::error!("Horizon: could not write the performance capture: {e}");
            if let Some(reply) = reply {
                let _ = reply.send(reply_text(&Failed {
                    error: e.to_string(),
                }));
            }
            Some(Done::Failed)
        }
    }
}
