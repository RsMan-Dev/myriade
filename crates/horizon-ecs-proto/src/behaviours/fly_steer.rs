//! The flying steer of a happy ghast (`happy_ghast.rs` `getRiddenInput`, `travel`): the first rider steers with its forward and strafe
//! input along its yaw; jump lifts it, otherwise it sinks slowly; the passenger has no input. `mount::Steer` has no flying variant, so
//! this drive replaces `mount::ridden_drive` for a mount that carries `FlyingSteer` (the socle skips it there, see the notes).
//! DOUTE : les constantes de vitesse et de montée viennent de l'espèce (`Params`) ; leurs valeurs vanilles ne sont pas relues ici.
//! DOUTE : le sens « avant » suit le lacet (`-sin`, `cos`), comme `Entity.getViewVector` ; à vérifier au test de pilotage.
use crate::cow::*;
use crate::mount::{Mount, RiderInput};
use crate::rules::fly_steer::steer_velocity;
use bevy_ecs::prelude::*;

/// Marks a mount that flies under its rider's steer.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct FlyingSteer;

/// The steer's constants (species): the speed per tick under full forward input, and the lift while jumping.
#[derive(Component, Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    pub climb: f64,
    pub sink: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 0.1, climb: 0.2, sink: 0.05 };
}

/// One tick of the steer: the first rider's input moves the flier; a passenger's input is ignored.
pub fn drive_one(mount: &Mount, input: RiderInput, yaw: &Yaw, vel: &mut Vel, p: &Params) {
    if mount.rider.is_none() {
        return;
    }
    vel.0 = steer_velocity(f64::from(input.forward), f64::from(input.strafe), f64::from(input.jump), f64::from(yaw.0), p.speed, p.climb, p.sink);
}

/// The rule that makes a mount fly under its rider's steer, with the species' constants.
pub static RULE: crate::rules::Rule = crate::rules::Rule { name: "fly_steer", equip, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };
fn equip(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync)) {
    let p = *crate::behaviours::params::<Params>(p);
    w.entity_mut(e).insert((FlyingSteer, p));
}
