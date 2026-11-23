//! Focused production-source tests and timings without the renderer dependency tree.
#[path = "../../../crates/myriade-engine/src/join_seq.rs"]
pub mod join_seq;

// The residency tests read the diagnostics those sources log. `myriade-engine` serves
// `crate::test_log` to the same files (crates/myriade-engine/src/engine_lib.rs):
// the probe imports the production sources, so it carries the same shim.
#[cfg(test)]
mod test_log;
