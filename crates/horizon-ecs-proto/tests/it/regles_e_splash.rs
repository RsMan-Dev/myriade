//! `Entity.doWaterSplashSound`: entering water makes the generic splash for every creature, its volume the speed and its pitch drawn from
//! the entity's own stream; nothing is heard while the entity is `Silent`. Expected name from the jar (`GENERIC_SPLASH` in both branches).
use crate::common;
use common::*;
use horizon_ecs_proto::behaviours::elder_curse::Silent;
use horizon_ecs_proto::cow::{spawn_species, ActiveUntil, Herd, SpeciesTable, Vel};
use horizon_ecs_proto::goals::Events;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_core::geometry::Vec3;

fn splashes(h: &mut Herd) -> usize {
    h.world
        .resource_mut::<Events>()
        .host
        .drain()
        .filter(|e| matches!(e, HostEvent::Sound { name, .. } if *name == "entity.generic.splash"))
        .count()
}

/// A zombie falling fast into a water column makes one generic splash; the silent one makes none.
#[test]
fn a_fast_water_entry_splashes_generically_for_every_mob() {
    if !have_data() {
        return;
    }
    for silent in [false, true] {
        let mut h = Herd::with_slab_awake(8, true, false);
        h.fill_water(4);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let zombie = *table.iter().find(|s| s.name == "minecraft:zombie").unwrap();
        let e = spawn_species(&mut h.world, &zombie, 0.5, 0.5, 5);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.entity_mut(e).insert(Vel(Vec3::new(0.0, -2.0, 0.0)));
        if silent {
            h.world.entity_mut(e).insert(Silent);
        }
        h.world.get_mut::<horizon_ecs_proto::cow::Pos>(e).unwrap().0.y = 7.0;
        let mut heard = 0;
        for _ in 0..30 {
            h.tick();
            heard += splashes(&mut h);
        }
        if silent {
            assert_eq!(heard, 0, "a silent creature makes no splash");
        } else {
            assert_eq!(heard, 1, "one generic splash on entering the water");
        }
    }
}
