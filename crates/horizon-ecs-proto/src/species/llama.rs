//! `Llama.registerGoals` as far as the frame goes: float, run around like crazy (1: the taming of a ridden llama), the caravan (2: `follow_caravan`,
//! on a lead), panic, breed (wheat and hay bales), tempt (hay bale), follow the parent, stroll, look. Targets: `LlamaHurtBy` (`hurt_by` that the llama's
//! own spit ends: `ends_on_shot`) and the untamed wolf (2, every 16 ticks, a quarter of the range; the path reach of the `Mob`'s `.reach()` is not modelled,
//! as in the polar bear). Left out: nothing of the goals (the spit is `ranged_attack`). A llama that is not the one of a trader: its panic is 1.2.
//! It takes a lead (`leash::LEASH`, `followLeashSpeed` 2,0) and a rider (a seat, no saddle: it is not steered).
use super::Def;
use crate::behaviours::{hurt_by, nearest_target, breed, float, follow_caravan, follow_parent, look_at_player, panic, random_look, ranged_attack, run_around_like_crazy, stroll, tempt, Cap, Entry};
use crate::leash::{LeashParams, LEASH};
use crate::mount::{RideParams, RIDE};
use crate::rules::RuleEntry;

const FOOD: &[&str] = &["minecraft:wheat", "minecraft:hay_block"];

/// `Prey::Filter(wolf && !tame)` of the llama's second target (`llama.rs:78-87`).
fn untamed(w: &bevy_ecs::prelude::World, e: bevy_ecs::prelude::Entity) -> bool {
    !w.get::<crate::tame::Owner>(e).is_some_and(|o| o.0.is_some())
}

pub static DEF: Def = Def {
    name: "minecraft:llama",
    variant: Some(&super::variants::LLAMA),
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { ends_on_shot: true, ..hurt_by::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params { types: &["minecraft:wolf"], candidate_when: Some(untamed), must_see: false, interval: 16, follow_scale: 0.25, ..nearest_target::Params::DEFAULT }),
    ],
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&run_around_like_crazy::MODULE, 1, &run_around_like_crazy::Params::DEFAULT),
        Entry::new(&ranged_attack::MODULE, 3, &ranged_attack::Params::LLAMA),
        Entry::new(&follow_caravan::MODULE, 2, &follow_caravan::Params::DEFAULT),
        Entry::new(&panic::MODULE, 3, &panic::Params { speed: 1.2, ..panic::Params::DEFAULT }),
        Entry::new(&breed::MODULE, 4, &breed::Params { speed: 1.0, foods: FOOD, ..breed::Params::DEFAULT }),
        Entry::new(&tempt::MODULE, 5, &tempt::Params { speed: 1.25, items: &["minecraft:hay_block"], ..tempt::Params::DEFAULT }),
        Entry::new(&follow_parent::MODULE, 6, &follow_parent::Params { speed: 1.0 }),
        Entry::new(&stroll::MODULE, 7, &stroll::Params { speed: 0.7, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 8, &look_at_player::Params { range: 6.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 9, &random_look::Params::DEFAULT),
    ],
    // A llama is a tamable, rideable horse (`AbstractChestedHorse`): the caps of the horse, for the wild run around like crazy.
    rules: &[crate::rules::RuleEntry::new(&crate::rules::climbable::NO_CLIMB, &()), RuleEntry::new(&RIDE, &RideParams::LLAMA),RuleEntry::new(&crate::make_mad::ANGRY, &crate::make_mad::Angry::LLAMA), RuleEntry::new(&LEASH, &LeashParams::LLAMA), RuleEntry::new(&crate::equipment::EQUIP, &crate::equipment::EquipParams::BODY), RuleEntry::new(&crate::fit_chest::FIT_CHEST, &crate::fit_chest::ChestParams::LLAMA)],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Tamable, Cap::Rideable]),
    llama_strength: true,
    random_max_health: true,
    spawn: Some(crate::spawning::SpawnDef { ..crate::spawning::SpawnDef::ANIMAL }),
    despawn: Some(crate::despawn::DespawnDef::KEPT),
    ..Def::EMPTY
};
