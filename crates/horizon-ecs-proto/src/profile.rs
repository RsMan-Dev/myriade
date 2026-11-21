//! Cost per system of the real schedule (the design notes). A measuring executor replaces the
//! single-threaded one: it runs the same systems in the same order and reads a user-space instruction counter
//! (`perf_event_open`, `instructions:u`) around each of them, and around each `apply_deferred` sync point (the
//! application of the `Commands`). Nothing else changes: the results of a profiled tick are those of a normal tick.
use crate::{cow::*, goals::*, hostile::*};
use bevy_ecs::{
    prelude::*,
    query::{QueryFilter, QueryState},
    schedule::{ApplyDeferred, SystemExecutor, SystemSchedule},
    system::System,
};
use std::{
    any::TypeId,
    sync::{Arc, Mutex},
};

/// A user-space instruction counter of the calling thread.
pub struct Counter(i32);
impl Counter {
    pub fn new() -> Self {
        // `struct perf_event_attr` (128 bytes): type, size, config, ..., flags at byte 40.
        let mut attr = [0u8; 128];
        attr[4..8].copy_from_slice(&128u32.to_ne_bytes());
        attr[8..16].copy_from_slice(&1u64.to_ne_bytes()); // PERF_COUNT_HW_INSTRUCTIONS
        let flags: u64 = (1 << 5) | (1 << 6); // exclude_kernel | exclude_hv; enabled from the start
        attr[40..48].copy_from_slice(&flags.to_ne_bytes());
        // SAFETY: a plain syscall with a valid attribute buffer.
        let fd = unsafe { libc::syscall(libc::SYS_perf_event_open, attr.as_ptr(), 0, -1, -1, 0) } as i32;
        assert!(fd >= 0, "perf_event_open failed (perf_event_paranoid?)");
        Self(fd)
    }
    #[inline]
    pub fn read(&self) -> u64 {
        let mut v = 0u64;
        // SAFETY: reads 8 bytes into `v`.
        unsafe { libc::read(self.0, (&raw mut v).cast(), 8) };
        v
    }
}
impl Default for Counter {
    fn default() -> Self {
        Self::new()
    }
}

/// One row of the table: a system (or a sync point), in schedule order.
#[derive(Clone, Debug, Default)]
pub struct Row {
    pub name: String,
    pub calls: u64,
    /// Instructions of the system itself (its parameters fetched, its body).
    pub run: u64,
    /// Instructions of applying its `Commands` (sync points are rows of their own: `apply_deferred`).
    pub apply: u64,
    /// Entities (or events) it visited, summed over the ticks.
    pub visited: u64,
}
#[derive(Default)]
pub struct Report {
    pub rows: Vec<Row>,
    pub ticks: u64,
    /// The cost of reading the counter twice in a row (subtracted from each measurement).
    pub read_cost: u64,
    /// Instructions of the whole `Schedule::run` calls, counted outside the loop (executor included).
    pub total: u64,
}
pub type Shared = Arc<Mutex<Report>>;

