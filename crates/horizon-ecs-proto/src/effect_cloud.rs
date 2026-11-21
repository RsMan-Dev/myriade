//! The potion cloud's spec (`AreaEffectCloud` of a potion, `items/potions.rs`): what `spawn_cloud` takes, on the jar's defaults. A live cloud
//! keeps the same struct as its state, run by `cloud_tick`; a thrown lingering potion makes one too (`projectile.rs`).
use crate::cloud_tick::{INFINITE, REAPPLICATION, WAIT};
use crate::effects::{Brew, Kind};
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::nbt::Compound;

/// What a potion cloud gives and how it changes: the radius, its change a tick, what a victim adds to the radius and to the duration, the wait,
/// the duration (-1: none), the delay before a victim is dosed again, the potion (`effects::POTIONS`) with its colour and custom particle as
/// saved, the flags of the effects it shows hidden or ambient, the scale of its timed durations, and the owner the damage is credited to
/// (`None`: the cloud). Sparse: clouds are few.
#[derive(Component, Clone, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct CloudSpec {
    pub radius: f32,
    pub radius_per_tick: f32,
    pub radius_on_use: f32,
    pub wait: i32,
    pub duration: i32,
    pub duration_on_use: i32,
    pub reapplication: i32,
    pub duration_scale: f32,
    /// The age the cloud was saved at (`Age`): its own time is this plus its clock, which a load starts at 0.
    pub age_base: i32,
    pub potion: &'static str,
    /// The `custom_effects` of the potion contents, given after the potion's own brews.
    pub custom: Vec<Brew>,
    pub color: Option<i32>,
    pub custom_particle: Option<Compound>,
    pub hidden: Vec<Kind>,
    pub ambient: Vec<Kind>,
    pub owner: Option<Who>,
}

impl CloudSpec {
    /// A cloud of `potion` at `radius` with the jar's defaults; the caller sets the fields it changes.
    pub fn new(radius: f32, potion: &'static str, owner: Option<Who>) -> Self {
        Self {
            radius,
            radius_per_tick: 0.0,
            radius_on_use: 0.0,
            wait: WAIT,
            duration: INFINITE,
            duration_on_use: 0,
            reapplication: REAPPLICATION,
            duration_scale: 1.0,
            age_base: 0,
            potion,
            custom: Vec::new(),
            color: None,
            custom_particle: None,
            hidden: Vec::new(),
            ambient: Vec::new(),
            owner,
        }
    }
}
