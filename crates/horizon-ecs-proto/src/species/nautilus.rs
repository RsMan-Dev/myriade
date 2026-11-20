//! `Nautilus` (`mobs/nautilus.rs:39-71`), the swimmer of the charge: `SmoothSwimming(85, 10, 0.011, 0, true)`; goals: the panic at 1.6
//! (0), the breed at 0.4 (1), the tempt at 1.3 (2), the charge at 0.6 (3), the swimming stroll every tick (4: `fish_swim`).
//! The foods are the data tag `nautilus_food` (the fishes and four buckets), listed by name. The tempt stops 3.5 blocks from the
//! player whatever the age (the `Mob` uses 2.5 for a baby: `suivi/ecs-ecarts.md`). The target rule (`find_target`, the anger, the
//! pufferfish) is still set by the test.
use super::Def;
use crate::behaviours::{animal_panic, breed, charge_attack, fish_swim, tempt, Cap, Entry};
use crate::locomotion::{swim, Medium};

/// `tags/item/nautilus_food.json`: `#fishes` and `#nautilus_bucket_food`.
pub const FOOD: &[&str] = &[
    "minecraft:cod", "minecraft:cooked_cod", "minecraft:salmon", "minecraft:cooked_salmon", "minecraft:pufferfish", "minecraft:tropical_fish",
    "minecraft:pufferfish_bucket", "minecraft:cod_bucket", "minecraft:salmon_bucket", "minecraft:tropical_fish_bucket",
];
/// `SmoothSwimmingMoveControl(85, 10, 0.011, 0, true)`, shared with the zombie nautilus.
pub const LOCOMOTION: &[crate::locomotion::ModeEntry] = &[(Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params { needs_water: false, breach: false, control: swim::Control::Smooth { max_turn_x: 85.0, max_turn_y: 10.0, in_water: 0.011, outside: 0.0, gravity: true }, push: swim::Push::Speed, sinks: false, ..swim::Params::DEFAULT }))];

pub static DEF: Def = Def {
    name: "minecraft:nautilus",
    goals: &[
        Entry::new(&animal_panic::MODULE, 0, &animal_panic::Params { speed: 1.6, ..animal_panic::Params::DEFAULT }),
        Entry::new(&breed::MODULE, 1, &breed::Params { speed: 0.4, foods: FOOD, ..breed::Params::DEFAULT }),
        Entry::new(&tempt::MODULE, 2, &tempt::Params { speed: 1.3, items: FOOD, stop: 3.5, ..tempt::Params::DEFAULT }),
        Entry::new(&charge_attack::MODULE, 3, &charge_attack::Params { speed: 0.6, ..charge_attack::Params::DEFAULT }),
        Entry::new(&fish_swim::MODULE, 4, &fish_swim::Params { speed: 1.0, interval: 1 }),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion]),
    locomotion: LOCOMOTION,
    ..Def::EMPTY
};
