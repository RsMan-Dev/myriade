//! `horizon-report [SECONDS]` and `horizon-trace [SECONDS]` on the dedicated server's console, and `HORIZON_TRACE`: record a window of ticks into `reports/profile-<time>Z.hzperf`; the words they take live in `horizon_gameplay::dedicated::report`.
use super::super::memory::{MEMORY_EVERY_S, flatten_report};
use super::super::{Dim, Game, tick_profile};
use myriade_engine::server_perf::{
    self, CaptureEnd, CaptureOptions, SamplerOptions, ServerPerf, WorldCounts,
    traffic::{Conn, Wire},
    world::{LevelCounts, MobCost, MobCosts, WorkerJobs},
};
use myriade_engine::{spans, streaming::job_stats};
pub(in crate::game) use horizon_gameplay::dedicated::report::LOOSE_ENTITIES;
pub(super) use horizon_gameplay::dedicated::report::{Command, MAX_SECONDS, parse, parse_trace};
use horizon_perf_format::{
    files,
    v1::{HorizonOptions, Start, StopReason, Trigger},
};
use horizon_world::level::BlockId;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// What a capture is asked to record, and how it was asked.
pub(super) struct Plan {
    pub seconds: u32,
    pub samples: bool,
    pub timeline: bool,
    /// Where the capture is written when it is not `reports/`.
    pub file: Option<PathBuf>,
    pub trigger: Trigger,
    pub sampler: SamplerOptions,
}

