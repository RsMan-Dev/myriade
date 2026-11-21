//! `remove(KILLED)`: the mark makes a death silent (no animation) only inside its window, and an unmarked entity dies with its animation.
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::remove_killed::{mark, silent, WINDOW};

#[test]
fn the_mark_silences_a_death_only_inside_its_window() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let table = species_table().0;
    let sp = table.iter().find(|s| s.name == "minecraft:zombie").expect("zombie species");
    let e = spawn_species(&mut h.world, sp, 0.5, 0.5, 5);
    assert!(!silent(&h.world, e), "unmarked: the death is animated");
    mark(&mut h.world, e);
    assert!(silent(&h.world, e));
    h.world.resource_mut::<Clock>().0 += WINDOW + 1;
    assert!(!silent(&h.world, e), "the mark lapsed");
}
