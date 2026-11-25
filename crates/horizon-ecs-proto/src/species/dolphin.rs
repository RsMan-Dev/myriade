//! `Dolphin`: its swim: water-bound navigation that may breach, `SmoothSwimmingMoveControl(85, 10, 0.02, 0.1, true)`, the push at the mob's speed.
//! Goals of `aquatic.rs:166-210`; the target is the hurt-by of priority 1 with an alert (the `Mob` ignores the guardians' hits: `suivi/ecs-ecarts.md`).
use super::Def;
use crate::behaviours::avoid_entity::{self, Avoid};
use crate::behaviours::{follow_player_vehicle, hurt_by, melee, item_pickup, move_to_item, play_with_items, swim_to_treasure, water_jump, breath_air, fish_swim, look_at_player, random_look, swim_with_player, try_find_water, Cap, Entry};
use crate::locomotion::{swim, Medium};

/// `ItemTags.FISHES` (`tags/item/fishes.json`).
const FISHES: &[&str] = &["minecraft:cod", "minecraft:cooked_cod", "minecraft:salmon", "minecraft:cooked_salmon", "minecraft:pufferfish", "minecraft:tropical_fish"];

pub static DEF: Def = Def {
    name: "minecraft:dolphin",
    targets: &[Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert: true, ..hurt_by::Params::DEFAULT })],
    goals: &[
        Entry::new(&breath_air::MODULE, 0, &breath_air::Params::DEFAULT),
        Entry::new(&try_find_water::MODULE, 0, &try_find_water::DOLPHIN),
        Entry::new(&swim_to_treasure::MODULE, 1, &swim_to_treasure::Params { fish: FISHES, ..swim_to_treasure::Params::DEFAULT }),
        Entry::new(&swim_with_player::MODULE, 2, &swim_with_player::Params::DEFAULT),
        Entry::new(&fish_swim::MODULE, 4, &fish_swim::Params { speed: 1.0, interval: 10 }),
        Entry::new(&random_look::MODULE, 4, &random_look::Params::DEFAULT),
        Entry::new(&water_jump::MODULE, 5, &water_jump::Params::DEFAULT),
        Entry::new(&melee::MODULE, 6, &melee::Params { speed: 1.2_f32 as f64, follow_unseen: true, ..melee::Params::DEFAULT }),
        Entry::new(&move_to_item::MODULE, 7, &move_to_item::Params::DEFAULT),
        Entry::new(&item_pickup::MODULE, 7, &item_pickup::Params::DEFAULT),
        Entry::new(&play_with_items::MODULE, 8, &play_with_items::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 5, &look_at_player::Params { range: 6.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&avoid_entity::MODULE, 9, &avoid_entity::Params { what: Avoid::Named(&["minecraft:guardian", "minecraft:elder_guardian"]), max_dist: 8.0, walk: 1.0, sprint: 1.0, ..avoid_entity::Params::DEFAULT }),
        Entry::new(&follow_player_vehicle::MODULE, 8, &follow_player_vehicle::Params::BOAT),
        Entry::new(&follow_player_vehicle::MODULE, 8, &follow_player_vehicle::Params::NAUTILUS),
    ],
    rules: &[crate::rules::RuleEntry::new(&crate::rules::bite_sound::BITE, &"entity.dolphin.attack"), crate::rules::RuleEntry::new(&crate::cow::OWN_AIR, &crate::cow::OwnAirParams { max: 4800, full_refill: true, nautilus: false, dry: false }), crate::rules::RuleEntry::new(&crate::moisture::MOISTURE, &())],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion]),
    locomotion: &[(Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params { needs_water: false, breach: true, control: swim::Control::Smooth { max_turn_x: 85.0, max_turn_y: 10.0, in_water: 0.02, outside: 0.1, gravity: true }, push: swim::Push::Speed, sinks: true, ..swim::Params::DEFAULT }))],
    spawn: Some(crate::spawning::SpawnDef { category: crate::spawning::Category::WaterCreature, place: crate::spawning::Place::InWater, rule: crate::spawning::Rule::SurfaceWater, cluster: 4, far: false, peaceful: true, liquid: true, despawn: crate::despawn::DespawnDef::MOB }), despawn: Some(crate::despawn::DespawnDef::MOB),
    ..Def::EMPTY
};
