//! What a primed TNT asks of the host: its landing (`Landed`, the host's fall on the block and HIT_GROUND) and its blast at the fuse's end
//! (`Explode`, credited to its owner). Only a TNT in the air is visited (`Moving`); a fuse is a deadline of the wheel (`tnt.rs`). In water
//! it lands on nothing: `fluid_update` forgets its fall there. The events queue in `TntOut`, read by `EcsWorld::drain_tnt`.
use crate::cow::*;
use crate::goals::Events;
use crate::host_out_tnt::TntEvent;
use crate::projectile::Blast;
use crate::rules::tnt::blast_point;
use crate::tnt::{Primed, POWER, SIZE};
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

/// The events of the primed TNTs since the host last read them, in order: one queue, empty at rest.
#[derive(Resource, Default)]
pub struct TntOut(pub Vec<TntEvent>);

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(tnt_step.after(travel).before(reindex));
}

/// The tick's end for the TNT in the air: one whose move ended on the ground lands, with the fall of the last tick in the air (the move that
/// lands resets `fall_distance`, so the landing reads the one kept in `Primed.fell`, as `falling_block` does). In water it does not land.
fn tnt_step(w: &mut World) {
    let mut landed = Vec::new();
    for (e, at, c, fs, mut p) in w.query_filtered::<(Entity, &Pos, &Contact, &FluidState, &mut Primed), With<Moving>>().iter_mut(w) {
        if fs.in_water {
            p.fell = 0.0;
        } else if !c.on_ground {
            p.fell = c.fall_distance;
        } else if p.fell > 0.0 {
            landed.push((e, at.0, p.fell));
            p.fell = 0.0;
        }
    }
    if landed.is_empty() {
        return;
    }
    let out = &mut w.get_resource_or_insert_with(TntOut::default).0;
    for (entity, at, fall) in landed {
        out.push(TntEvent::Landed { entity, at: [at.x, at.y, at.z], fall });
    }
}

/// The fuse's end (`PrimedTnt.tick` at 0): the TNT is removed, and the host is asked the blast at 0.0625 of its height (`PrimedTnt.explode`),
/// credited to its owner. The blast runs in the host (`EcsWorld::tnt_explode`, explosion.rs `explode_tnt`), not here.
pub fn explode(w: &mut World, e: Entity) -> Blast {
    let (power, owner) = w.get::<Primed>(e).map_or((POWER, None), |p| (p.power, p.owner));
    let at = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    let blast = Blast { at: blast_point(at, SIZE), radius: power, fire: false, owner, source: Some(e) };
    w.entity_mut(e).despawn();
    w.resource_mut::<Events>().exploded.push(blast);
    let c = blast.at;
    w.get_resource_or_insert_with(TntOut::default).0.push(TntEvent::Explode { at: [c.x, c.y, c.z], power, owner });
    blast
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::targeting::Who;

    fn tnt(w: &mut World, owner: Option<Who>, fell: f64, in_water: bool, on_ground: bool) -> Entity {
        let fluid = FluidState { in_water, ..Default::default() };
        let contact = Contact { on_ground, horizontal: false, fall_distance: 0.0, stuck: 0 };
        w.spawn((Pos(Vec3::new(0.5, 64.0, 0.5)), contact, fluid, Primed { fuse_at: 0, power: POWER, owner, fell }, Moving)).id()
    }

    #[test]
    fn a_blast_is_credited_to_the_tnt_owner_and_asks_the_host_once() {
        let mut w = World::new();
        w.insert_resource(Events::default());
        let owner = w.spawn_empty().id();
        let e = tnt(&mut w, Some(Who::Entity(owner)), 0.0, false, false);
        let b = explode(&mut w, e);
        assert!(w.get_entity(e).is_err());
        assert_eq!(b.owner, Some(Who::Entity(owner)));
        match w.resource::<TntOut>().0.as_slice() {
            [TntEvent::Explode { power, owner: o, .. }] => assert!(*power == POWER && *o == Some(Who::Entity(owner))),
            other => panic!("expected one Explode, got {other:?}"),
        }
    }

    #[test]
    fn a_tnt_landing_after_a_fall_asks_for_its_landing() {
        let mut w = World::new();
        let e = tnt(&mut w, None, 3.0, false, true);
        tnt_step(&mut w);
        match w.resource::<TntOut>().0.as_slice() {
            [TntEvent::Landed { entity, fall, .. }] => assert!(*entity == e && *fall == 3.0),
            other => panic!("expected one Landed, got {other:?}"),
        }
    }

    #[test]
    fn a_tnt_in_water_does_not_land() {
        let mut w = World::new();
        tnt(&mut w, None, 3.0, true, true);
        tnt_step(&mut w);
        assert!(w.get_resource::<TntOut>().is_none_or(|o| o.0.is_empty()));
    }
}
