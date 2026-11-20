//! The End fight against Horizon's `Dragon` (`D:1113-1653`, `D:3035`, `D/death.rs`, `D:4130-4650`). Scene, imposed: the spikes of seed 7,
//! their ten crystals destroyed in a fixed order (the first by player 3, the others by the blasts' owner 4), then the death of the dragon
//! (ticks 150..=200, XP 12 000 the first time, 500 after), then a respawn started with four portal crystals and ended with four left.
//! The golden `dragon_golden/fight.txt` is written by the reconciler from Horizon's side; one line per event:
//! `crystal T IDX BY`, `place T X Y Z`, `block T X Y Z NAME`, `sound T NAME`, `xp T AMOUNT`, `bar T ID PROGRESS_BITS COLOR`,
//! `killed T BY`, `respawn T STAGE TIME`, `gone T`. Each is compared to the ECS side to the bit, at its tick.
use crate::common;
use horizon_ecs_proto::dragon::fight::{self, EndFight};
use horizon_ecs_proto::dragon::{crystals, death, respawn};

const SEED: i64 = 7;

/// The ECS side, line by line, in the golden's order: the world and the tick loop come from `common`.
fn ours() -> Vec<String> {
    let mut out = Vec::new();
    let mut w = common::fight_world(SEED);
    let spikes = horizon_world::worldgen::end_spike::spikes_for_seed(SEED);
    for s in &spikes {
        crystals::place_spike(&mut w, s);
    }
    let placed: Vec<_> = common::crystals_in_order(&mut w);
    for (i, c) in placed.iter().enumerate() {
        let by = if i == 0 { 3 } else { 4 };
        crystals::destroy(&mut w, *c, Some(by), true);
        out.push(format!("crystal {} {} {}", w.resource::<common::Tick>().0, i, by));
        common::advance(&mut w, 1);
    }
    let dragon = common::spawn_dragon(&mut w, [0.0, 128.0, 0.0]);
    w.resource_mut::<EndFight>().dragon = Some(dragon);
    death::begin(&mut w, dragon);
    for _ in 0..200 {
        common::advance(&mut w, 1);
        common::drain_lines(&mut w, &mut out);
    }
    let kills = w.resource::<fight::FightEvents>().killed.len();
    out.push(format!("killed {} {}", w.resource::<common::Tick>().0, kills));
    let portal: Vec<_> = (0..4).map(|_| crystals::spawn(&mut w, horizon_core::geometry::Vec3::new(0.5, 128.0, 0.5), false)).collect();
    respawn::start(&mut w, portal);
    for _ in 0..1000 {
        common::advance(&mut w, 1);
        common::drain_lines(&mut w, &mut out);
    }
    out
}

#[test]
fn dragon_fight_matches_horizon_to_the_tick() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/dragon_golden/fight.txt");
    let golden = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("golden {path} missing (the reconciler writes it): {e}"));
    let ours = ours().join("\n");
    assert_eq!(ours, golden.trim_end(), "fight lines differ from Horizon's Dragon");
}
