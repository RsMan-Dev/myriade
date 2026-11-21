//! What a player rides and steers (`PlayerView.vehicle` of the `Mob`): a boat or a nautilus, for the creatures that follow it. The host writes it
//! each tick it changes (`set`), as it writes `Online`; the creatures that ride (`mount.rs`) are entities of their own and need none of this.
//! Event: `Events.vehicle_moved` (the player's id) when the vehicle moved in its last tick, or stopped, or went away: the modules that follow it
//! (`behaviours/follow_player_vehicle.rs`) are woken by it, never by a scan.
use crate::goals::Events;
use crate::rules::vehicle::horizontally_moved;
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Vehicle {
    /// A nautilus (else a boat).
    pub nautilus: bool,
    pub bounds: Aabb,
    /// `Entity.getKnownSpeed`: what it moved in its last tick.
    pub velocity: Vec3,
    /// The rider's yaw (the directions the followers take are the rider's) and whether it is a passenger still.
    pub yaw: f32,
    pub passenger: bool,
}
impl Vehicle {
    /// `Entity.hasMovedHorizontallyRecently` of the rider: the horizontal speed is over 1e-5.
    pub fn moved(&self) -> bool {
        horizontally_moved(self.velocity)
    }
}
#[derive(Resource, Default)]
pub struct Vehicles {
    list: Vec<Option<Vehicle>>,
}
impl Vehicles {
    pub fn get(&self, player: u32) -> Option<Vehicle> {
        self.list.get(player as usize).copied().flatten()
    }
    /// The players that ride something, with it.
    pub fn riders(&self) -> impl Iterator<Item = (u32, Vehicle)> + '_ {
        self.list.iter().enumerate().filter_map(|(i, v)| v.map(|v| (i as u32, v)))
    }
}
/// The player rides `v` (or nothing): the event is raised when it moves, stops or goes.
pub fn set(w: &mut World, player: u32, v: Option<Vehicle>) {
    let mut all = w.get_resource_or_insert_with(Vehicles::default);
    let i = player as usize;
    if all.list.len() <= i {
        all.list.resize(i + 1, None);
    }
    let old = all.list[i];
    all.list[i] = v;
    let moved = |v: &Option<Vehicle>| v.is_some_and(|v| v.moved());
    if moved(&old) || moved(&v) || old.is_some() != v.is_some() {
        w.resource_mut::<Events>().vehicle_moved.push(player);
    }
}
