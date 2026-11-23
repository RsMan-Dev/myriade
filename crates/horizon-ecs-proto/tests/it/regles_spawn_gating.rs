//! The gates of the natural spawning, by event: the rules, the peaceful difficulty, the chunks that may spawn, the spectators and the 24
//! blocks around the world spawn. Each case counts what the pass gave birth to (`Spawner.stats`) over a few hundred ticks.
use horizon_core::fast_hash::FxHashSet;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::bridge::players::{set_player, PlayerFact};
use horizon_ecs_proto::cow::{Herd, Pos};
use horizon_ecs_proto::effects::Difficulty;
use horizon_ecs_proto::goals::Player;
use horizon_ecs_proto::spawn_env::SpawnEnv;
use horizon_ecs_proto::spawning::{Category, Counted, Spawner};
use bevy_ecs::prelude::*;

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

/// A slab with the natural spawning on, one player at `PLAYER`, and the host's inputs as `env` leaves them. The world spawn is far away
/// unless the case moves it.
fn herd(env: impl FnOnce(&mut SpawnEnv)) -> Herd {
    let mut h = Herd::with_spawn_world_awake(176, true);
    h.enable_spawning("minecraft:plains", 7);
    h.set_online(&[PLAYER]);
    let mut e = SpawnEnv { spawn_mobs: true, monsters: true, spawnable: chunks(10), border: None, world_spawn: [-200, 1, -200], changed: false };
    env(&mut e);
    h.world.insert_resource(e);
    h
}

/// The creatures born by the pass, by category.
fn births(h: &Herd) -> [[u64; 3]; 8] {
    h.world.resource::<Spawner>().stats.by_category
}

fn run(h: &mut Herd) {
    for _ in 0..TICKS {
        h.tick();
    }
}

#[test]
fn spawn_rules_off_means_no_birth() {
    if !have() {
        return;
    }
    let mut h = herd(|e| e.spawn_mobs = false);
    run(&mut h);
    assert_eq!(births(&h).iter().map(|c| c[2]).sum::<u64>(), 0);
}

#[test]
fn peaceful_lets_animals_spawn_but_no_monster() {
    if !have() {
        return;
    }
    let mut h = herd(|_| {});
    h.world.insert_resource(Difficulty(0));
    run(&mut h);
    let b = births(&h);
    assert_eq!(b[Category::Monster as usize][2], 0);
    assert!(b[Category::Creature as usize][2] > 0);
}

#[test]
fn a_chunk_outside_the_spawnable_set_gets_no_birth() {
    if !have() {
        return;
    }
    let mut h = herd(|e| e.spawnable = FxHashSet::default());
    run(&mut h);
    assert_eq!(births(&h).iter().map(|c| c[2]).sum::<u64>(), 0);
}

#[test]
fn a_spectator_alone_gets_no_birth() {
    if !have() {
        return;
    }
    let mut h = herd(|_| {});
    let spectator = Player { id: 1, x: PLAYER.0, y: 1.0, z: PLAYER.1, ..Default::default() };
    set_player(&mut h.world, spectator, PlayerFact { alive: true, targetable: true, hero: None, spectator: true });
    run(&mut h);
    assert_eq!(births(&h).iter().map(|c| c[2]).sum::<u64>(), 0);
}

#[test]
fn no_birth_within_24_blocks_of_the_world_spawn() {
    if !have() {
        return;
    }
    // The world spawn is on the player: every start near it is refused. A creature wanders a little after its birth, hence the margin.
    let spawn = [PLAYER.0 as i32, 1, PLAYER.1 as i32];
    let mut h = herd(|e| e.world_spawn = spawn);
    let mut seen: FxHashSet<Entity> = FxHashSet::default();
    let mut born = 0;
    for _ in 0..TICKS {
        h.tick();
        let fresh: Vec<(Entity, Vec3)> = h.world.query::<(Entity, &Pos, &Counted)>().iter(&h.world).filter(|(e, _, _)| !seen.contains(e)).map(|(e, p, _)| (e, p.0)).collect();
        for (e, p) in fresh {
            seen.insert(e);
            born += 1;
            let d2 = (p.x - (f64::from(spawn[0]) + 0.5)).powi(2) + (p.y - (f64::from(spawn[1]) + 0.5)).powi(2) + (p.z - (f64::from(spawn[2]) + 0.5)).powi(2);
            assert!(d2 >= 22.0 * 22.0, "born at {p:?}, {} blocks from the world spawn", d2.sqrt());
        }
    }
    assert!(born > 0);
}
