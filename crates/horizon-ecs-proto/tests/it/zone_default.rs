//! The zone of simulation is the DEFAULT of a herd (`Herd::new`, `with_slab`...): with no player registered every creature sleeps (no
//! system visits it), a player wakes only the creatures of its radius, and they sleep again when it leaves. The `_awake`
//! constructors (every creature ticks, no zone) are the explicit choice of a harness, never the state of a server without players.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::{cow::*, zone::Dormant};

const HALF: i32 = 260;

fn grid(x0: f64, z0: f64, n: usize) -> Vec<(f64, f64)> {
    (0..n).map(|i| (x0 + (i % 5) as f64 * 3.0, z0 + (i / 5) as f64 * 3.0)).collect()
}
fn asleep(h: &Herd, e: Entity) -> bool {
    h.world.get_entity(e).is_ok_and(|m| m.contains::<Dormant>())
}

#[test]
fn no_player_means_every_creature_sleeps_and_a_player_wakes_only_its_radius() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(HALF, true, false);
    let near = h.populate(&grid(0.5, 0.5, 25), 1);
    let far = h.populate(&grid(200.5, 200.5, 25), 2);
    let all: Vec<Entity> = near.iter().chain(far.iter()).copied().collect();
    // 0 player: nothing is awake, over many ticks, and nothing moves.
    for _ in 0..3 {
        h.tick();
    }
    let at = |h: &Herd| all.iter().map(|&e| h.world.get::<Pos>(e).unwrap().0).collect::<Vec<_>>();
    let rest = at(&h);
    assert!(all.iter().all(|&e| asleep(&h, e)), "a creature is awake with no player");
    let woke0 = h.world.resource::<Index>().zone.stats.woke;
    for _ in 0..300 {
        h.tick();
    }
    assert_eq!(rest, at(&h), "a sleeping creature moved");
    assert!(all.iter().all(|&e| asleep(&h, e)), "a creature woke with no player");
    assert_eq!(h.world.resource::<Index>().zone.stats.woke, woke0, "a wake with no player");
    // A player beside the first group: only the creatures within its radius wake (the far group is 12 chunks away, the radius 10).
    h.set_players(vec![(0.5, 0.5)]);
    for _ in 0..5 {
        h.tick();
    }
    assert!(near.iter().all(|&e| !asleep(&h, e)), "a creature of its radius sleeps");
    assert!(far.iter().all(|&e| asleep(&h, e)), "a creature out of its radius woke");
    // It leaves: they sleep again.
    h.set_players(vec![]);
    for _ in 0..5 {
        h.tick();
    }
    assert!(all.iter().all(|&e| asleep(&h, e)), "a creature stayed awake after the last player left");
}

#[test]
fn the_awake_constructors_are_the_explicit_choice_of_a_harness() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(HALF, true, false);
    let es = h.populate(&grid(0.5, 0.5, 5), 3);
    h.tick();
    assert!(es.iter().all(|&e| !asleep(&h, e)), "an _awake herd put a creature to sleep");
}
