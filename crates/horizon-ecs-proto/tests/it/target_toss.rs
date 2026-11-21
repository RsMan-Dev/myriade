//! `target_toss`: the strike adds `lift * (1 - resistance)` to the target's vertical velocity and sends the attack event.
use bevy_ecs::prelude::*;
use horizon_ecs_proto::behaviours::target_toss::{on_strike, Params, Toss};
use horizon_ecs_proto::cow::{spawn_species, Herd, Resist, SpeciesTable, Vel};
use horizon_ecs_proto::targeting::{set_target, Target, Who};

static P: Params = Params::DEFAULT;

fn pair() -> Option<(Herd, Entity, Entity)> {
    std::env::var_os("HORIZON_TEST_RESOURCES")?;
    let mut h = Herd::with_slab_awake(60, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let cow = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
    let golem = spawn_species(&mut h.world, &cow, 0.5, 0.5, 61);
    let prey = spawn_species(&mut h.world, &cow, 2.5, 0.5, 62);
    h.world.entity_mut(golem).insert(Toss(&P));
    set_target(&mut h.world, golem, Target::new(Who::Entity(prey), 200, u8::MAX));
    Some((h, golem, prey))
}

#[test]
fn the_target_is_tossed_by_the_lift_scaled_by_its_resistance() {
    let Some((mut h, golem, prey)) = pair() else { return };
    h.world.entity_mut(prey).insert(Resist(0.5));
    let before = h.world.get::<Vel>(prey).unwrap().0.y;
    on_strike(&mut h.world, golem, Who::Entity(prey), 7.0);
    let up = h.world.get::<Vel>(prey).unwrap().0.y - before;
    assert!((up - 0.4000000059604645 * 0.5).abs() < 1e-9, "{up}");
}
