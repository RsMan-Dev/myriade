//! What the movement of an entity emits, where its type differs from `Entity`'s defaults: `getMovementEmission` (NONE or SOUNDS/EVENTS/ALL: for
//! the game events only NONE differs), `nextStep()`, `isFlapping()` and `dampensVibrations()`. A species without this entry has the defaults.
//! The module has no goal: `move_events` reads `Cfg`.
use super::{Inst, Module, params};
use bevy_ecs::prelude::*;

/// `isFlapping()` of the types that flap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Flap {
    Never,
    /// `!onGround()` (the allay).
    Airborne,
    /// `tickCount % period == 0` (bat, bee while flying, vex), plus the entity's own offset for the phantom (`getUniqueFlapTickOffset`).
    Every { period: u32, flying_only: bool, offset: bool },
}

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `getMovementEmission().emitsEvents()`: false is NONE (the shulker).
    pub events: bool,
    /// `getMovementEmission().emitsSounds()`: false is EVENTS (the squid, the bat, the silverfish) and NONE (the shulker).
    pub sounds: bool,
    /// `nextStep()` is `moveDist + step_gap` (turtle 0.15, strider 0.6, warden 0.55); 0 is the default, `floor(moveDist) + 1`.
    pub step_gap: f32,
    pub flap: Flap,
    /// `dampensVibrations()` (the warden).
    pub dampens: bool,
}
impl Params {
    pub const DEFAULT: Params = Params { events: true, sounds: true, step_gap: 0.0, flap: Flap::Never, dampens: false };
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Cfg(pub &'static Params);

pub static MODULE: Module = Module { name: "move_emission", flags: 0, equip, ..Module::DEFAULT };

fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Cfg(params::<Params>(p)));
}