type CountFn = Box<dyn FnMut(&mut World) -> usize + Send + Sync>;
fn filter<F: QueryFilter + 'static>() -> CountFn {
    let mut st: Option<QueryState<(), F>> = None;
    Box::new(move |w| st.get_or_insert_with(|| QueryState::new(w)).iter(w).count())
}
fn all<F: QueryFilter + 'static>() -> CountFn {
    filter::<F>()
}
/// How many entities (or queued events) a system is about to visit; by the last segment of its name. `Changed`
/// filters scan every row of their tables, so they count all the rows.
fn counter_for(name: &str) -> Option<CountFn> {
    let last = name.rsplit("::").next().unwrap_or(name);
    Some(match last {
        "due" => Box::new(|w: &mut World| {
            let t = w.resource::<Clock>().0 + 1;
            w.resource::<Wheel>().due_len(t)
        }),
        "apply_damage" => Box::new(|w: &mut World| w.resource::<DamageQueue>().0.len()),
        "hurt_events" => Box::new(|w: &mut World| w.resource::<Hurts>().0.len()),
        "hurt_by_events" => Box::new(|w: &mut World| w.resource::<HurtBy>().0.len()),
        "block_changes" => Box::new(|w: &mut World| w.resource::<BlockChanges>().0.len()),
        "player_near" => Box::new(|w: &mut World| usize::from(w.resource::<Players>().dirty)),
        "entered_events" => Box::new(|w: &mut World| w.resource::<Entered>().0.len()),
        "cell_moves" => Box::new(|w: &mut World| w.resource::<CellMoves>().0.len()),
        "arbitrate" => Box::new(|w: &mut World| w.resource::<Fired>().0.len() + w.resource::<Requests>().0.len()),
        "recompute" => filter::<With<Recompute>>(),
        "mother_moved" | "reindex" | "push" | "sun_eval" => all::<With<Bounds>>(),
        "tempt_run" => filter::<With<Tempting>>(),
        "breed_run" => filter::<With<Breeding>>(),
        "follow_parent_run" => filter::<With<Trailing>>(),
        "glance_run" => filter::<With<Glancing>>(),
        "scan_run" => filter::<With<Scanning>>(),
        "float_events" => filter::<With<InFluid>>(),
        "float_run" => filter::<With<Floating>>(),
        "look_control" => filter::<With<Looking>>(),
        "target_continue" | "sight_eval" => filter::<With<crate::targeting::Target>>(),
        "melee_run" => filter::<With<Meleeing>>(),
        "fly" => filter::<With<crate::projectile::Flying>>(),
        "bow_run" => filter::<With<crate::behaviours::ranged_bow::Bowing>>(),
        "sun_burn" => filter::<With<Exposed>>(),
        "pathfind" => filter::<With<NeedsPath>>(),
        "follow" | "move_control" => filter::<With<Following>>(),
        "path_ended" => filter::<With<PathEnded>>(),
        "fluid_update" => filter::<With<InFluid>>(),
        "breathing" => filter::<Or<(With<InFluid>, With<AirLow>)>>(),
        "wall_check" => filter::<With<Recheck>>(),
        "travel" => filter::<With<Moving>>(),
        "hazard_tick" => filter::<With<Hazard>>(),
        "wall_damage" => filter::<With<InWall>>(),
        "apply_pushes" => filter::<With<PushIn>>(),
        _ => return None,
    })
}

/// The measuring executor (single-threaded, same order and same sync points as the stock one).
pub struct ProfExec {
    counter: Counter,
    shared: Shared,
    counters: Vec<Option<CountFn>>,
    unapplied: Vec<bool>,
    measuring: bool,
}
impl ProfExec {
    pub fn new(shared: Shared) -> Self {
        let counter = Counter::new();
        let (a, b) = (counter.read(), counter.read());
        shared.lock().unwrap().read_cost = b - a;
        Self { counter, shared, counters: Vec::new(), unapplied: Vec::new(), measuring: true }
    }
}
impl SystemExecutor for ProfExec {
    fn init(&mut self, _: &SystemSchedule) {
        self.unapplied = Vec::new();
        self.counters = Vec::new();
        self.shared.lock().unwrap().rows.clear();
    }
    fn run(&mut self, schedule: &mut SystemSchedule, world: &mut World, _: Option<&bevy_ecs::schedule::FixedBitSet>, _: fn(bevy_ecs::error::BevyError, bevy_ecs::error::ErrorContext)) {
        // SAFETY: only `System` trait methods are called on the systems.
        let systems = unsafe { schedule.systems_mut() };
        if self.counters.is_empty() {
            self.unapplied = vec![false; systems.len()];
            self.counters = systems.iter().map(|s| counter_for(&s.name().to_string())).collect();
        }
        let mut rows = std::mem::take(&mut self.shared.lock().unwrap().rows);
        if rows.is_empty() {
            rows = systems.iter().map(|s| Row { name: s.name().to_string(), ..Row::default() }).collect();
        }
        let cost = self.shared.lock().unwrap().read_cost;
        let apply_id = TypeId::of::<ApplyDeferred>();
        for i in 0..systems.len() {
            let visited = match self.counters[i].as_mut() {
                Some(f) => f(world) as u64,
                None => 0,
            };
            let c0 = self.counter.read();
            if systems[i].system_type() == apply_id {
                for j in 0..self.unapplied.len() {
                    if self.unapplied[j] {
                        systems[j].apply_deferred(world);
                        self.unapplied[j] = false;
                    }
                }
                let c1 = self.counter.read();
                rows[i].apply += (c1 - c0).saturating_sub(cost);
            } else {
                let _ = systems[i].run_without_applying_deferred((), world);
                let c1 = self.counter.read();
                rows[i].run += (c1 - c0).saturating_sub(cost);
                self.unapplied[i] = true;
            }
            rows[i].calls += 1;
            rows[i].visited += visited;
        }
        let c0 = self.counter.read();
        for j in 0..self.unapplied.len() {
            if self.unapplied[j] {
                systems[j].apply_deferred(world);
                self.unapplied[j] = false;
            }
        }
        let c1 = self.counter.read();
        if let Some(last) = rows.last_mut() {
            last.apply += (c1 - c0).saturating_sub(cost);
        }
        let mut r = self.shared.lock().unwrap();
        r.rows = rows;
        r.ticks += 1;
        let _ = self.measuring;
    }
    fn set_apply_final_deferred(&mut self, _: bool) {}
}

