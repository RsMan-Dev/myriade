//! `Goat` (`mobs/goat.rs:60-65`, `GoatAi`): the farm animal base (panic, breed, tempt with wheat, follow the parent, stroll, look), the ram (2) and
//! the long jump (2: 600 to 1200 ticks between two, a box of 5 x 5, any landing spot, only while the ram cools down). Left out: the screaming
//! goat's horns, the lowered-head pose of the ram, the effects of the jump (`jump_boost` 0).
use super::{templates::farm_animal, Change, Def, Section};
use crate::behaviours::{long_jump, ram, Entry};
use crate::cow::{SpeciesId, SpeciesTable};
use crate::goals::with_rng;
use bevy_ecs::prelude::*;

/// `Goat.finalizeSpawn` on a natural spawn: a screaming goat 2 percent of the time (`finalize.rs` `goat_spawn`).
pub const SCREAMING_CHANCE: f64 = 0.02;

/// `Goat.isScreamingGoat`: the screaming goat, the one with the sounds, the horns and the jump's boost. Zero bytes, set at birth on
/// the drawn goats only; saved as `IsScreamingGoat` when it is there.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Screaming;

pub static DEF: Def = Def {
    name: "minecraft:goat",
    reference: Some(&farm_animal::DEF),
    changes: &[
        Change::Add(Section::Goals, Entry::new(&ram::MODULE, 2, &ram::Params::DEFAULT)),
        Change::Add(Section::Goals, Entry::new(&long_jump::MODULE, 2, &long_jump::Params { cooldown: (600, 1200), width: 5, height: 5, preferred: None, swim_rules: false, gated_by_ram: true, ..long_jump::Params::DEFAULT })),
    ],
    rules: &[crate::rules::RuleEntry::new(&crate::behaviours::held_swap::HELD_SWAP, &crate::behaviours::held_swap::SwapParams { from: "minecraft:bucket", to: "minecraft:milk_bucket", adult_only: true, takes_creature: false })],
    screaming: Some(SCREAMING_CHANCE),
    ..Def::EMPTY
};

/// `Goat.getBreedOffspring`: the baby screams if the parent a coin toss picks does, or on the chance of its species (drawn only when
/// that parent does not scream, as the Mob's `||`). A species without the chance gives nothing.
pub fn inherit_screaming(w: &mut World, baby: Entity, a: Entity, b: Entity) {
    let Some(chance) = w.get::<SpeciesId>(baby).and_then(|i| w.resource::<SpeciesTable>().0[i.0 as usize].screaming) else { return };
    let parent = if with_rng(w, a, |r| r.next_int(2) == 0) { a } else { b };
    if w.get::<Screaming>(parent).is_some() || with_rng(w, a, |r| r.next_f64() < chance) {
        w.entity_mut(baby).insert(Screaming);
    }
}
