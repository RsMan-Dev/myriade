//! The ECS trial spawner (`spawn_trial`) on its own: the states, the omen, the reward ejection and the cooldown move as vanilla's
//! `TrialSpawnerState` does (`tick_and_get_next`). No species passes `valid`, so no mob is born: the wave's numbers are the ones read.
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;
use horizon_ecs_proto::cow::{Clock, Herd, Level, Species};
use horizon_ecs_proto::goals::{Events, Online, Player};
use horizon_ecs_proto::spawn_spawner::ensure;
use horizon_ecs_proto::spawn_trial::{self as tr, Cfg, Env, Omen, PlayerState, State, Wave};
use std::sync::atomic::{AtomicUsize, Ordering};

const POS: [i32; 3] = [0, 64, 0];
/// The player that carries a Bad Omen in the omen test.
const OMENED: u32 = 7;

static LOOTS: AtomicUsize = AtomicUsize::new(0);
static OMENS: AtomicUsize = AtomicUsize::new(0);

fn player_state(_: &World, id: u32) -> PlayerState {
    PlayerState { creative: false, spectator: false, alive: true, omen: (id == OMENED).then_some(Omen::Bad) }
}
fn no_block(_: &World, _: [i32; 3]) -> bool {
    false
}
fn never_valid(_: &World, _: &Species, _: [f64; 3]) -> bool {
    false
}
/// Counts the ejections only: the ominous item roll asks for its own (empty) table, and the tests run in parallel.
fn loot(_: &mut World, table: &str, _: i64) -> Vec<ItemStack> {
    if table == EJECT {
        LOOTS.fetch_add(1, Ordering::SeqCst);
    }
    Vec::new()
}
fn omened(_: &mut World, _: u32) {
    OMENS.fetch_add(1, Ordering::SeqCst);
}

fn env() -> Env {
    Env { enabled: true, seed: 0, player: player_state, solid: no_block, valid: never_valid, loot, trial_omen: omened }
}

/// The reward table of the ejections.
const EJECT: &str = "test:trial_reward";

/// Potentials for one species id, so the spawner has a wave, and a reward table; the wave's totals are set by each test.
fn cfg(total: f32, cooldown: u64) -> Cfg {
    let wave = Wave { total, potentials: vec![(0, 1)], eject: vec![(EJECT.to_string(), 1)], ..Wave::default() };
    Cfg { cooldown, normal: wave.clone(), ominous: wave, ..Cfg::default() }
}

fn world() -> World {
    let mut w = World::new();
    // The level of a harness world: a birth finalizes through it (`born_finalized`). The herd is leaked, its level lives on with it.
    let herd: &'static Herd = Box::leak(Box::new(Herd::with_slab_awake(4, true, false)));
    w.insert_resource(*herd.world.resource::<Level>());
    w.insert_resource(Clock::default());
    w.insert_resource(Online::default());
    w.insert_resource(Events::default());
    ensure(&mut w, 1);
    w
}

fn at(w: &mut World, now: u64) {
    w.resource_mut::<Clock>().0 = now;
}

fn put(w: &mut World, id: u32, x: f64) {
    w.resource_mut::<Online>().set(Player { id, x, y: 64.0, z: 3.5, main: 0, off: 0, ..Default::default() });
}

/// Runs the ticks `from..=to` (the wake-ups are run as the caller's loop does: one call per tick).
fn run(w: &mut World, e: &Env, from: u64, to: u64) {
    for now in from..=to {
        at(w, now);
        tr::run_wakes(w, e);
    }
}

#[test]
fn waits_for_a_player_then_goes_active_in_its_reach() {
    let mut w = world();
    tr::place(&mut w, POS, cfg(6.0, 36_000));
    run(&mut w, &env(), 1, 60);
    assert_eq!(tr::info(&w, POS).map(|i| i.state), Some(State::WaitingForPlayers));
    put(&mut w, 1, 0.5);
    let mut seen = None;
    for now in 61..=100 {
        at(&mut w, now);
        tr::run_wakes(&mut w, &env());
        if tr::info(&w, POS).is_some_and(|i| i.state == State::Active) {
            seen = Some(now);
            break;
        }
    }
    let now = seen.expect("the spawner goes active within a detection period (20 ticks)");
    assert!(now - 60 <= 20, "detected at tick {now}, a detection period is 20 ticks");
    assert_eq!(tr::info(&w, POS).map(|i| i.players), Some(1));
}

#[test]
fn an_empty_wave_ejects_once_per_thirty_ticks_then_cools_down() {
    let mut w = world();
    tr::place(&mut w, POS, cfg(0.0, 200));
    put(&mut w, 1, 0.5);
    let e = env();
    let ejecting = (1..=200).find(|&now| {
        at(&mut w, now);
        tr::run_wakes(&mut w, &e);
        tr::info(&w, POS).is_some_and(|i| i.state == State::WaitingForRewardEjection)
    });
    let start = ejecting.expect("with no total to reach, the spawner waits for its ejection");
    let info = tr::info(&w, POS).expect("placed");
    assert_eq!(info.cooldown_ends, start + 200, "the cooldown counts from the start of the ejection wait");
    assert_eq!(LOOTS.load(Ordering::SeqCst), 0);

    run(&mut w, &e, start + 1, start + 39);
    assert_eq!(tr::info(&w, POS).map(|i| i.state), Some(State::WaitingForRewardEjection));
    run(&mut w, &e, start + 40, start + 59);
    assert_eq!(tr::info(&w, POS).map(|i| i.state), Some(State::EjectingReward));
    assert_eq!(LOOTS.load(Ordering::SeqCst), 0, "no loot before the first ejection slot");

    run(&mut w, &e, start + 60, start + 60);
    assert_eq!(LOOTS.load(Ordering::SeqCst), 1, "the first slot (40 ticks in, every 30) ejects the loot");
    assert_eq!(tr::info(&w, POS).map(|i| i.players), Some(0), "the detected player leaves the wave");

    // The player is out of reach: the next slot finds nobody and the spawner cools down.
    put(&mut w, 1, 100.0);
    run(&mut w, &e, start + 61, start + 90);
    assert_eq!(tr::info(&w, POS).map(|i| i.state), Some(State::Cooldown));
    run(&mut w, &e, start + 91, start + 199);
    assert_eq!(tr::info(&w, POS).map(|i| i.state), Some(State::Cooldown), "still cooling down");
    run(&mut w, &e, start + 200, start + 200);
    assert_eq!(tr::info(&w, POS).map(|i| i.state), Some(State::WaitingForPlayers), "the cooldown is over");
}

#[test]
fn a_bad_omen_in_sight_makes_the_spawner_ominous_and_calls_the_omen_once() {
    let mut w = world();
    tr::place(&mut w, POS, cfg(6.0, 36_000));
    run(&mut w, &env(), 1, 60);
    let before = OMENS.load(Ordering::SeqCst);
    put(&mut w, OMENED, 0.5);
    let e = env();
    let turned = (61..=100).find(|&now| {
        at(&mut w, now);
        tr::run_wakes(&mut w, &e);
        tr::info(&w, POS).is_some_and(|i| i.ominous)
    });
    let now = turned.expect("the omened player turns the spawner ominous within a detection period");
    let info = tr::info(&w, POS).expect("placed");
    assert_eq!(info.state, State::Active, "the omen starts the wave");
    assert_eq!(info.cooldown_ends, now + 160, "the omen sets the item-spawner gap of 160 ticks");
    assert_eq!(OMENS.load(Ordering::SeqCst) - before, 1, "a Bad Omen is changed into Trial once");
}
