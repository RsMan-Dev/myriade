//! The lazy look option (`goals::set_lazy_look`): the look goals and the look control run only for an animal a player can
//! see (within 160 blocks). Nothing else a player can see may change: strolls, tempting and breeding at the same rates
//! with and without the option.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_ecs_proto::{cow::*, goals::*};

/// A 200-block slab: the player in one corner, the far corner out of its sight (more than 176 blocks along an axis: 160 rounded out to bins).
fn herd(lazy: bool, seed: u64) -> Herd {
    let mut h = Herd::new_awake(4000, 3.0, true, false, seed);
    set_lazy_look(&mut h.world, lazy);
    h
}

#[test]
fn the_lazy_look_changes_nothing_a_player_sees() {
    if horizon_ecs_proto::cow::skip_unless_default_species("the_lazy_look_changes_nothing_a_player_sees") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_farm_goals("the_lazy_look_changes_nothing_a_player_sees") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("the_lazy_look_changes_nothing_a_player_sees") {
        return;
    }
    if !have_data() {
        return;
    }
    // A species that hunts the player (the zombie) walks to it: the walk count then depends on the draws the look goals
    // make, which the option changes; the test counts strolls of passive animals.
    if horizon_ecs_proto::cow::skip_if(&horizon_ecs_proto::behaviours::melee::MODULE, "the_lazy_look_changes_nothing_a_player_sees") || horizon_ecs_proto::cow::skip_if(&horizon_ecs_proto::behaviours::ranged_bow::MODULE, "the_lazy_look_changes_nothing_a_player_sees") {
        return;
    }
    let mut out = Vec::new();
    for lazy in [false, true] {
        let mut total = (0, 0, 0, 0);
        for seed in [7, 8, 9] {
        let mut h = herd(lazy, seed);
        let half = f64::from(h.world.resource::<Level>().half);
        let corner = -half + 6.0;
        h.world.resource_mut::<Online>().set(Player { id: 1, x: corner, y: 1.0, z: corner, main: tempt_item().0, off: 0, ..Default::default() });
        let cows: Vec<Entity> = h.world.query_filtered::<Entity, With<Pos>>().iter(&h.world).collect();
        let pos = |h: &Herd, e: Entity| h.world.get::<Pos>(e).unwrap().0;
        // Pairs to feed: 10 near the player, 4 in the far corner (out of its sight).
        let near: Vec<Entity> = cows.iter().copied().filter(|&e| (pos(&h, e).x - corner).hypot(pos(&h, e).z - corner) < 16.0).collect();
        // The bins the player watches end at `edge` (10 bins of 16 blocks past its own): beyond it, out of sight.
        let edge = ((corner / 16.0).floor() + 11.0) * 16.0;
        let far: Vec<Entity> = cows.iter().copied().filter(|&e| pos(&h, e).x >= edge + 2.0 || pos(&h, e).z >= edge + 2.0).collect();
        let far_corner: Vec<Entity> = far.iter().copied().filter(|&e| pos(&h, e).x >= edge + 2.0 && pos(&h, e).z >= edge + 2.0).collect();
        assert!(near.len() >= 20 && far_corner.len() >= 8, "{} near, {} far", near.len(), far_corner.len());
        // The walkers are counted every 5 ticks over the last 200 (a count at one tick is a sample of a random number).
        let mut walkers = 0usize;
        for t in 1..=500 {
            if t > 300 && t % 5 == 0 {
                walkers += h.world.query::<&Stroll>().iter(&h.world).filter(|s| s.walking).count();
            }
            if t == 60 {
                for pair in near.chunks(2).take(10).chain(far_corner.chunks(2).take(4)) {
                    for &e in pair {
                        feed(&mut h.world, e);
                    }
                }
            }
            h.tick();
        }
        let strolled = walkers / 40;
        let babies = h.world.query::<&GrowAt>().iter(&h.world).count();
        let tempted = h.world.query::<&Tempting>().iter(&h.world).count();
        // The far animals (200 blocks away, out of sight) that have a look state now.
        let looking = far.iter().filter(|&&e| h.world.get::<Scanning>(e).is_some() || h.world.get::<Glancing>(e).is_some() || h.world.get::<Looking>(e).is_some()).count();
        total = (total.0 + strolled, total.1 + tempted, total.2 + babies, total.3 + looking);
        }
        out.push((lazy, total.0, total.1, total.2, total.3));
    }
    eprintln!("(lazy, walking, tempted, babies, looking states): {out:?}");
    let (a, b) = (out[0], out[1]);
    // The walks near the player: the same rate (the count at one tick, a few strollers more or less).
    assert!((a.1 as i64 - b.1 as i64).abs() <= 18, "walks: {} against {}", a.1, b.1);
    assert!((a.2 as i64 - b.2 as i64).abs() <= 9, "tempted: {} against {}", a.2, b.2);
    assert!((a.3 as i64 - b.3 as i64).abs() <= 6, "births: {} against {}", a.3, b.3);
    // (A few animals still have one: a timer that has not come up yet when the option is on.)
    assert!(b.4 * 8 < a.4 && a.4 > 90, "far animals with a look state: {} lazy, {} not", b.4, a.4);
}

#[test]
fn the_lazy_look_wakes_when_a_player_comes() {
    if horizon_ecs_proto::cow::skip_unless_small_body("the_lazy_look_wakes_when_a_player_comes") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("the_lazy_look_wakes_when_a_player_comes") {
        return;
    }
    // A wild cat or ocelot runs from the player it sees (`avoid_entity`, 16 blocks): it does not look around.
    if horizon_ecs_proto::cow::skip_if(&horizon_ecs_proto::behaviours::ocelot_attack::MODULE, "the_lazy_look_wakes_when_a_player_comes") {
        return;
    }
    if !have_data() {
        return;
    }
    let mut h = herd(true, 7);
    for _ in 0..300 {
        h.tick();
    }
    // Nobody sees: no look goal runs (`RandomLookAround`: scanning, `LookAtPlayer`: glancing). The head's turn (`Looking`) is not counted
    // here: a frog in a long jump aims too, whoever sees it, and its look control is asleep out of sight, so the mark stays until a player comes.
    let goals = |h: &mut Herd| counts(&mut h.world).iter().filter(|c| matches!(c.0, "scanning" | "glancing")).map(|c| c.1).sum::<usize>();
    assert_eq!(goals(&mut h), 0, "nobody sees: nobody looks around");
    let states = |h: &mut Herd| counts(&mut h.world).iter().filter(|c| matches!(c.0, "scanning" | "looking")).map(|c| c.1).sum::<usize>();
    let half = f64::from(h.world.resource::<Level>().half);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: -half + 20.0, y: 1.0, z: -half + 20.0, main: 0, off: 0, ..Default::default() });
    for _ in 0..300 {
        h.tick();
    }
    let seen = states(&mut h);
    // The control: the same herd without the option, the same player at the same tick. The animals a player sees (the near 40 % of the
    // slab) look around as often as in the control (a species that strolls all the time, the frog, looks rarely in both).
    let mut c = herd(false, 7);
    for _ in 0..300 {
        c.tick();
    }
    c.world.resource_mut::<Online>().set(Player { id: 1, x: -half + 20.0, y: 1.0, z: -half + 20.0, main: 0, off: 0, ..Default::default() });
    for _ in 0..300 {
        c.tick();
    }
    let control = states(&mut c);
    eprintln!("after a player comes: {seen} animals look around ({control} without the option)");
    assert!(seen > 0 && seen * 4 >= control, "{seen} against {control} without the option");
}
