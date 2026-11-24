//! The rider's keys and the fluid view of a boat through the bridge (`boat_input.rs`): the host writes the keys of the head seat, releases
//! the boat when the rider leaves, and reads whether the hull is under or in liquid for the HUD and the passenger.
use crate::bridge::host::EcsWorld;
use crate::boat::{Boat, Status};
use crate::boat_tick::Steered;
use crate::pilot::{self, PilotInput};
use crate::seat;
use crate::targeting::Who;
use bevy_ecs::entity::Entity;

/// What the host reads of a boat's hull: its head under the liquid, and its box in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoatStatus {
    pub under_water: bool,
    pub in_liquid: bool,
}

impl EcsWorld {
    /// The rider of `vehicle` holds the keys `left`, `right`, `up`, `down`; the boat wakes to steer. False when `vehicle` is not a boat.
    pub fn boat_input(&mut self, vehicle: u32, left: bool, right: bool, up: bool, down: bool) -> bool {
        let Some(e) = self.entity_of(vehicle) else { return false };
        let w = self.world_mut();
        if w.get::<Boat>(e).is_none() {
            return false;
        }
        let who = seat::riders_of(w, e).first().map(|&r| Who::Entity(r)).or_else(|| seat::players_of(w, e).first().map(|&p| Who::Player(p)));
        let Some(who) = who else { return false };
        pilot::set(w, e, PilotInput { who, forward: 0.0, strafe: 0.0, yaw: 0.0, pitch: 0.0, jump: 0.0, sprint: false, sneak: false, steer: false, keys: [left, right, up, down] });
        true
    }

    /// `handlePaddleBoat`: the paddle strokes a client reports for the boat `vehicle`; false when it is not a boat.
    pub fn boat_paddle(&mut self, vehicle: u32, paddles: [bool; 2]) -> bool {
        let Some(e) = self.entity_of(vehicle) else { return false };
        let Some(mut boat) = self.world_mut().get_mut::<Boat>(e) else { return false };
        boat.paddling = paddles;
        crate::bridge::view::mark(self.world_mut(), e);
        true
    }

    /// The rider left: the boat stops taking keys and may sleep once it lies still.
    pub fn boat_release(&mut self, e: Entity) {
        self.world_mut().entity_mut(e).remove::<(PilotInput, Steered)>();
    }

    /// The hull's status from the boat's last tick; None when `e` is not a boat.
    pub fn boat_status(&self, e: Entity) -> Option<BoatStatus> {
        let boat = self.world().get::<Boat>(e)?;
        Some(BoatStatus {
            under_water: matches!(boat.status, Status::UnderWater | Status::UnderFlowingWater),
            in_liquid: matches!(boat.status, Status::InWater | Status::UnderWater | Status::UnderFlowingWater),
        })
    }
}
