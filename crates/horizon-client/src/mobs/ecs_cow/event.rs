//! What a cow's tick asks the world to do: only what another cow (or the world's own pass) acts on.
//! Sounds, particles and game events of `MobEvent` have no listener in this prototype (their random
//! draws are still made, in the same places).
use crate::mobs::Who;

#[derive(Clone, Debug)]
pub enum CowEvent {
    /// `Entity.push` of another cow (applied at its next tick).
    Push { target: Who, velocity: [f64; 3] },
    /// `BreedGoal.breed`: the child, the experience (`BreedExperience`) and the parents.
    Breed { parent: u32, partner: u32, parent_variant: bool, experience: Option<u32> },
}
