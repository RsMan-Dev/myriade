//! The parallel mode gives the SEQUENTIAL result, whatever the thread count and from one run to the next (the design notes, G'): the
//! structural commands of the parallel systems are applied sorted by entity (`OrdCmds`) and the pushes are summed. The task pool of a process has
//! one size, so each run is a process of `scale_bench` (cargo builds it for this test): the checksum of the positions at the end of the window.
use std::process::Command;

fn checksum(scene: &str, species: &str, n: usize, threads: usize, players: Option<usize>, budget: Option<usize>) -> String {
    let mut c = Command::new(env!("CARGO_BIN_EXE_scale_bench"));
    c.args([scene, &n.to_string(), "250", "20", &threads.to_string()]).env("HORIZON_SPECIES", species).env("NO_XP", "1");
    if let Some(p) = players {
        c.env("PLAYERS", p.to_string());
    }
    if let Some(b) = budget {
        c.env("PATH_BUDGET", b.to_string());
    }
    let out = c.output().expect("scale_bench runs");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    text.split("checksum=").nth(1).map(|s| s.trim().to_string()).unwrap_or_else(|| panic!("no checksum: {text} {}", String::from_utf8_lossy(&out.stderr)))
}

/// Sequential, 1, 4 and 8 threads, and 4 threads twice: one single checksum.
fn same_everywhere(scene: &str, species: &str, n: usize, players: Option<usize>, budget: Option<usize>) -> String {
    let runs: Vec<(usize, String)> = std::thread::scope(|s| {
        let handles: Vec<_> = [0usize, 1, 4, 8, 4].into_iter().map(|t| s.spawn(move || (t, checksum(scene, species, n, t, players, budget)))).collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let first = &runs[0].1;
    for (t, c) in &runs {
        assert_eq!(c, first, "{scene} ({n}): {t} threads gave {c}, the sequential run {first}; all: {runs:?}");
    }
    first.clone()
}

#[test]
fn crowded_cows_parallel_equals_sequential() {
    same_everywhere("crowd", "minecraft:cow", 1000, None, None);
}
#[test]
fn solicited_cows_parallel_equals_sequential() {
    same_everywhere("solicit", "minecraft:cow", 1000, None, None);
}
#[test]
fn chasing_zombies_parallel_equals_sequential() {
    same_everywhere("chase", "minecraft:zombie", 1000, Some(4), None);
}
/// The per-tick path budget: the same game for every thread count, and a budget that really limits (its checksum differs from the unlimited one).
#[test]
fn path_budget_parallel_equals_sequential() {
    let limited = same_everywhere("chase", "minecraft:zombie", 1000, Some(4), Some(3));
    let unlimited = same_everywhere("chase", "minecraft:zombie", 1000, Some(4), None);
    assert_ne!(limited, unlimited, "a budget of 3 searches a tick changed nothing: it does not bite");
}
