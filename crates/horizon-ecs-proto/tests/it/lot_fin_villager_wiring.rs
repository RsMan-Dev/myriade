//! Le villageois après le câblage du lot 5 : la célébration de victoire (`Celebrate(timed(600, 600))`, `villager_brain.rs:4235`, tick
//! `:3024`), sur un troupeau éveillé. Valeurs attendues lues dans le `Mob`, pas dans le code ECS.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_ecs_proto::behaviours::{raid, victory_celebrate::Celebration};
use horizon_ecs_proto::{brain::*, cow::*, goals::Entered};

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:villager"])
}

fn villager(h: &mut Herd) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:villager").unwrap();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 1);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    horizon_ecs_proto::behaviours::schedule::Times::set(&mut h.world, 4000);
    e
}

#[test]
fn the_villager_celebrates_a_victory_for_600_ticks_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = villager(&mut h);
    raid::set_status(&mut h.world, [0.5, 1.0, 0.5], 64.0, raid::RAID_VICTORY);
    // `SetRaidStatus` poses the `raid` activity one tick in twenty; the celebration starts with it.
    for _ in 0..400 {
        h.tick();
        if h.world.get::<Celebration>(e).is_some() {
            break;
        }
    }
    let until = h.world.get::<Celebration>(e).expect("la célébration commence à la victoire").until;
    let now = now_of(&h.world, e);
    // `Celebrate(timed(600, 600))`: 600 ticks from the start (the tick of the start may be one before `now`).
    assert!((599..=601).contains(&(until as i64 - now as i64)), "fin à {until}, maintenant {now}");
}