/// Logs the captures whose files are done; with `wait`, every one still being finished.
pub(in crate::game) fn log_saved(perf: &mut ServerPerf, wait: bool) {
    for saved in perf.take_saved(wait) {
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
}

/// A report being recorded: what the game side needs while the capture runs.
pub(in crate::game) struct Recording {
    seconds: u32,
    /// Whether it turned the spans of every thread on, which it turns off again.
    all_threads: bool,
    started: Instant,
    ticks: u64,
    /// The ticks whose mobs were timed one by one ([`tick_profile::SAMPLE_EVERY`]).
    sampled_ticks: u64,
}

impl Recording {
    pub(super) fn start(seconds: u32) -> Self {
        tick_profile::want_spans(tick_profile::SPANS_REPORT, true);
        Self::new(seconds, true)
    }
    fn new(seconds: u32, all_threads: bool) -> Self {
        job_stats::enable(true);
        job_stats::take();
        tick_profile::take();
        Self {
            seconds,
            all_threads,
            started: Instant::now(),
            ticks: 0,
            sampled_ticks: 0,
        }
    }
    /// Whether the next tick's mobs are timed one by one.
    pub(in crate::game) fn sample_next(&self) -> bool {
        self.ticks.is_multiple_of(tick_profile::SAMPLE_EVERY as u64)
    }
    pub(in crate::game) fn add(&mut self, sampled: bool) {
        self.ticks += 1;
        self.sampled_ticks += u64::from(sampled);
    }
    pub(super) fn done(&self) -> bool {
        self.started.elapsed() >= Duration::from_secs(u64::from(self.seconds))
    }
    /// `horizon-report status`: how far the recording is.
    pub(super) fn status(&self) -> String {
        format!(
            "Recording a performance report: {} of {} s, {} ticks (horizon-report stop writes it now)",
            self.started.elapsed().as_secs(),
            self.seconds,
            self.ticks
        )
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        if self.all_threads {
            tick_profile::want_spans(tick_profile::SPANS_REPORT, false);
        }
        job_stats::enable(false);
    }
}

/// What one span costs on this thread with spans on: the report's own overhead. The spans it
/// records are thrown away.
fn span_cost_nanos() -> f64 {
    const N: u32 = 5_000;
    let kept = spans::take_raw();
    let timer = Instant::now();
    for _ in 0..N {
        drop(spans::guard("calibration"));
    }
    let per = timer.elapsed().as_nanos() as f64 / f64::from(N);
    spans::take_raw();
    debug_assert!(kept.is_empty(), "the tick takes its spans");
    per
}

impl Game<'_> {
    /// Starts a report: the recording the game side needs and the capture beside it.
    pub(super) fn record_report(
        &self,
        state: &mut super::ServerState,
        plan: &Plan,
    ) -> Option<PathBuf> {
        state.report = Some(Recording::start(plan.seconds));
        self.start_perf_capture(state, plan)
    }

    /// Opens the `.hzperf` that records beside a report, and logs where it goes.
    pub(super) fn start_perf_capture(
        &self,
        state: &mut super::ServerState,
        plan: &Plan,
    ) -> Option<PathBuf> {
        let options = self.capture_options(
            plan,
            (state.view, state.max_players, state.compression),
            state.horizon,
        );
        let t = state.perf_epoch.elapsed().as_secs_f64();
        match state.perf.start_capture(t, Path::new("reports"), options) {
            Ok(path) => {
                log::info!("Horizon: capturing to {}", path.display());
                Some(path)
            }
            Err(e) => {
                log::error!("Horizon: could not start the performance capture: {e}");
                None
            }
        }
    }

    /// Every block's name by the id the sampler's context holds.
    pub(in crate::game) fn block_names(&self) -> Vec<String> {
        let table = &self.res.behaviours().table;
        (0..table.block_count() as u16)
            .map(|id| table.info(BlockId(id)).name.to_string())
            .collect()
    }

    /// What a capture of `plan` records of the server as it starts: the typed allowlists, the world and the connections.
    fn capture_options(
        &self,
        plan: &Plan,
        (view, max_players, compression): ((u32, u32), usize, i32),
        horizon: HorizonOptions,
    ) -> CaptureOptions {
        let start = Start {
            build: Some(server_perf::build()),
            machine: Some(server_perf::machine(server_perf::hardware())),
            settings: Some(server_perf::settings(
                view.0,
                view.1,
                max_players as u32,
                compression,
            )),
            mods: Vec::new(),
            game_rules: Some(server_perf::game_rules(&self.session.rules)),
            horizon_options: Some(horizon),
            players: self.lan.as_ref().map_or(0, |h| {
                h.hub
                    .guests
                    .iter()
                    .filter(|g| g.participates_online())
                    .count() as u32
            }),
            ..Start::default()
        };
        CaptureOptions {
            window_s: Some(f64::from(plan.seconds)),
            trigger: plan.trigger,
            start,
            samples: plan.samples,
            sampler: plan.sampler,
            block_names: self.block_names(),
            levels: self.level_counts(),
            wire: self.wire_bytes(),
            timeline: plan.timeline.then(Default::default),
            file: plan.file.clone(),
        }
    }

    fn active_perf_view(&self) -> super::super::levels::EntityCountView {
        super::super::levels::entity_count_view!(self)
    }

    /// All typed entities in the active dimension, without allocating.
    pub(in crate::game) fn perf_entity_count(&self) -> usize {
        self.active_perf_view().total()
    }

    /// Health samples count every typed family using lengths only.
    pub(in crate::game) fn perf_world_counts(&self) -> WorldCounts {
        let (mut chunks, mut entities) = (self.world.chunks.len(), self.perf_entity_count());
        for (_, world, view, _) in self.levels.parked_perf_views() {
            chunks += world.chunks.len();
            entities += view.total();
        }
        WorldCounts {
            players: self.lan.as_ref().map_or(0, |h| {
                h.hub
                    .guests
                    .iter()
                    .filter(|g| g.participates_online())
                    .count() as u32
            }),
            chunks: chunks as u32,
            entities: entities as u32,
        }
    }

    /// Capture snapshots resolve registry labels; gauges use the same view without labels.
    pub(in crate::game) fn level_counts(&self) -> Vec<LevelCounts> {
        let level = |dim: Dim,
                     world: &myriade_engine::world::World,
                     view: super::super::levels::EntityCountView,
                     ecs_kinds: &[(&'static str, usize)]| {
            let mut block_entities = BTreeMap::<String, u32>::new();
            for entity in world.block_entities.values() {
                *block_entities.entry(entity.id().to_owned()).or_default() += 1;
            }
            LevelCounts {
                dimension: dim.id().to_owned(),
                chunks: world.chunks.len() as u32,
                entities: view.registry_counts(ecs_kinds),
                block_entities: block_entities.into_iter().collect(),
            }
        };
        std::iter::once(level(self.dim, &self.world, self.active_perf_view(), &self.ecs.count_by_kind()))
            .chain(
                self.levels
                    .parked_perf_views()
                    .map(|(dim, world, view, kinds)| level(dim, world, view, &kinds)),
            )
            .collect()
    }

    /// Each player's connection: wire bytes (in, out), name, brand and view distance.
    pub(in crate::game) fn wire_bytes(&self) -> Wire {
        self.lan.as_ref().map_or_else(Vec::new, |h| {
            h.hub
                .guests
                .iter()
                .filter_map(|g| {
                    Some(Conn {
                        id: g.conn,
                        bytes: g.sender.wire_bytes()?,
                        name: g.name.clone(),
                        brand: g.brand.clone(),
                        view_distance: g.view_distance.max(0) as u32,
                    })
                })
                .collect()
        })
    }

    fn take_server_memory_snapshot(&self, state: &mut super::ServerState, label: &str) {
        let rows = flatten_report(&self.memory_value());
        let t = state.perf_epoch.elapsed().as_secs_f64();
        state.perf.memory_snapshot(t, label, &rows);
    }

    /// A capture's memory snapshots: one as it starts, then every `MEMORY_EVERY_S`.
    pub(super) fn poll_server_memory_snapshot(&self, state: &mut super::ServerState) {
        if !state.perf.capturing() {
            state.memory_due = None;
            return;
        }
        let now = state.perf_epoch.elapsed().as_secs_f64();
        match state.memory_due {
            None => {
                self.take_server_memory_snapshot(state, "start");
                state.memory_due = Some((now + MEMORY_EVERY_S, 0));
            }
            Some((due, taken)) if now >= due => {
                let taken = taken + 1;
                let label = format!("{}s", u64::from(taken) * MEMORY_EVERY_S as u64);
                self.take_server_memory_snapshot(state, &label);
                state.memory_due = Some((due + MEMORY_EVERY_S, taken));
            }
            Some(_) => {}
        }
    }

    /// Ends the capture and the recording beside it, if they run; `log_saved` reports the file when it is done.
    pub(super) fn finish_perf_capture(&self, state: &mut super::ServerState, reason: StopReason) {
        let recording = state.report.take();
        if !state.perf.capturing() {
            return;
        }
        self.take_server_memory_snapshot(state, "stop");
        let end = recording.map_or_else(CaptureEnd::default, |r| self.capture_end(&r));
        let t = state.perf_epoch.elapsed().as_secs_f64();
        state.perf.begin_stop(t, reason, end);
    }

    /// What a finished recording leaves for the capture: the world as it stands, the sampled mob costs and the workers' time.
    pub(in crate::game) fn capture_end(&self, recording: &Recording) -> CaptureEnd {
        let span_ns = span_cost_nanos();
        let mob_ns = tick_profile::sample_cost_nanos();
        // Release this recording's span request; other recordings or HORIZON_SPIKES may remain.
        if recording.all_threads {
            tick_profile::want_spans(tick_profile::SPANS_REPORT, false);
        }
        job_stats::enable(false);
        CaptureEnd {
            levels: self.level_counts(),
            mob_costs: MobCosts {
                costs: tick_profile::take()
                    .into_iter()
                    .map(|c| MobCost {
                        dimension: c.dimension.to_owned(),
                        kind: c.kind,
                        nanos: c.nanos,
                        mob_ticks: c.mob_ticks,
                    })
                    .collect(),
                sampled_ticks: recording.sampled_ticks,
            },
            jobs: job_stats::take()
                .into_iter()
                .map(|(kind, nanos, count)| WorkerJobs {
                    kind: kind.to_owned(),
                    nanos,
                    count,
                })
                .collect(),
            wire: self.wire_bytes(),
            span_ns,
            mob_ns,
            bench: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One tick in four has its mobs timed one by one, starting with the first.
    #[test]
    fn one_tick_in_four_is_sampled() {
        let mut r = Recording {
            seconds: 1,
            all_threads: false,
            started: Instant::now(),
            ticks: 0,
            sampled_ticks: 0,
        };
        for _ in 0..20 {
            let sampled = r.sample_next();
            r.add(sampled);
        }
        assert_eq!((r.ticks, r.sampled_ticks), (20, 5));
    }

    /// A recording that ends without `capture_end` (the capture failed to start, or shutdown) leaves spans off.
    #[test]
    fn a_dropped_recording_turns_spans_off() {
        drop(Recording::start(1));
        assert!(!tick_profile::wanted(tick_profile::SPANS_REPORT));
    }

    /// On this machine, timing a span and a mob each cost well under a millisecond.
    #[test]
    fn the_prices_of_a_span_and_a_mob_sample_are_small() {
        let (span_ns, mob_ns) = (span_cost_nanos(), tick_profile::sample_cost_nanos());
        assert!(span_ns > 0.0 && span_ns < 50_000.0, "{span_ns}");
        assert!(mob_ns > 0.0 && mob_ns < 50_000.0, "{mob_ns}");
    }
}
