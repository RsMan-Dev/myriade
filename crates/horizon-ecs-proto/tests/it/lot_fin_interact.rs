//! `InteractGoal` (`mobs/goals.rs:1322-1390`, the trader's `LookAtPlayer::interact(3.0, 1.0)`, `mobs/trader.rs:87`): while it looks at a player
//! within 3 blocks it holds MOVE as well as LOOK, so a goal that wants MOVE (here a probe, `Stroll`'s flag) does not start meanwhile.
use bevy_ecs::prelude::*;
use horizon_ecs_proto::{
    behaviours::{look_at_player, Entry, Module, MOVE},
    cow::*,
    goals::*,
    species::Config,
};

static PROBE: Module = Module { name: "probe_move", flags: MOVE, ..Module::DEFAULT };

#[test]
fn the_interact_look_holds_move_so_a_mover_waits() {
    let mut h = Herd::with_slab_awake(60, true, false);
    let p: &'static look_at_player::Params = Box::leak(Box::new(look_at_player::Params { range: 3.0, period: 1, ..look_at_player::Params::DEFAULT }));
    let entries: &'static [Entry] = Box::leak(vec![Entry::new(&look_at_player::INTERACT, 9, p), Entry::new(&PROBE, 10, &())].into_boxed_slice());
    let mut sp = *species();
    sp.goals = Config { entries };
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 70);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 1.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    let looked_once = std::cell::Cell::new(false);
    for _ in 0..200 {
        h.tick();
        let g = h.world.get::<GoalSet>(e).unwrap();
        looked_once.set(looked_once.get() | g.is_running(&look_at_player::INTERACT));
        assert!(!(g.is_running(&look_at_player::INTERACT) && g.is_running(&PROBE)), "a mover ran beside the interact look");
    }
    assert!(looked_once.get(), "the interact look started");
}
