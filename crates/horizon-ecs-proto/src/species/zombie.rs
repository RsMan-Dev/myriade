//! `Zombie.registerGoals`: melee 3, stroll 7, look 8, 8; the target selector's nearest player (priority 2).
use super::Def;
use crate::rules::RuleEntry;
use crate::behaviours::{hurt_by, look_at_player, melee, nearest_target, random_look, stroll, Entry};

pub static DEF: Def = Def {
    name: "minecraft:zombie",
    reference: None,
    // `Zombie.addBehaviourGoals`: hurt-by (alerting the others but the zombified piglin), the nearest player. Still to add when
    // the species exist here (an instance for a kind with no species is never armed but still costs its entry): the
    // villagers and the wandering trader (priority 3: `types: [minecraft:villager, minecraft:wandering_trader], must_see:
    // false`), the iron golem (3), a baby turtle (5: `types: [minecraft:turtle], predicate: "baby"`).
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert: true, alert_except: &["minecraft:zombified_piglin"], ..hurt_by::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER),
    ],
    goals: &[
        Entry::new(&melee::MODULE, 3, &melee::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 7, &stroll::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 8, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 8, &random_look::Params::DEFAULT),
    ],
    changes: &[],
    rules: &[RuleEntry::new(&crate::hostile::SUN_BURN, &())],
    caps: None,
};
