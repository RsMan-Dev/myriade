//! The mods' spawn hooks of the natural pass: a refused placement or a cancelled finalization gives no birth, a forced placement gives births
//! where vanilla's predicate refused, and answers that keep vanilla's answers leave the pass's draws as they were (same births, per category).
use horizon_core::fast_hash::FxHashSet;
use horizon_ecs_proto::cow::Herd;
use horizon_ecs_proto::spawn_env::SpawnEnv;
use horizon_ecs_proto::spawn_finalize::Group;
use horizon_ecs_proto::spawn_mod_host::{LentSpawnHost, SpawnModCalls, SpawnModHost, Verdict};
use horizon_ecs_proto::spawn_reason::SpawnReason;
use horizon_ecs_proto::spawning::Spawner;
use std::cell::Cell;

const TICKS: usize = 800;
const PLAYER: (f64, f64) = (8.0, 8.0);

fn have() -> bool {
    horizon_ecs_proto::species::inscribe_every_module();
    std::env::var_os("HORIZON_TEST_RESOURCES").is_some()
}

/// Every chunk within `r` of the chunk 0,0.
fn chunks(r: i32) -> FxHashSet<(i32, i32)> {
    (-r..=r).flat_map(|dx| (-r..=r).map(move |dz| (dx, dz))).collect()
}

/// The slab of `refusal.rs`'s gates with the natural spawning on, one player at `PLAYER`, and the world spawn far away.
fn herd() -> Herd {
    let mut h = Herd::with_spawn_world_awake(176, true);
    h.enable_spawning("minecraft:plains", 7);
    h.set_online(&[PLAYER]);
    h.world.insert_resource(SpawnEnv { spawn_mobs: true, monsters: true, spawnable: chunks(10), border: None, world_spawn: [-200, 1, -200], changed: false });
    h
}

/// The answers of a mod, with how often it was asked to refuse a start vanilla's predicate had refused.
struct Answers {
    placement: fn(bool) -> bool,
    position: Option<bool>,
    verdict: Verdict,
    refused: Cell<u64>,
}
impl SpawnModHost for Answers {
    fn placement_check(&self, _: &str, _: [i32; 3], _: SpawnReason, default: bool) -> bool {
        if !default {
            self.refused.set(self.refused.get() + 1);
        }
        (self.placement)(default)
    }
    fn position_check(&self, _: &str, _: [f64; 3], _: SpawnReason) -> Option<bool> {
        self.position
    }
    fn finalize_spawn(&self, _: &str, _: [f64; 3], _: SpawnReason, _: &mut Option<Group>) -> Verdict {
        self.verdict
    }
}

/// The creatures born by the pass over the ticks, by category, with `host` lent (None: no mod listens).
fn births_with(host: Option<&dyn SpawnModHost>) -> [[u64; 3]; 8] {
    let mut h = herd();
    h.world.insert_resource(SpawnModCalls(host.map(LentSpawnHost::lend)));
    for _ in 0..TICKS {
        h.tick();
    }
    h.world.insert_resource(SpawnModCalls(None));
    h.world.resource::<Spawner>().stats.by_category
}

fn born(c: [[u64; 3]; 8]) -> u64 {
    c.iter().map(|k| k[2]).sum()
}

#[test]
fn a_refused_placement_gives_no_birth() {
    if !have() {
        return;
    }
    let refuse = Answers { placement: |_| false, position: None, verdict: Verdict::Done(None), refused: Cell::new(0) };
    assert_eq!(born(births_with(Some(&refuse))), 0);
}

#[test]
fn a_cancelled_finalization_gives_no_birth() {
    if !have() {
        return;
    }
    let cancel = Answers { placement: |d| d, position: None, verdict: Verdict::Cancelled, refused: Cell::new(0) };
    assert_eq!(born(births_with(Some(&cancel))), 0);
}

#[test]
fn a_forced_placement_gives_births_where_vanilla_refused() {
    if !have() {
        return;
    }
    let force = Answers { placement: |_| true, position: Some(true), verdict: Verdict::Done(None), refused: Cell::new(0) };
    assert!(born(births_with(Some(&force))) > 0);
    assert!(force.refused.get() > 0);
}

#[test]
fn vanilla_answers_keep_the_draws_of_the_pass() {
    if !have() {
        return;
    }
    let keep = Answers { placement: |d| d, position: None, verdict: Verdict::Done(None), refused: Cell::new(0) };
    assert_eq!(births_with(Some(&keep)), births_with(None));
}
