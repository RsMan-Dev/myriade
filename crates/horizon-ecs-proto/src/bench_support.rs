//! Support of the benches: `DUMMIES`, modules nobody carries, with the systems a small real module has (one filtered query,
//! one reading an event list), to measure what an inscribed module costs (`goal_bench` with `DUMMY=n`).
use crate::behaviours::Module;
use crate::cow::Pos;
use crate::goals::{Events, GoalSlot};
use bevy_ecs::{prelude::*, schedule::Schedule};

#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct DummyMark;
fn dummy_a(q: Query<(Entity, &Pos), With<DummyMark>>) {
    for _ in &q {}
}
fn dummy_b(ev: Res<Events>, q: Query<Entity, With<DummyMark>>) {
    if !ev.ignited.is_empty() {
        for _ in &q {}
    }
}
fn dummy_systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((dummy_a, dummy_b).in_set(slot));
}
pub static DUMMIES: [Module; 200] = [const { Module { name: "dummy", systems: dummy_systems, ..Module::DEFAULT } }; 200];
/// Inscribes the first `n` dummies in the registry (before the first `registry()` call).
pub fn install_dummies(n: usize) {
    fn add(b: &mut crate::species::Builder) {
        let n: usize = std::env::var("DUMMY").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        for m in DUMMIES.iter().take(n) {
            b.module(m);
        }
    }
    let _ = n;
    let _ = crate::species::install(add);
}

/// The benches count instructions: on a library compiled below `opt-level` 2 (the development profile `ecsdev`) the figures mean nothing.
/// They stop with a clear message; the instructions come from `bilan_check.sh` on the `fast` profile only.
pub fn require_optimized() {
    if cfg!(unoptimized) {
        eprintln!("REFUSED: this bench was built with an unoptimized prototype (development profile). No cost is read on it: build with `--profile fast` (bilan_check.sh does).");
        std::process::exit(3);
    }
}
