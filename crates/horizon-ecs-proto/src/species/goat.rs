//! `Goat` (`mobs/goat.rs:60-65`, `GoatAi`): the farm animal base (panic, breed, tempt with wheat, follow the parent, stroll, look), the ram (2) and
//! the long jump (2: 600 to 1200 ticks between two, a box of 5 x 5, any landing spot, only while the ram cools down). Left out: the screaming
//! goat's horns, the lowered-head pose of the ram, the effects of the jump (`jump_boost` 0).
use super::{templates::farm_animal, Change, Def, Section};
use crate::behaviours::spawn_draw::{Draw, Draws, Kind, Store, When};
use crate::behaviours::{animal_panic, long_jump, panic, ram, Entry};
use crate::cow::{SpeciesId, SpeciesTable};
use crate::goals::with_rng;
use crate::rules::spawn_draw::Roll;
use bevy_ecs::prelude::*;

/// `Goat.finalizeSpawn` on a natural spawn: a screaming goat 2 percent of the time (`finalize.rs` `goat_spawn`).
pub const SCREAMING_CHANCE: f64 = 0.02;
/// `Goat.finalizeSpawn`: a horn lost 10 percent of the time on a natural spawn, adults only.
pub const LOST_HORN_CHANCE: f64 = 0.1;

/// `Goat.isScreamingGoat`: the screaming goat, the one with the sounds, the horns and the jump's boost. Zero bytes, set at birth on
/// the drawn goats only; saved as `IsScreamingGoat` when it is there.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet", on_insert = crate::bridge::view::touched, on_remove = crate::bridge::view::touched)]
pub struct Screaming;

/// `RAMS_SCREAMER` and the screaming sounds, picked by the `Screaming` marker.
pub const SCREAMING_RAM: ram::Variant = ram::Variant {
    is: is_screaming,
    cooldown: (100, 300),
    prepare_sound: "entity.goat.screaming.prepare_ram",
    impact_sound: "entity.goat.screaming.ram_impact",
    shed: Some(SCREAMING_SHED),
};

/// The horns a ram snaps off: the instrument of a regular goat, and the screaming goat's.
const REGULAR_SHED: ram::ShedDef = ram::ShedDef {
    items: &["minecraft:ponder_goat_horn", "minecraft:sing_goat_horn", "minecraft:seek_goat_horn", "minecraft:feel_goat_horn"],
    break_sound: "entity.goat.horn_break",
};
const SCREAMING_SHED: ram::ShedDef = ram::ShedDef {
    items: &["minecraft:admire_goat_horn", "minecraft:call_goat_horn", "minecraft:yearn_goat_horn", "minecraft:dream_goat_horn"],
    break_sound: "entity.goat.horn_break",
};

pub fn is_screaming(w: &World, e: Entity) -> bool {
    w.get::<Screaming>(e).is_some()
}

pub static DEF: Def = Def {
    load_attrs: &[crate::attr_mod::LoadAttr { name: "attack_damage", base: Some(1.0), when: crate::attr_mod::LoadWhen::Baby }],
    name: "minecraft:goat",
    reference: Some(&farm_animal::DEF),
    // `GoatAi.initCoreActivity`: `AnimalPanic(2.0)`, the panic of the farm animals' goal swapped for it (100 to 120 ticks, the window of 40).
    changes: &[
        Change::Remove { module: &panic::MODULE, nth: 0 },
        Change::Add(Section::Goals, Entry::new(&animal_panic::MODULE, 1, &animal_panic::Params::DEFAULT)),
        Change::Add(Section::Goals, Entry::new(&ram::MODULE, 2, &ram::Params { marked: Some(SCREAMING_RAM), shed: Some(REGULAR_SHED), ..ram::Params::DEFAULT })),
        Change::Add(Section::Goals, Entry::new(&long_jump::MODULE, 2, &long_jump::Params { cooldown: (600, 1200), width: 5, height: 5, preferred: None, swim_rules: false, gated_by_ram: true, ..long_jump::Params::DEFAULT })),
    ],
    rules: &[crate::rules::RuleEntry::new(&crate::behaviours::held_swap::HELD_SWAP, &crate::behaviours::held_swap::SwapParams { from: "minecraft:bucket", to: "minecraft:milk_bucket", adult_only: true, takes_creature: false, sound: "minecraft:entity.goat.milk", alt: Some((is_screaming, "minecraft:entity.goat.screaming.milk")) })],
    screaming: Some(SCREAMING_CHANCE),
    lost_horn: Some(LOST_HORN_CHANCE),
    draws: Some(&DRAWS),
    ..Def::EMPTY
};

/// `GoatAi.initMemories`: the long jump's cooldown (600 to 1200 ticks) and the ram's (600 to 6000), both `UniformInt` drawn at the spawn.
static DRAWS: Draws = Draws {
    group: None,
    list: &[
        Draw { when: When::Before, kind: Kind::Plain(Roll::Between { min: 600, max: 1200 }), store: Store::Call(long_jump::spawn_cooldown) },
        Draw { when: When::Before, kind: Kind::Plain(Roll::Between { min: 600, max: 6000 }), store: Store::Call(ram_memory) },
    ],
};

fn ram_memory(w: &mut World, e: Entity, v: u32) {
    ram::set_cooldown(w, e, v as i32);
}

/// `Goat.getBreedOffspring`: the baby screams if the parent a coin toss picks does, or on the chance of its species (drawn only when
/// that parent does not scream, as the Mob's `||`). A species without the chance gives nothing.
pub fn inherit_screaming(w: &mut World, baby: Entity, a: Entity, b: Entity) {
    let Some(chance) = w.get::<SpeciesId>(baby).and_then(|i| w.resource::<SpeciesTable>().0[i.0 as usize].screaming) else { return };
    let parent = if with_rng(w, a, |r| r.next_int(2) == 0) { a } else { b };
    if w.get::<Screaming>(parent).is_some() || with_rng(w, a, |r| r.next_f64() < chance) {
        w.entity_mut(baby).insert(Screaming);
    }
}
