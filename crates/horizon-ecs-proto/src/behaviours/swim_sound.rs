//! `Entity.getSwimSound` and `playSwimSound`: the water sound of an entity, its name from the species' data (`species/swim.rs`, given at
//! creation). The component is only on the entities whose name differs from `entity.generic.swim` (sparse); nothing here names a species.
use bevy_ecs::prelude::*;

/// The sound names of a species that splashes and swims (`entity.<species>.step_water`, `gallop_water`, `swim`).
#[derive(Clone, Copy, Debug)]
pub struct Sounds {
    pub step_water: &'static str,
    pub gallop_water: &'static str,
    pub swim: &'static str,
}

impl Sounds {
    /// One name for every water sound: the `getSwimSound` of a species that has a single one.
    pub const fn one(name: &'static str) -> Sounds {
        Sounds { step_water: name, gallop_water: name, swim: name }
    }
}

/// The water sounds of the entity, `AbstractHorse.gallopSoundCounter` (the steps a ridden one took), and the sound of a baby when the
/// species has one (`baby`, used while the entity has `GrowAt`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct SwimSounds {
    pub sounds: Sounds,
    pub steps: u32,
    pub baby: Option<&'static str>,
    pub law: Volume,
}

/// `playSwimSound`'s volume: `Entity` passes the speed on, the turtle's is x1.5, the skeleton horse's is 0.3 on the ground and `min(0.1, f * 25)` in water.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Volume {
    Plain,
    Turtle,
    SkeletonHorse,
}

/// `getSwimSound`: in the air it swims; on the ground an unridden one splashes, a ridden one counts its gallop: the first five
/// steps splash, then it gallops every third and swims in between.
pub fn swim_sound(s: &Sounds, on_ground: bool, ridden: bool, steps: &mut u32) -> &'static str {
    if !on_ground {
        return s.swim;
    }
    if !ridden {
        return s.step_water;
    }
    *steps += 1;
    let count = *steps;
    if count > 5 && count % 3 == 0 {
        s.gallop_water
    } else if count <= 5 {
        s.step_water
    } else {
        s.swim
    }
}

/// `playSwimSound`'s volume for the speed `f` (see [`Volume`]).
pub fn splash_volume(law: Volume, on_ground: bool, f: f32) -> f32 {
    match law {
        Volume::Plain => f,
        Volume::Turtle => f * 1.5,
        Volume::SkeletonHorse if on_ground => 0.3,
        Volume::SkeletonHorse => (f * 25.0).min(0.1),
    }
}
