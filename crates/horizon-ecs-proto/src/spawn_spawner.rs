//! The block spawner (`MobSpawner`, `spawner.rs`) by event. Vanilla decrements its delay each tick while a player is within
//! `required_player_range`, and tries to spawn when it reads 0. Here a spawner holds no per-tick state: it has a PHASE (idle, running
//! until the tick it will try, frozen with the ticks left while no player is near) and one wake-up in the schedule of the service. The
//! player's approach and departure are events (`near_changed`), so an idle spawner costs nothing.
//! The attempt, as vanilla's tick: `count` positions in the box of `spawn_range`, each refused by `valid` (the spawn rules and the
//! obstruction, the bridge's), then `max_nearby` of its species in the box ends the attempt; a spawn is `spawning::born`. The first
//! attempt comes `delay` ticks after the player's arrival; after an attempt that spawned (or hit the limit) the next one comes `1 + delay`
//! ticks later, and after one that spawned nothing, one tick later. Not here: the spawn event of the level (its particles are the
//! host's), the item of a spawner that displays one, the client's delay.
use crate::cow::{species_table, Clock, Level, Pos, Species, SpeciesId};
use crate::goals::{Events, Online};
use crate::rules::spawn_spawner::{attempt_position, spawn_delay};
pub use crate::rules::spawn_spawner::{weighted, weighted_index};
use bevy_ecs::prelude::*;
use horizon_core::random::{LegacyRandom, RandomSource};
use std::collections::{BTreeMap, HashMap};

/// A block position.
pub type Block3 = [i32; 3];

/// The `valid` callback of an attempt: the spawn rules and the obstruction of the species at a position.
pub type Valid = fn(&World, &Species, [f64; 3]) -> bool;

