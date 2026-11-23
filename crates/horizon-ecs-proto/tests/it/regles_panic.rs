use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_ecs_proto::behaviours::panic::{self, Panic, Panicking, Params};
use horizon_ecs_proto::behaviours::animal_panic::{self, AnimalPanicking};
use horizon_ecs_proto::behaviours::Entry;
use horizon_ecs_proto::brain::LastDamage;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::species::Config;

/// The turtle looks for water within 7 blocks (`lookForWater(level, mob, 7)`); the camel burns and looks within 5.
static TURTLE: Params = Params { water: Some(7), ..Params::DEFAULT };
static PLAIN: Params = Params { ..Params::DEFAULT };

/// A herd with one animal of `species` on the block (0, 0, 0), running only the panic goal with `params`, and `water` placed at the
/// block offsets given (y 0 is the animal's own level, as in the other tests of this harness).
fn world(species: &str, params: &'static Params, water: &[[i32; 3]]) -> Option<(Herd, Entity)> {
    if !have_data() {
        return None;
    }
    let mut h = Herd::with_slab_awake(60, false, false);
    set_lazy_look(&mut h.world, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let mut sp = *table.iter().find(|s| s.name == species).unwrap();
    let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&panic::MODULE, 1, params)]));
    sp.goals = Config { entries };
    for p in water {
        h.set_block(p[0], p[1], p[2], "minecraft:water");
    }
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 11);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    arm_timers(&mut h.world, e);
    h.tick();
    Some((h, e))
}

/// A hit by a creature, as `apply_damage` leaves it: its `LastDamage` and the `hurt` event.
fn hurt(h: &mut Herd, e: Entity) {
    let now = now_of(&h.world, e);
    h.world.entity_mut(e).insert(LastDamage { at: now, panic: true, by: None });
    h.world.resource_mut::<Events>().hurt.push(e);
}

/// `TurtlePanicGoal.lookForWater` walks `BlockPos.manhattanOrdered` by depth, then x, then y, then +z before -z. The
/// spot is the block centre (`x + 0.5`, `y`, `z + 0.5`); the expected blocks are the source's, offset from the turtle's block.
#[test]
fn the_turtle_flees_to_the_manhattan_first_pool() {
    let Some((mut h, e)) = world("minecraft:turtle", &TURTLE, &[[0, 0, 1], [0, 0, -1]]) else {
        return;
    };
    hurt(&mut h, e);
    h.tick();
    h.tick();
    assert_eq!(h.world.get::<Panic>(e).unwrap().pick, (0.5, 0, 1.5));

    let Some((mut h, e)) = world("minecraft:turtle", &TURTLE, &[[0, 1, 0], [1, 0, 0]]) else {
        return;
    };
    hurt(&mut h, e);
    h.tick();
    h.tick();
    assert_eq!(h.world.get::<Panic>(e).unwrap().pick, (0.5, 1, 0.5));

    let Some((mut h, e)) = world("minecraft:turtle", &TURTLE, &[[0, 0, 2], [0, 0, -1]]) else {
        return;
    };
    hurt(&mut h, e);
    h.tick();
    h.tick();
    assert_eq!(h.world.get::<Panic>(e).unwrap().pick, (0.5, 0, -0.5));
}

/// A herd with one goat running only `AnimalPanic` (`GoatAi.initCoreActivity`: speed 2.0), as the goat's own goal.
fn goat_world() -> Option<(Herd, Entity)> {
    if !have_data() {
        return None;
    }
    let mut h = Herd::with_slab_awake(60, false, false);
    set_lazy_look(&mut h.world, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let mut sp = *table.iter().find(|s| s.name == "minecraft:goat").unwrap();
    let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&animal_panic::MODULE, 1, &animal_panic::Params::DEFAULT)]));
    sp.goals = Config { entries };
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 11);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    arm_timers(&mut h.world, e);
    h.tick();
    Some((h, e))
}

/// `AnimalPanic.start` (`Behavior.start`): `endTimestamp = now + minDuration + nextInt(maxDuration + 1 - minDuration)` with the
/// constructor's 100 and 120 (`animal_panic::Params::DEFAULT`: min 100, spread 21), so a hit runs the goat for 100 to 120 ticks.
#[test]
fn a_panicking_goat_flees_for_its_drawn_100_to_120_ticks() {
    let Some((mut h, e)) = goat_world() else {
        return;
    };
    hurt(&mut h, e);
    let mut panicked = 0;
    for _ in 0..200 {
        h.tick();
        if h.world.get::<AnimalPanicking>(e).is_some() {
            panicked += 1;
        }
    }
    assert!((100..=120).contains(&panicked), "a panicking goat runs 100 to 120 ticks, not {panicked}");
}

/// A burning camel (`Burning`, range 5) takes no lone water block, and takes a pool two blocks wide: the lone block is no pool.
#[test]
fn a_burning_camel_needs_a_two_by_two_pool() {
    let Some((mut h, e)) = world("minecraft:camel", &PLAIN, &[[0, 0, 1]]) else {
        return;
    };
    let until = now_of(&h.world, e) + 1000;
    h.world.entity_mut(e).insert(horizon_ecs_proto::cow::Burning { until: until as i64 });
    hurt(&mut h, e);
    h.tick();
    h.tick();
    assert_ne!(h.world.get::<Panic>(e).unwrap().pick, (0.5, 0, 1.5), "a lone water block is no pool");

    let Some((mut h, e)) = world("minecraft:camel", &PLAIN, &[[0, 0, 1], [0, 0, 2], [1, 0, 1], [1, 0, 2]]) else {
        return;
    };
    let until = now_of(&h.world, e) + 1000;
    h.world.entity_mut(e).insert(horizon_ecs_proto::cow::Burning { until: until as i64 });
    hurt(&mut h, e);
    h.tick();
    h.tick();
    assert_eq!(h.world.get::<Panic>(e).unwrap().pick, (0.5, 0, 1.5));
}