/// Switches a herd's schedule to the measuring executor; the rows accumulate in the returned report.
pub fn instrument(herd: &mut Herd) -> Shared {
    let shared: Shared = Arc::default();
    herd.schedule.set_executor(ProfExec::new(shared.clone()));
    shared
}
/// Forgets what was measured so far (the warm-up) and starts counting again.
pub fn reset(shared: &Shared) {
    let mut r = shared.lock().unwrap();
    for row in &mut r.rows {
        row.calls = 0;
        row.run = 0;
        row.apply = 0;
        row.visited = 0;
    }
    r.ticks = 0;
}

#[cfg(test)]
mod tick_forms {
    use super::*;
    use bevy_ecs::system::SystemState;
    #[derive(Component)]
    struct Mark;
    fn sys(q: Query<&crate::cow::Pos, With<Mark>>, mut reqs: ResMut<Requests>, mut cmd: Commands) {
        for p in &q {
            reqs.0.push(Req::Start(Entity::PLACEHOLDER, p.0.x as u8));
            cmd.spawn_empty();
        }
    }
    /// Instructions of one empty call of a module's tick, in three forms (a run by system id, a `SystemState`, a `QueryState` alone):
    /// the cost the `arbitrate` pays per module that runs (and that the schedule paid, with its condition, even for the modules that do not).
    #[test]
    #[ignore = "probe"]
    fn per_call_cost_of_the_tick_forms() {
        let mut w = World::new();
        w.init_resource::<Requests>();
        for _ in 0..200 {
            w.spawn(crate::cow::Pos(horizon_core::geometry::Vec3::ZERO));
        }
        let id = w.register_system(sys);
        let mut st: SystemState<(Query<&crate::cow::Pos, With<Mark>>, ResMut<Requests>, Commands)> = SystemState::new(&mut w);
        let mut qs = w.query_filtered::<&crate::cow::Pos, With<Mark>>();
        let c = Counter::new();
        let n = 20_000u64;
        let t0 = c.read();
        for _ in 0..n {
            let _ = w.run_system(id);
        }
        let t1 = c.read();
        for _ in 0..n {
            let (q, mut reqs, mut cmd) = st.get_mut(&mut w).unwrap();
            for p in &q {
                reqs.0.push(Req::Start(Entity::PLACEHOLDER, p.0.x as u8));
                cmd.spawn_empty();
            }
            st.apply(&mut w);
        }
        let t2 = c.read();
        for _ in 0..n {
            for p in qs.iter(&w) {
                std::hint::black_box(p);
            }
        }
        let t3 = c.read();
        eprintln!("TICKFORMS run_system {} SystemState {} QueryState {}", (t1 - t0) / n, (t2 - t1) / n, (t3 - t2) / n);
    }
}
