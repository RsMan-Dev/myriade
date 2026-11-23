//! Règle du suivi, reprise de `invisible_follow_tests.rs` (valeurs inchangées) : un animal suiveur ne suit pas un animal invisible
//! (`FollowMobGoal.canUse`, `!isInvisible()`) ; il suit un animal visible d'une autre espèce.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::behaviours::follow_mob::{self, Params, Tailing};
use horizon_ecs_proto::effects::Invisible;
use horizon_ecs_proto::behaviours::Entry;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;

/// Whether a pig with the follow goal starts to follow a cow that is `invisible` or not, after one tick.
fn follows(invisible: bool) -> bool {
    let mut h = Herd::with_slab_awake(60, false, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let mut follower_sp = *table.iter().find(|s| s.name == "minecraft:pig").unwrap();
    let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&follow_mob::MODULE, 1, Box::leak(Box::new(Params::DEFAULT)))]));
    follower_sp.goals = Config { entries };
    let cow_sp = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
    let follower = spawn_armed(&mut h.world, &follower_sp, 0.5, 0.5, 81);
    let target = spawn_armed(&mut h.world, &cow_sp, 1.5, 0.5, 82);
    for e in [follower, target] {
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    }
    if invisible {
        h.world.entity_mut(target).insert(Invisible);
    }
    h.tick();
    h.world.get::<Tailing>(follower).is_some()
}

#[test]
fn a_follower_follows_a_visible_animal_and_passes_over_an_invisible_one() {
    if !have_data() {
        return;
    }
    assert!(follows(false), "a visible animal is followed");
    assert!(!follows(true), "an invisible animal is not");
}
