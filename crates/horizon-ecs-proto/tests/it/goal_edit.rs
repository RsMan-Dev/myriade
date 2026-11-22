//! A mod's edits of the goal list (`goal_edit.rs`): add, remove, reset and read on one animal; the others keep the list of their species.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::{
    behaviours::{climb_on_powder_snow, panic, Entry, Module},
    cow::*,
    goal_edit::{self, GoalEdit},
    goals::*,
};

fn herd() -> Herd {
    Herd::new_awake(3, 3.0, true, false, 5)
}
fn animals(h: &mut Herd) -> Vec<Entity> {
    let mut v: Vec<Entity> = h.world.query_filtered::<Entity, With<GoalSet>>().iter(&h.world).collect();
    v.sort();
    v
}
fn mods(h: &Herd, e: Entity) -> Vec<(u8, &'static str)> {
    goal_edit::list(&h.world, e, false).unwrap().into_iter().map(|(p, m, _)| (p, m.name)).collect()
}
fn is_panic(m: &'static Module) -> bool {
    std::ptr::eq(m, &panic::MODULE)
}
static SNOW: climb_on_powder_snow::Params = climb_on_powder_snow::Params::DEFAULT;

#[test]
fn a_mod_edits_one_animal_and_the_others_keep_their_species_list() {
    if horizon_ecs_proto::cow::skip_unless_default_species("a_mod_edits_one_animal") || !have_data() {
        return;
    }
    let mut h = herd();
    let ids = animals(&mut h);
    let (a, b) = (ids[0], ids[1]);
    let species = mods(&h, a);
    assert!(species.iter().any(|(_, n)| *n == "panic"));

    // Removal: every goal of the kind goes, the other slots keep their place.
    assert_eq!(goal_edit::remove_where(&mut h.world, a, false, &is_panic), Some(1));
    let after: Vec<_> = mods(&h, a);
    assert_eq!(after.len(), species.len() - 1);
    assert!(after.iter().all(|(_, n)| *n != "panic"));
    assert_eq!(goal_edit::remove_where(&mut h.world, a, false, &is_panic), Some(0));

    // Add: a new goal reads last, with its priority.
    let entry = Entry::new(&climb_on_powder_snow::MODULE, 2, &SNOW);
    assert!(goal_edit::add(&mut h.world, a, entry));
    assert_eq!(mods(&h, a).last().copied(), Some((2, climb_on_powder_snow::MODULE.name)));
    // The same list on another animal is the same allocation.
    assert!(goal_edit::remove_where(&mut h.world, b, false, &is_panic).is_some());
    assert!(goal_edit::add(&mut h.world, b, entry));
    assert_eq!(h.world.get::<GoalSet>(a).unwrap().entries().as_ptr(), h.world.get::<GoalSet>(b).unwrap().entries().as_ptr());
    // Targets and goals read apart.
    assert!(goal_edit::list(&h.world, a, true).unwrap().iter().all(|(_, m, _)| goal_edit::is_target(m)));

    // The untouched animal has no edit state and its species entries.
    let c = ids[2];
    assert!(h.world.get::<GoalEdit>(c).is_none());

    // Reset: the species list is back, the edit state is gone, the untouched animal did not change.
    assert!(goal_edit::reset(&mut h.world, a));
    assert_eq!(mods(&h, a), species);
    // The slot a mod added stays (emptied): its edit state stays with it.
    assert!(h.world.get::<GoalEdit>(a).is_some());
    assert!(h.world.get::<GoalEdit>(c).is_none());
}