/// The configuration of a spawner: the block's tag, else `MobSpawner.load`'s defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub min_delay: i32,
    pub max_delay: i32,
    pub count: u8,
    pub max_nearby: usize,
    pub required_range: f64,
    pub spawn_range: i32,
}
impl Default for Config {
    fn default() -> Self {
        Self { min_delay: 200, max_delay: 800, count: 4, max_nearby: 6, required_range: 16.0, spawn_range: 4 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// No player near has started it yet.
    Idle,
    /// Near: it tries at this tick.
    Running(u64),
    /// Not near: it tries after this many more near ticks.
    Frozen(u64),
}

/// One spawner of the world (block data: not a component of any mob).
struct Block {
    cfg: Config,
    /// The species the spawner's data holds, weighted (a single entry when the tag has none).
    potentials: Vec<(u8, i32)>,
    /// The species of the next mob (`None`: the attempt finds no species and waits one tick).
    next: Option<u8>,
    phase: Phase,
    /// Bumped each time the phase leaves `Running`: a wake-up of an older generation is stale.
    generation: u32,
}

/// The spawners of the world and their wake-ups, keyed by the tick they fire at.
#[derive(Resource, Default)]
pub struct Spawners {
    at: HashMap<Block3, Block>,
    wakes: BTreeMap<u64, Vec<(Block3, u32)>>,
}

/// The spawners' own random source (the level's, in vanilla).
#[derive(Resource)]
pub struct SpawnerRng(pub LegacyRandom);

/// The spawners' resources, made at the first use.
pub fn ensure(w: &mut World, seed: i64) {
    if !w.contains_resource::<Spawners>() {
        w.insert_resource(Spawners::default());
        w.insert_resource(SpawnerRng(LegacyRandom::new(seed)));
    }
}

/// Places a spawner at `pos`. Its first species is drawn from the potentials, as `setEntityId` does at load.
pub fn place(w: &mut World, pos: Block3, cfg: Config, potentials: Vec<(u8, i32)>) {
    let next = w.resource_scope(|_, mut rng: Mut<SpawnerRng>| weighted(&potentials, &mut rng.0));
    w.resource_mut::<Spawners>().at.insert(pos, Block { cfg, potentials, next, phase: Phase::Idle, generation: 0 });
}

/// A player came near (`true`) or went away (`false`) of the spawner at `pos`. Pure bookkeeping: the wake-up is scheduled or dropped.
pub fn near_changed(w: &mut World, pos: Block3, near: bool) {
    let now = w.resource::<Clock>().0;
    w.resource_scope(|w, mut rng: Mut<SpawnerRng>| {
        let mut s = w.resource_mut::<Spawners>();
        let s = &mut *s;
        let Some(b) = s.at.get_mut(&pos) else { return };
        match (near, b.phase) {
            (true, Phase::Idle) => {
                let due = now + spawn_delay(b.cfg.min_delay, b.cfg.max_delay, &mut rng.0) as u64;
                b.phase = Phase::Running(due);
                schedule(&mut s.wakes, due, pos, b.generation);
            }
            (true, Phase::Frozen(left)) => {
                let due = now + left;
                b.phase = Phase::Running(due);
                schedule(&mut s.wakes, due, pos, b.generation);
            }
            (false, Phase::Running(due)) => {
                b.generation += 1;
                b.phase = Phase::Frozen(due.saturating_sub(now));
            }
            _ => {}
        }
    });
}

/// Runs the attempts due at this tick.
pub fn run_wakes(w: &mut World, valid: Valid) {
    let now = w.resource::<Clock>().0;
    let ready: Vec<(Block3, u32)> = {
        let mut s = w.resource_mut::<Spawners>();
        let later = s.wakes.split_off(&(now + 1));
        std::mem::replace(&mut s.wakes, later).into_values().flatten().collect()
    };
    for (pos, generation) in ready {
        let due = w.resource::<Spawners>().at.get(&pos).is_some_and(|b| b.generation == generation && b.phase == Phase::Running(now));
        if due {
            w.resource_scope(|w, mut rng: Mut<SpawnerRng>| attempt(w, &mut rng.0, pos, now, valid));
        }
    }
}

fn attempt(w: &mut World, rng: &mut LegacyRandom, pos: Block3, now: u64, valid: Valid) {
    let (cfg, next) = match w.resource::<Spawners>().at.get(&pos) {
        Some(b) => (b.cfg, b.next),
        None => return,
    };
    let Some(sp) = next.and_then(|id| species_table().0.into_iter().find(|s| s.id == id)) else {
        return delay(w, rng, pos, now, false);
    };
    let mut spawned = 0usize;
    for _ in 0..cfg.count {
        let at = attempt_position(pos, cfg.spawn_range, rng);
        if !valid(w, &sp, at) {
            continue;
        }
        if nearby(w, sp.id, pos, cfg.spawn_range) >= cfg.max_nearby {
            return delay(w, rng, pos, now, true);
        }
        let seed = rng.next_long() as u64;
        let nearest = nearest_sqr(w, at);
        let mw = w.resource::<Level>().mw.0;
        if crate::spawn_natural::born_finalized(w, mw, &sp, at, seed, nearest, crate::spawn_reason::SpawnReason::Spawner, &mut None).is_none() {
            continue;
        }
        spawned += 1;
        w.resource_mut::<Events>().host.particles(
            "minecraft:flame",
            [f64::from(pos[0]) + 0.5, f64::from(pos[1]) + 0.5, f64::from(pos[2]) + 0.5],
            [0.5, 0.5, 0.5],
            0.0,
            20,
        );
    }
    delay(w, rng, pos, now, spawned > 0);
}

/// `MobSpawner.delay`: a new delay and a new species after a spawn (or a limit hit); otherwise one tick.
fn delay(w: &mut World, rng: &mut LegacyRandom, pos: Block3, now: u64, spawned: bool) {
    let mut s = w.resource_mut::<Spawners>();
    let Some(b) = s.at.get_mut(&pos) else { return };
    let due = if spawned {
        let d = spawn_delay(b.cfg.min_delay, b.cfg.max_delay, rng) as u64;
        b.next = weighted(&b.potentials, rng);
        now + 1 + d
    } else {
        now + 1
    };
    b.phase = Phase::Running(due);
    let generation = b.generation;
    schedule(&mut s.wakes, due, pos, generation);
}

fn schedule(wakes: &mut BTreeMap<u64, Vec<(Block3, u32)>>, due: u64, pos: Block3, generation: u32) {
    wakes.entry(due).or_default().push((pos, generation));
}

/// The creatures of the species whose position is in the box of the spawner grown by `range` (`count_entities`). A scan of the
/// species' creatures, once per attempt (an attempt comes every 200 to 800 ticks): the spatial index lags the cows that moved and
/// the ones just born, which made the limit overshoot.
fn nearby(w: &mut World, species: u8, pos: Block3, range: i32) -> usize {
    let r = f64::from(range);
    let (x, z) = (f64::from(pos[0]), f64::from(pos[2]));
    let (ylo, yhi) = (f64::from(pos[1]) - r, f64::from(pos[1]) + 1.0 + r);
    w.query::<(&SpeciesId, &Pos)>()
        .iter(w)
        .filter(|(s, p)| s.0 == species && p.0.y >= ylo && p.0.y <= yhi && p.0.x >= x - r && p.0.x <= x + 1.0 + r && p.0.z >= z - r && p.0.z <= z + 1.0 + r)
        .count()
}

/// The squared distance from `at` to the nearest online player (the spawn's despawn reads it).
pub fn nearest_sqr(w: &World, at: [f64; 3]) -> f64 {
    w.resource::<Online>()
        .players()
        .map(|p| (p.x - at[0]).powi(2) + (p.y - at[1]).powi(2) + (p.z - at[2]).powi(2))
        .fold(f64::MAX, f64::min)
}
