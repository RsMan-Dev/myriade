//! `Llama.registerGoals` as far as the frame goes: float, panic, breed (wheat and hay bales), tempt (hay bale), follow the parent, stroll, look. Left out, with the
//! reason: `RunAroundLikeCrazy` (a llama is not ridden), the caravan (`LlamaFollowCaravanGoal`: leads are absent), the spit (`RangedAttack`: the spit projectile
//! is absent) and its two target goals that serve it (`LlamaHurtBy`, the wolves). A llama that is not the one of a trader: its panic is 1.2.
use super::Def;
use crate::behaviours::{breed, float, follow_parent, look_at_player, panic, random_look, stroll, tempt, Entry};

const FOOD: &[&str] = &["minecraft:wheat", "minecraft:hay_block"];

pub static DEF: Def = Def {
    name: "minecraft:llama",
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&panic::MODULE, 3, &panic::Params { speed: 1.2, ..panic::Params::DEFAULT }),
        Entry::new(&breed::MODULE, 4, &breed::Params { speed: 1.0, foods: FOOD, ..breed::Params::DEFAULT }),
        Entry::new(&tempt::MODULE, 5, &tempt::Params { speed: 1.25, items: &["minecraft:hay_block"], ..tempt::Params::DEFAULT }),
        Entry::new(&follow_parent::MODULE, 6, &follow_parent::Params { speed: 1.0 }),
        Entry::new(&stroll::MODULE, 7, &stroll::Params { speed: 0.7, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 8, &look_at_player::Params { range: 6.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 9, &random_look::Params::DEFAULT),
    ],
    ..Def::EMPTY
};
