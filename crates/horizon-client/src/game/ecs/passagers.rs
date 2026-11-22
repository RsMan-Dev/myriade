//! Mobs riding mobs in the ECS: a mount is a write of the ECS's rider and seat (`mount::ride`, `mount::dismount`), and the seats are
//! placed by the ECS (`mount::seats`), so no host code copies a rider's position at each tick. The chains of riders are walked
//! from a vehicle by its `Mount.rider`, never by a sweep over the entities.
use super::super::Game;
use bevy_ecs::world::World;
use horizon_client::ecs_bridge::EcsLevel;
use horizon_ecs_proto::{
    bridge::ids::id_of,
    mount::{self, Mount, Riding},
    seat::{self, Seated},
    targeting::Who,
};
use std::collections::HashSet;

/// Longest chain of riders walked from a vehicle: a rider of a rider, bounded as `Entity.getPassengers` is.
const CHAIN_LIMIT: usize = 64;

/// The network id of the mob `who` names; None for a player or an entity the bridge does not follow.
fn mob_id(w: &World, who: Who) -> Option<u32> {
    match who {
        Who::Entity(e) => id_of(w, e),
        Who::Player(_) => None,
    }
}

/// `Entity.startRiding` in the ECS: `rider` takes a seat of `vehicle`; false when either is not in the ECS or no seat is free.
pub(super) fn mount_in(ecs: &mut EcsLevel, rider: u32, vehicle: u32) -> bool {
    let (Some(r), Some(v)) = (ecs.entity_of(rider), ecs.entity_of(vehicle)) else {
        return false;
    };
    let Some(w) = ecs.world_mut() else {
        return false;
    };
    if w.get::<Mount>(v).is_none() {
        return seat::board(w, r, v, None);
    }
    ecs.ride(v, Who::Entity(r))
}

/// `Entity.stopRiding` in the ECS: `rider` gets off its vehicle, from the first seat or the second; false when it rides nothing.
pub(super) fn dismount_in(ecs: &mut EcsLevel, rider: u32) -> bool {
    let Some(r) = ecs.entity_of(rider) else {
        return false;
    };
    let Some(w) = ecs.world_mut() else {
        return false;
    };
    if w.get::<Seated>(r).is_some() {
        return seat::leave(w, r).is_some();
    }
    let Some(v) = w.get::<Riding>(r).map(|m| m.0) else {
        return false;
    };
    if w.get::<Mount>(v).is_some_and(|m| m.rider == Some(Who::Entity(r))) {
        mount::dismount(w, v).is_some()
    } else {
        mount::dismount_passenger(w, v).is_some()
    }
}

/// The mob that `rider` rides in the ECS, by its network id.
pub(super) fn vehicle_of_in(ecs: &EcsLevel, rider: u32) -> Option<u32> {
    let w = ecs.world()?;
    let r = ecs.entity_of(rider)?;
    let v = match w.get::<Riding>(r) {
        Some(m) => m.0,
        None => w.get::<Seated>(r)?.vehicle,
    };
    id_of(w, v)
}

/// The first rider of the vehicle `vehicle` in the ECS, by its network id.
pub(super) fn rider_of_in(ecs: &EcsLevel, vehicle: u32) -> Option<u32> {
    let w = ecs.world()?;
    let v = ecs.entity_of(vehicle)?;
    match mount::rider_of(w, v) {
        Some(who) => mob_id(w, who),
        None => seat::riders_of(w, v).first().and_then(|&e| id_of(w, e)),
    }
}

/// The arrival place of `rider` in its vehicle (`Seated.order`); 0 for a rider on a mount's chain, which keeps no seat.
pub(super) fn vehicle_order_in(ecs: &EcsLevel, rider: u32) -> Option<i64> {
    let w = ecs.world()?;
    let r = ecs.entity_of(rider)?;
    Some(w.get::<Seated>(r).map_or(0, |s| i64::from(s.order)))
}

/// The riders below each of `roots`, down their chains of `Mount.rider`: the ids of the mobs whose vehicle chain reaches a root.
pub(super) fn riders_below_in(ecs: &EcsLevel, roots: impl Iterator<Item = u32>) -> HashSet<u32> {
    let mut below = HashSet::new();
    for root in roots {
        let mut vehicle = root;
        for _ in 0..CHAIN_LIMIT {
            let Some(rider) = rider_of_in(ecs, vehicle) else {
                break;
            };
            if !below.insert(rider) {
                break;
            }
            vehicle = rider;
        }
    }
    below
}

impl Game<'_> {
    /// The function event of a mount between two mobs: writes the ECS's seat, the ECS places the rider.
    pub(in crate::game) fn ecs_mount(&mut self, rider: u32, vehicle: u32) -> bool {
        mount_in(&mut self.ecs, rider, vehicle)
    }

    /// The function event of a dismount: the ECS's rider gets off its vehicle.
    pub(in crate::game) fn ecs_dismount(&mut self, rider: u32) {
        dismount_in(&mut self.ecs, rider);
    }

    /// The mob the ECS seats `rider` in, by its network id.
    pub(in crate::game) fn ecs_vehicle_of(&self, rider: u32) -> Option<u32> {
        vehicle_of_in(&self.ecs, rider)
    }

    /// The mob the ECS seats on `vehicle`, by its network id; None when no mob rides it.
    pub(in crate::game) fn ecs_rider_of(&self, vehicle: u32) -> Option<u32> {
        rider_of_in(&self.ecs, vehicle)
    }

    /// The place of the mob `rider` in the ECS seats, as `vehicle_order` gave it; None when it is not in the ECS.
    pub(in crate::game) fn ecs_vehicle_order(&self, rider: u32) -> Option<i64> {
        vehicle_order_in(&self.ecs, rider)
    }

    /// The riders below the vehicles `roots`, read down the ECS chains.
    pub(in crate::game) fn ecs_riders_below(&self, roots: impl Iterator<Item = u32>) -> HashSet<u32> {
        riders_below_in(&self.ecs, roots)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::prelude::Entity;
    use horizon_client::mobs::world::leaked_test_world_mut;
    use horizon_ecs_proto::bridge::host::HostSetup;
    use std::path::PathBuf;

    #[test]
    fn mounting_then_dismounting_leaves_the_initial_state() {
        let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
        let setup = HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 };
        let tags = PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
        let mut ecs = EcsLevel::new(&setup, mw, types, &tags);
        let horse: Entity = ecs.spawn(mw, "minecraft:horse", [0.5, 1.0, 0.5], None).expect("a known species");
        let cow: Entity = ecs.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
        let mut next = 100;
        let mut alloc = || {
            next += 1;
            next
        };
        let (h, c) = (ecs.id_for(horse, &mut alloc), ecs.id_for(cow, &mut alloc));
        assert_eq!(ecs.world().and_then(|w| w.get::<Mount>(horse)).map(|m| m.rider), Some(None));

        assert!(mount_in(&mut ecs, c, h));
        assert_eq!(vehicle_of_in(&ecs, c), Some(h));
        assert_eq!(rider_of_in(&ecs, h), Some(c));
        assert_eq!(riders_below_in(&ecs, [h].into_iter()), HashSet::from([c]));

        assert!(dismount_in(&mut ecs, c));
        let w = ecs.world().expect("an active level");
        assert_eq!(w.get::<Mount>(horse).map(|m| m.rider), Some(None));
        assert!(w.get::<Riding>(cow).is_none());
        assert!(riders_below_in(&ecs, [h].into_iter()).is_empty());
    }
}
