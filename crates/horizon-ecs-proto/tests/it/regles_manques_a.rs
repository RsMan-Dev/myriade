//! A pig on fire in the dry, in water and in rain (`wet_fire_tests.rs`): one burn of 1 on the first tick and on every twentieth tick
//! of fire, water and rain put the fire out at once, with or without AI. Expected values are the Mob's table rows, not the code's.
//! The lava and neighbouring-fire rows are printed by the Mob's test without an assertion, so they are not here either.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::dragon::flight::NoAi;
use horizon_ecs_proto::effects::IMMUNE_FIRE;
use horizon_ecs_proto::goals::Events;
use horizon_ecs_proto::host_out::HostEvent;

/// The extinguish sounds the host queue holds since the last call.
fn hisses(h: &mut Herd) -> usize {
    h.world.resource_mut::<Events>().host.drain().filter(|e| matches!(e, HostEvent::Sound { name, .. } if *name == "entity.generic.extinguish_fire")).count()
}

/// A pig `fire` ticks on fire at (x, z), with the NoAi marker when `no_ai`.
fn spawn_pig(h: &mut Herd, x: f64, z: f64, no_ai: bool, fire: u64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:pig").unwrap();
    let e = spawn_species(&mut h.world, &sp, x, z, 5);
    if no_ai {
        h.world.entity_mut(e).insert(NoAi);
    }
    set_on_fire(&mut h.world, e, fire);
    e
}

/// Water in the cells under and over the pig's feet (x and z from 0 to 2), or air again.
fn water(h: &mut Herd, on: bool) {
    let block = if on { "minecraft:water" } else { "minecraft:air" };
    for x in 0..=2 {
        for z in 0..=2 {
            h.set_block(x, 1, z, block);
            h.set_block(x, 2, z, block);
        }
    }
}

/// `Entity.baseTick` in the dry (table rows `dry fire=21`, `fire=20`, `fire=1`): a fire of 21 burns no one on its tick, a fire of 20 takes
/// one, and a fire of one tick is out on its tick.
#[test]
fn a_pig_on_fire_in_the_dry_takes_one_burn_on_its_twentieth_tick() {
    if !have_data() || wrong_species(&["minecraft:pig"]) {
        return;
    }
    for no_ai in [false, true] {
        for (fire, health_after, lit) in [(0, 10.0, false), (1, 10.0, false), (19, 10.0, true), (20, 9.0, true), (21, 10.0, true), (40, 9.0, true)] {
            let mut h = Herd::with_slab_awake(8, true, false);
            let pig = spawn_pig(&mut h, 1.5, 1.5, no_ai, fire);
            h.tick();
            assert_eq!(health(&h, pig), health_after, "NoAI={no_ai} dry fire={fire}");
            assert_eq!(h.world.get::<Burning>(pig).is_some(), lit, "NoAI={no_ai} dry fire={fire}");
        }
    }
}

/// `Entity.baseTick` in water and in rain (table rows `water fire=N`, `rain fire=N`): the wet puts the fire out on the tick, and a fire
/// of 20 or 40 still takes its burn of 1 on that tick.
#[test]
fn a_pig_in_water_or_rain_puts_its_fire_out_on_the_tick() {
    if !have_data() || wrong_species(&["minecraft:pig"]) {
        return;
    }
    for no_ai in [false, true] {
        for wet in ["water", "rain"] {
            for (fire, health_after) in [(1, 10.0), (19, 10.0), (20, 9.0), (21, 10.0), (40, 9.0)] {
                let mut h = Herd::with_slab_awake(8, true, false);
                water(&mut h, wet == "water");
                h.set_raining(wet == "rain");
                let pig = spawn_pig(&mut h, 1.5, 1.5, no_ai, fire);
                h.tick();
                assert_eq!(health(&h, pig), health_after, "NoAI={no_ai} {wet} fire={fire}");
                assert!(h.world.get::<Burning>(pig).is_none(), "NoAI={no_ai} {wet} fire={fire}: the fire is out");
                assert_eq!(hisses(&mut h), usize::from(fire > 1), "NoAI={no_ai} {wet} fire={fire}: hiss");
            }
        }
    }
}

/// `Entity.baseTick` through a walk (table rows `transition=water/dry`, `transition=rain/dry`): a pig on fire of 21 in the dry is at 10
/// health and stays lit; the next tick in water or rain takes its burn of 1 and puts the fire out; back in the dry it does not light again.
#[test]
fn a_pig_that_walks_into_water_or_rain_goes_out_and_stays_out() {
    if !have_data() || wrong_species(&["minecraft:pig"]) {
        return;
    }
    for no_ai in [false, true] {
        for wet_by_water in [true, false] {
            let mut h = Herd::with_slab_awake(8, true, false);
            let pig = spawn_pig(&mut h, 1.5, 1.5, no_ai, 21);
            h.tick();
            assert_eq!(health(&h, pig), 10.0, "NoAI={no_ai} dry fire=21");
            assert!(h.world.get::<Burning>(pig).is_some(), "NoAI={no_ai} still lit in the dry");
            assert_eq!(hisses(&mut h), 0, "NoAI={no_ai} dry step");

            if wet_by_water {
                water(&mut h, true);
            } else {
                h.set_raining(true);
            }
            h.tick();
            assert_eq!(health(&h, pig), 9.0, "NoAI={no_ai} wet step");
            assert!(h.world.get::<Burning>(pig).is_none(), "NoAI={no_ai} wet step puts the fire out");
            assert_eq!(hisses(&mut h), 1, "NoAI={no_ai} wet step hiss");

            if wet_by_water {
                water(&mut h, false);
            } else {
                h.set_raining(false);
            }
            h.tick();
            assert_eq!(health(&h, pig), 9.0, "NoAI={no_ai} dry again");
            assert!(h.world.get::<Burning>(pig).is_none(), "NoAI={no_ai} dry again, no fire");
            assert_eq!(hisses(&mut h), 0, "NoAI={no_ai} dry again hiss");
        }
    }
}

/// `Entity.baseTick` of a fire-immune pig (table rows `immune` in the dry, water and rain, fire 20): the immunity refuses the fire, so the
/// pig keeps its health and is never lit. The table sets its fire directly; here the ignition is refused, which is what the immunity does.
#[test]
fn a_fire_immune_pig_never_burns_in_the_dry_water_or_rain() {
    if !have_data() || wrong_species(&["minecraft:pig"]) {
        return;
    }
    for no_ai in [false, true] {
        for wet in ["dry", "water", "rain"] {
            let mut h = Herd::with_slab_awake(8, true, false);
            water(&mut h, wet == "water");
            h.set_raining(wet == "rain");
            let table = h.world.resource::<SpeciesTable>().0.clone();
            let sp = *table.iter().find(|s| s.name == "minecraft:pig").unwrap();
            let pig = spawn_species(&mut h.world, &sp, 1.5, 1.5, 5);
            if no_ai {
                h.world.entity_mut(pig).insert(NoAi);
            }
            h.world.entity_mut(pig).insert(Immune(IMMUNE_FIRE));
            set_on_fire(&mut h.world, pig, 20);
            h.tick();
            assert_eq!(health(&h, pig), 10.0, "NoAI={no_ai} immune {wet}");
            assert!(h.world.get::<Burning>(pig).is_none(), "NoAI={no_ai} immune {wet}: never lit");
            assert_eq!(hisses(&mut h), 0, "NoAI={no_ai} immune {wet} hiss");
        }
    }
}
