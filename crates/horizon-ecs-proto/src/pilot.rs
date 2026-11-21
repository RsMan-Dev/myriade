//! The pilot's input of a vehicle (lots 70 and 84): the keys of the player who steers a boat, a minecart or a mount, held by one sparse
//! component on the vehicle, the only copy of it. The host sets it when the input changes and removes it when the rider leaves; the
//! mount, the boat and the minecart read it. Nothing is written for an unchanged input.
use crate::boat::Boat;
use crate::boat_tick::Steered;
use crate::cart_move;
use crate::cow::Moving;
use crate::mount::{Mount, RiderInput};
use crate::targeting::Who;
use bevy_ecs::prelude::*;

/// What the pilot asks of the vehicle this tick: `forward` above 0 is ahead, `strafe` above 0 is right, `jump` is the charge's result.
/// A boat reads `keys` (left, right, up, down): its up and down are separate keys, both can be held.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[component(storage = "SparseSet")]
pub struct PilotInput {
    pub who: Who,
    pub forward: f32,
    pub strafe: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub jump: f32,
    pub sprint: bool,
    pub sneak: bool,
    pub steer: bool,
    pub keys: [bool; 4],
}

/// Sets the pilot's input of `vehicle` and wakes the vehicle. False, and nothing written, when the vehicle already holds this input.
pub fn set(world: &mut World, vehicle: Entity, input: PilotInput) -> bool {
    if world.get::<PilotInput>(vehicle) == Some(&input) {
        return false;
    }
    world.entity_mut(vehicle).insert(input);
    if world.get::<Boat>(vehicle).is_some() {
        world.entity_mut(vehicle).insert((Steered, Moving));
    } else if world.get::<Mount>(vehicle).is_some() {
        world.entity_mut(vehicle).insert(Moving);
    }
    cart_move::wake(world, vehicle);
    true
}

/// The pilot left: its input is removed, and the boat stops being steered.
pub fn clear(world: &mut World, vehicle: Entity) {
    world.entity_mut(vehicle).remove::<(PilotInput, Steered)>();
}

/// The pilot's input of `vehicle`, None without a pilot.
pub fn input_of(world: &World, vehicle: Entity) -> Option<PilotInput> {
    world.get::<PilotInput>(vehicle).copied()
}

/// The rider's input a mount takes from its pilot.
pub fn as_rider_input(p: &PilotInput) -> RiderInput {
    RiderInput { forward: p.forward, strafe: p.strafe, yaw: p.yaw, pitch: p.pitch, jump: p.jump, sprint: p.sprint, steer: p.steer }
}

/// The intent of a minecart: `[strafe, forward]`, both at rest without a pilot.
pub fn cart_intent(p: &PilotInput) -> [f64; 2] {
    [f64::from(p.strafe), f64::from(p.forward)]
}
