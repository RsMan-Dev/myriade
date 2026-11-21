//! `Piglin` (`mobs/piglin.rs:1329-1363`, `register_piglin`). Brain-less: goals and targets in the priorities of `register_piglin`.
//! The behaviours are generic; this file only holds the piglin's names and numbers. Not ported (see the notes): the baby's hoglin ride.
use crate::behaviours::{
    admire, admire_seen, ambient_sound, back_up_if_too_close, celebrate, convert_timer, flee_repellent, float, go_to_wanted, hunt_prey, hurt_by, look_at_held,
    interact_with_door, look_at_player, melee, nearest_target, random_look, ranged_crossbow, spear_use, stroll, walk_away, Entry,
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
        // `PiglinAi.initCoreActivity`: `InteractWithDoor.create()`.
        Entry::new(&interact_with_door::MODULE, 0, &()),
        Entry::new(&admire::MODULE, 1, &admire::Params::DEFAULT),
        Entry::new(&admire_seen::MODULE, 1, &admire_seen::Params { items: &["minecraft:gold_ingot"], ..admire_seen::Params::DEFAULT }),
        Entry::new(&walk_away::MODULE, 2, &walk_away::Params::DEFAULT),
        Entry::new(&flee_repellent::MODULE, 2, &flee_repellent::Params {
            blocks: &["minecraft:soul_campfire", "minecraft:soul_torch", "minecraft:soul_wall_torch", "minecraft:soul_lantern"],
            species: &[],
            ..flee_repellent::Params::DEFAULT
        }),
        // `crossbow_goal(1.0, 8.0)` (`illager::crossbow_goal`): the crossbow's goal, charge 25 ticks, the difficulty of the illagers.
        Entry::new(&ranged_crossbow::MODULE, 3, &ranged_crossbow::Params { speed: 1.0, radius: 8.0, charge: 25, ..ranged_crossbow::Params::PILLAGER }).when(&HOLDS_CROSSBOW),
        // `BackUpIfTooClose.create(5, 0.75)` behind `hasCrossbow` (`PiglinAi.initFightActivity`): backs off from a target too close.
        Entry::new(&back_up_if_too_close::MODULE, 3, &back_up_if_too_close::Params::DEFAULT).when(&HOLDS_CROSSBOW),
        // `g.add(3, PiglinSpear)` (`mobs/piglin.rs:1346`): the spear in hand takes the fight, the melee keeps the rest.
        Entry::new(&spear_use::MODULE, 3, &spear_use::PIGLIN),
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
