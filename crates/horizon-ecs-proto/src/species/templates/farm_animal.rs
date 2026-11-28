//! The base of the farm animals: float, panic, breed, tempt, follow the parent, stroll and look (`Cow.registerGoals`).
//! Each species names what differs, as `changes`.
use crate::behaviours::{ambient_sound, breed, float, follow_parent, look_at_player, panic, random_look, stroll, tempt, Entry};
use crate::species::Def;

pub const FLOAT: Entry = Entry::new(&float::MODULE, 0, &float::Params::DEFAULT);
pub const WANDER: Entry = Entry::new(&stroll::MODULE, 5, &stroll::Params::DEFAULT);
pub const LOOK: Entry = Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params::DEFAULT);
pub const RANDOM_LOOK: Entry = Entry::new(&random_look::MODULE, 7, &random_look::Params::DEFAULT);

pub static DEF: Def = Def {
    name: "template:farm_animal",
    reference: None,
    targets: &[],
    goals: &[
        FLOAT,
        Entry::new(&panic::MODULE, 1, &panic::Params::DEFAULT),
        Entry::new(&breed::MODULE, 2, &breed::Params { foods: &["minecraft:wheat"], ..breed::Params::DEFAULT }),
        Entry::new(&tempt::MODULE, 3, &tempt::Params { speed: 1.25, items: &["minecraft:wheat"], ..tempt::Params::DEFAULT }),
        Entry::new(&follow_parent::MODULE, 4, &follow_parent::Params { speed: 1.25 }),
        WANDER,
        LOOK,
        RANDOM_LOOK,
        Entry::new(&ambient_sound::MODULE, 9, &ambient_sound::Params { interval: Some(120), choose: Some(ambient), ..ambient_sound::Params::DEFAULT }),
    ],
    changes: &[],
    rules: &[crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 1, max: 3 })],
    caps: None,
    brain: crate::brain::BrainDef::EMPTY,
    locomotion: &[],
    spawn: Some(crate::spawning::SpawnDef::ANIMAL), despawn: Some(crate::despawn::DespawnDef::KEPT),
    ..Def::EMPTY
};

/// `getAmbientSound` of the farm animals: the species' own, the variant's for a moody cow, a big or mini pig, a picky chicken; none for the rest.
fn ambient(w: &mut bevy_ecs::prelude::World, e: bevy_ecs::prelude::Entity) -> &'static str {
    let variant = w.get::<crate::behaviours::variant::SoundVariant>(e).map_or("", |v| v.0.as_str());
    match (crate::area::species_name(w, e), variant) {
        ("minecraft:cow" | "minecraft:mooshroom", "minecraft:moody") => "entity.cow_moody.ambient",
        ("minecraft:cow" | "minecraft:mooshroom", _) => "entity.cow.ambient",
        ("minecraft:pig", "minecraft:big") => "entity.pig_big.ambient",
        ("minecraft:pig", "minecraft:mini") => "entity.pig_mini.ambient",
        ("minecraft:pig", _) => "entity.pig.ambient",
        ("minecraft:sheep", _) => "entity.sheep.ambient",
        ("minecraft:chicken", "minecraft:picky") => "entity.chicken_picky.ambient",
        ("minecraft:chicken", _) => "entity.chicken.ambient",
        ("minecraft:goat", _) if crate::species::goat::is_screaming(w, e) => "entity.goat.screaming.ambient",
        ("minecraft:goat", _) => "entity.goat.ambient",
        _ => "",
    }
}
