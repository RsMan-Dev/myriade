//! The goals of each species, as a configuration: a list of `Entry { module, priority, params }` for the goal selector and
//! one for the target selector (the same model). Nothing else ties a species to a goal. A module can appear several times
//! (the pig's two tempt goals). Priorities are vanilla's `registerGoals`; the lists are in registration order.
use crate::behaviours::*;
use crate::behaviours::{breed, eat_block, float, follow_parent, look_at_player, melee, panic, random_look, stroll, target, tempt};

/// What the selectors of an animal are built from: the target entries then the goal entries (same model; a target goal
/// takes no flag), resolved once into the animal's `GoalSet`.
#[derive(Clone, Copy)]
pub struct Config {
    pub entries: &'static [Entry],
}
/// One species: its two sections.
pub struct Def {
    pub targets: &'static [Entry],
    pub goals: &'static [Entry],
}
impl Def {
    /// The two sections as one list, built once.
    fn config(&'static self, cell: &'static std::sync::OnceLock<Box<[Entry]>>) -> Config {
        Config { entries: cell.get_or_init(|| self.targets.iter().chain(self.goals).copied().collect()) }
    }
}
impl Config {
    /// The parameters of the `nth` entry of the module (for the tests and the tools that read the configuration).
    pub fn param<T: 'static>(&self, m: &'static Module, nth: usize) -> Option<&'static T> {
        let e = self.entries.iter().filter(|e| same(e.module, m)).nth(nth)?;
        let p: &'static dyn std::any::Any = e.params;
        p.downcast_ref::<T>()
    }
    pub fn has(&self, m: &'static Module) -> bool {
        self.entries.iter().any(|e| same(e.module, m))
    }
}

const FLOAT: Entry = Entry::new(&float::MODULE, 0, &float::Params::DEFAULT);
const WANDER: Entry = Entry::new(&stroll::MODULE, 0, &stroll::Params::DEFAULT);
const LOOK: Entry = Entry::new(&look_at_player::MODULE, 0, &look_at_player::Params::DEFAULT);
const RANDOM_LOOK: Entry = Entry::new(&random_look::MODULE, 0, &random_look::Params::DEFAULT);
const fn at(e: Entry, priority: u8) -> Entry {
    Entry { priority, ..e }
}

/// `Cow.registerGoals`.
pub static COW: Def = Def {
    targets: &[],
    goals: &[
        FLOAT,
        Entry::new(&panic::MODULE, 1, &panic::Params::DEFAULT),
        Entry::new(&breed::MODULE, 2, &breed::Params::DEFAULT),
        Entry::new(&tempt::MODULE, 3, &tempt::Params { speed: 1.25, items: &["minecraft:wheat"], ..tempt::Params::DEFAULT }),
        Entry::new(&follow_parent::MODULE, 4, &follow_parent::Params { speed: 1.25 }),
        at(WANDER, 5),
        at(LOOK, 6),
        at(RANDOM_LOOK, 7),
    ],
};
/// `Pig.registerGoals`: two tempt goals of one priority (the carrot on a stick, then its foods), each with its calm-down.
pub static PIG: Def = Def {
    targets: &[],
    goals: &[
        FLOAT,
        Entry::new(&panic::MODULE, 1, &panic::Params { speed: 1.25, ..panic::Params::DEFAULT }),
        Entry::new(&breed::MODULE, 3, &breed::Params::DEFAULT),
        Entry::new(&tempt::MODULE, 4, &tempt::Params { speed: 1.2, items: &["minecraft:carrot_on_a_stick"], ..tempt::Params::DEFAULT }),
        Entry::new(&tempt::MODULE, 4, &tempt::Params { speed: 1.2, items: &["minecraft:carrot", "minecraft:potato", "minecraft:beetroot"], ..tempt::Params::DEFAULT }),
        Entry::new(&follow_parent::MODULE, 5, &follow_parent::Params { speed: 1.1 }),
        at(WANDER, 6),
        at(LOOK, 7),
        at(RANDOM_LOOK, 8),
    ],
};
/// `Sheep.registerGoals`.
pub static SHEEP: Def = Def {
    targets: &[],
    goals: &[
        FLOAT,
        Entry::new(&panic::MODULE, 1, &panic::Params { speed: 1.25, ..panic::Params::DEFAULT }),
        Entry::new(&breed::MODULE, 2, &breed::Params::DEFAULT),
        Entry::new(&tempt::MODULE, 3, &tempt::Params { speed: 1.1, items: &["minecraft:wheat"], ..tempt::Params::DEFAULT }),
        Entry::new(&follow_parent::MODULE, 4, &follow_parent::Params { speed: 1.1 }),
        Entry::new(&eat_block::MODULE, 5, &eat_block::Params::DEFAULT),
        at(WANDER, 6),
        at(LOOK, 7),
        at(RANDOM_LOOK, 8),
    ],
};
/// `Chicken.registerGoals` (the egg timer is a rule of `Chicken.aiStep`, not a goal: `farm.rs`).
pub static CHICKEN: Def = Def {
    targets: &[],
    goals: &[
        FLOAT,
        Entry::new(&panic::MODULE, 1, &panic::Params { speed: 1.4, ..panic::Params::DEFAULT }),
        Entry::new(&breed::MODULE, 2, &breed::Params::DEFAULT),
        Entry::new(
            &tempt::MODULE,
            3,
            &tempt::Params { speed: 1.0, items: &["minecraft:wheat_seeds", "minecraft:beetroot_seeds", "minecraft:melon_seeds", "minecraft:pumpkin_seeds"], ..tempt::Params::DEFAULT },
        ),
        Entry::new(&follow_parent::MODULE, 4, &follow_parent::Params { speed: 1.1 }),
        at(WANDER, 5),
        at(LOOK, 6),
        at(RANDOM_LOOK, 7),
    ],
};
/// `Zombie.registerGoals`: melee 3, stroll 7, look 8, 8; the target selector's nearest player (priority 2).
pub static ZOMBIE: Def = Def {
    targets: &[Entry::new(&target::MODULE, 2, &target::Params::DEFAULT)],
    goals: &[
        Entry::new(&melee::MODULE, 3, &melee::Params::DEFAULT),
        at(WANDER, 7),
        Entry::new(&look_at_player::MODULE, 8, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        at(RANDOM_LOOK, 8),
    ],
};

/// The configuration of a species by its type name (the cow's is the default).
pub fn config(name: &str) -> Config {
    use std::sync::OnceLock;
    static C: [OnceLock<Box<[Entry]>>; 5] = [const { OnceLock::new() }; 5];
    match name {
        "minecraft:pig" => PIG.config(&C[1]),
        "minecraft:sheep" => SHEEP.config(&C[2]),
        "minecraft:chicken" => CHICKEN.config(&C[3]),
        "minecraft:zombie" => ZOMBIE.config(&C[4]),
        _ => COW.config(&C[0]),
    }
}
