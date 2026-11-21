//! `TraderLlama` (`mobs/llama.rs:53-96`): the llama, plus a panic of 2,0 at priority 1 (before the run around), the defence of the holder of its lead
//! (`defend_leash_holder`, targets, 1: the trader's lamas attack whoever hurt the trader), and the zombie family as prey (2, seen). Left out: the illagers
//! as prey (their species are absent), the spit and its two goals (as the llama).
use super::{llama, Change, Def, Section};
use crate::behaviours::{defend_leash_holder, float, nearest_target, panic, Entry};

pub static DEF: Def = Def {
    name: "minecraft:trader_llama",
    reference: Some(&llama::DEF),
    changes: &[
        Change::InsertAfter { module: &float::MODULE, nth: 0, entry: Entry::new(&panic::MODULE, 1, &panic::Params { speed: 2.0, ..panic::Params::DEFAULT }) },
        Change::Add(Section::Targets, Entry::new(&defend_leash_holder::MODULE, 1, &defend_leash_holder::Params::DEFAULT)),
        Change::Add(
            Section::Targets,
            Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params { types: &["minecraft:zombie", "minecraft:husk", "minecraft:drowned", "minecraft:zombie_villager", "minecraft:zombified_piglin"], must_see: true, ..nearest_target::Params::DEFAULT }),
        ),
    ],
    llama_strength: true,
    ..Def::EMPTY
};
