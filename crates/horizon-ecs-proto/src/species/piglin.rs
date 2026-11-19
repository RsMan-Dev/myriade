//! `Piglin` (`mobs/piglin.rs:1329-1363`, `register_piglin`). Brain-less: goals and targets in the priorities of `register_piglin`.
//! The behaviours are generic; this file only holds the piglin's names and numbers. Not ported (see the notes): the spear,
//! the baby's hoglin ride, the look at the dropped item (AdmireSeen), the `BackUpIfTooClose` strafe.
use crate::behaviours::{
    admire, ambient_sound, celebrate, convert_timer, flee_repellent, float, go_to_wanted, hunt_prey, hurt_by, look_at_held,
    look_at_player, melee, nearest_target, random_look, ranged_bow, stroll, walk_away, Entry,
};
use crate::species::{predicates::Cond, Def};

static HOLDS_CROSSBOW: Cond = Cond::Is("horizon:holds_crossbow");
static NO_CROSSBOW: Cond = Cond::Not(&HOLDS_CROSSBOW);
static HOSTILE: Cond = Cond::Is("horizon:hostile_to_player");

pub static DEF: Def = Def {
    name: "minecraft:piglin",
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert: true, alert_types: &["minecraft:piglin", "minecraft:piglin_brute"], ignoring: &["minecraft:piglin", "minecraft:piglin_brute"], ..hurt_by::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER).when(&HOSTILE),
        Entry::new(&hunt_prey::MODULE, 3, &hunt_prey::Params { prey: &["minecraft:hoglin"], ..hunt_prey::Params::DEFAULT }),
    ],
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&convert_timer::MODULE, 0, &convert_timer::Params { into: "minecraft:zombified_piglin", after: 300 }),
        Entry::new(&admire::MODULE, 1, &admire::Params::DEFAULT),
        Entry::new(&walk_away::MODULE, 2, &walk_away::Params { speed: 1.0, distance: 12.0 }),
        Entry::new(&flee_repellent::MODULE, 2, &flee_repellent::Params {
            blocks: &["minecraft:soul_campfire", "minecraft:soul_torch", "minecraft:soul_wall_torch", "minecraft:soul_lantern"],
            species: &[],
            ..flee_repellent::Params::DEFAULT
        }),
        // `crossbow_goal(1.0, 8.0)`: the bow module with a charge of 25 ticks and the fixed power of the crossbow (1.6).
        Entry::new(&ranged_bow::MODULE, 3, &ranged_bow::Params { speed: 1.0, radius: 8.0, charge: 25, fixed_power: Some(1.6), ..ranged_bow::Params::DEFAULT }).when(&HOLDS_CROSSBOW),
        Entry::new(&melee::MODULE, 3, &melee::Params { interval: 20, ..melee::Params::DEFAULT }).when(&NO_CROSSBOW),
        Entry::new(&go_to_wanted::MODULE, 4, &go_to_wanted::Params { items: &["minecraft:gold_ingot"], ..go_to_wanted::Params::DEFAULT }),
        Entry::new(&celebrate::MODULE, 4, &celebrate::Params::DEFAULT),
        Entry::new(&look_at_held::MODULE, 5, &look_at_held::Params { items: &["minecraft:gold_ingot"], ..look_at_held::Params::DEFAULT }),
        Entry::new(&stroll::MODULE, 5, &stroll::Params { speed: 0.6, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 7, &random_look::Params::DEFAULT),
        Entry::new(&ambient_sound::MODULE, 8, &ambient_sound::Params { sounds: &["minecraft:entity.piglin.ambient"], min: 80, max: 160 }),
    ],
    ..Def::EMPTY
};
