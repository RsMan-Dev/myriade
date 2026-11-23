//! `PhantomAttackStrategyGoal.start`: the first dive comes at tick 8 of the chase (`nextSweepTick = adjustedTickDelay(...)`, the goal
//! ticks every second server tick). Expected value from the jar, not from the code under test.
use crate::common;
use common::*;
use horizon_ecs_proto::behaviours::flier_goals::Strategy;
use horizon_ecs_proto::cow::{spawn_species, ActiveUntil, Herd, SpeciesTable};

#[test]
fn the_first_dive_is_scheduled_eight_ticks_into_the_chase() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    h.set_players(vec![(0.5, 0.5)]);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let phantom = *table.iter().find(|s| s.name == "minecraft:phantom").unwrap();
    let e = spawn_species(&mut h.world, &phantom, 4.5, 4.5, 5);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.tick();
    let next = h.world.get::<Strategy>(e).map(|s| s.next_sweep);
    assert_eq!(next, Some(8), "the first dive is at tick 8 (`PhantomAttackStrategyGoal.start`)");
}

/// The swoop's sound (level event 1039, `PhantomSweepAttackGoal.tick`) comes at the phantom's block when its dive reaches a player,
/// and a `Silent` phantom makes none.
#[test]
fn a_bite_is_level_event_1039_at_the_phantoms_block_unless_it_is_silent() {
    if !have_data() {
        return;
    }
    let mut events: Vec<(bool, Vec<([i32; 3], [i32; 3])>)> = Vec::new();
    for silent in [false, true] {
        let mut h = Herd::with_slab_awake(8, true, false);
        let fact = horizon_ecs_proto::bridge::players::PlayerFact { alive: true, targetable: true, hero: None, spectator: false };
        horizon_ecs_proto::bridge::players::set_player(&mut h.world, horizon_ecs_proto::goals::Player { id: 1, x: 0.5, y: 1.0, z: 0.5, ..Default::default() }, fact);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let phantom = *table.iter().find(|s| s.name == "minecraft:phantom").unwrap();
        let e = spawn_species(&mut h.world, &phantom, 4.5, 4.5, 5);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        if silent {
            h.world.entity_mut(e).insert(horizon_ecs_proto::behaviours::elder_curse::Silent);
        }
        let mut level = Vec::new();
        for _ in 0..400 {
            h.tick();
            let p = h.world.get::<horizon_ecs_proto::cow::Pos>(e).unwrap().0;
            let block = [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32];
            for ev in h.world.resource_mut::<horizon_ecs_proto::goals::Events>().host.drain() {
                if let horizon_ecs_proto::host_out::HostEvent::LevelEvent { event: 1039, at, .. } = ev {
                    level.push((at, block));
                }
            }
        }
        events.push((silent, level));
    }
    let (_, loud) = &events[0];
    let (_, quiet) = &events[1];
    assert!(!loud.is_empty(), "the bite plays level event 1039");
    assert!(loud.iter().all(|(at, block)| at == block), "the bite is at the phantom's block");
    assert!(quiet.is_empty(), "a silent phantom plays no bite");
}
