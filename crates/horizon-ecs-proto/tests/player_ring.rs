//! `player_near` by event: after every tick an animal that did not move is active (`ActiveUntil` without end) exactly when a
//! player is within 32 blocks of it, for players that walk, jump, come and go.
mod common;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;

#[test]
fn the_ring_of_32_blocks_is_kept_by_events_for_walking_jumping_coming_and_going_players() {
    if horizon_ecs_proto::cow::skip_unless_small_body("the_ring_of_32_blocks_is_kept_by_events_for_walking_jumping_coming_and_going_players") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("the_ring_of_32_blocks_is_kept_by_events_for_walking_jumping_coming_and_going_players") {
        return;
    }
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(100, true, false);
    let at: Vec<(f64, f64)> = (0..600).map(|i| (-95.0 + f64::from(i % 30) * 6.3 + f64::from(i % 7) * 0.31, -95.0 + f64::from(i / 30) * 9.1 + f64::from(i % 5) * 0.27)).collect();
    let all = h.populate(&at, 11);
    let mut seed = 0x1234_5678_u64;
    let mut next = move |m: f64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as f64 / (1u64 << 53) as f64 * m
    };
    let mut players: Vec<(f64, f64)> = (0..5).map(|_| (next(180.0) - 90.0, next(180.0) - 90.0)).collect();
    let mut last: Vec<Option<(f64, f64)>> = vec![None; all.len()];
    let mut checked = 0;
    for t in 0..400 {
        match t % 40 {
            // A player jumps far, one leaves, one comes.
            13 => players[0] = (next(180.0) - 90.0, next(180.0) - 90.0),
            27 if players.len() > 2 => {
                players.pop();
            }
            33 => players.push((next(180.0) - 90.0, next(180.0) - 90.0)),
            _ => {
                for p in &mut players {
                    // Small steps, some of less than a block.
                    p.0 = (p.0 + next(0.9) - 0.45).clamp(-98.0, 98.0);
                    p.1 = (p.1 + next(0.9) - 0.45).clamp(-98.0, 98.0);
                }
            }
        }
        // Animals that move across the ring by themselves (a teleport is a move of the box like a step is).
        if t % 10 == 5 {
            for _ in 0..60 {
                let e = all[next(all.len() as f64) as usize];
                let p = h.world.get::<Pos>(e).unwrap().0;
                let to = Vec3::new((p.x + next(8.0) - 4.0).clamp(-97.0, 97.0), p.y, (p.z + next(8.0) - 4.0).clamp(-97.0, 97.0));
                h.teleport(e, to);
            }
        }
        h.set_players(players.clone());
        h.tick();
        for (k, &e) in all.iter().enumerate() {
            let p = h.world.get::<Pos>(e).unwrap().0;
            let moved = last[k].is_some_and(|l| l != (p.x, p.z));
            last[k] = Some((p.x, p.z));
            // An animal that moved this tick is tested at the next one.
            if moved || t < 2 {
                continue;
            }
            let near = players.iter().any(|q| (p.x - q.0).powi(2) + (p.z - q.1).powi(2) < 32.0 * 32.0);
            let active = h.world.get::<ActiveUntil>(e).unwrap().0 == u64::MAX;
            assert_eq!(active, near, "tick {t}: animal {k} at ({:.2}, {:.2}), players {players:?}", p.x, p.z);
            checked += 1;
        }
    }
    assert!(checked > 100_000);
}
