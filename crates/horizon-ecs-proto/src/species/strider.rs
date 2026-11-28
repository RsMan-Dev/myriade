//! `Strider` (`mobs/strider.rs:20-36`): walks on the ground and ON lava (the lava mode: its path map has lava as ground, the lava holds
//! it up, it floats). Goals: panic, breed and tempt by warped fungus, going to the lava, follow its parent, stroll, look.
//! The look at the other striders: a second `look_at_player` with `species` (priority 9, `LookAtPlayer::species([Strider], 8)`).
//! The stroll is `RandomStrollGoal` (bytecode: `new RandomStrollGoal(this, 1.0, 60)`): `DefaultRandomPos`, no water avoidance (`water_avoiding` 1.0).
use super::Def;
use crate::behaviours::{breed, follow_parent, look_at_player, panic, random_look, stroll, strider_go_to_lava, tempt, Cap, Entry};
use crate::locomotion::{lava, walk, Medium};
use crate::mount::{RideParams, RIDE};
use crate::rules::RuleEntry;

pub static DEF: Def = Def {
    spawn: Some(crate::spawning::SpawnDef { place: crate::spawning::Place::InLava, rule: crate::spawning::Rule::Strider, liquid: true, ..crate::spawning::SpawnDef::ANIMAL }),
    name: "minecraft:strider",
    goals: &[
        Entry::new(&crate::behaviours::move_emission::MODULE, 0, &crate::behaviours::move_emission::Params { step_gap: 0.6, ..crate::behaviours::move_emission::Params::DEFAULT }),
        Entry::new(&panic::MODULE, 1, &panic::Params { speed: 1.65, ..panic::Params::DEFAULT }),
        Entry::new(&breed::MODULE, 2, &breed::Params { speed: 1.0, foods: &["minecraft:warped_fungus"], ..breed::Params::DEFAULT }),
        Entry::new(&tempt::MODULE, 3, &tempt::Params { speed: 1.4, items: &["minecraft:warped_fungus", "minecraft:warped_fungus_on_a_stick"], ..tempt::Params::DEFAULT }),
        Entry::new(&strider_go_to_lava::MODULE, 4, &strider_go_to_lava::Params::DEFAULT),
        Entry::new(&follow_parent::MODULE, 5, &follow_parent::Params { speed: 1.0 }),
        Entry::new(&stroll::MODULE, 7, &stroll::Params { interval: Some(60), water_avoiding: 1.0, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 8, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 8, &random_look::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 9, &look_at_player::Params { range: 8.0, species: &["minecraft:strider"], ..look_at_player::Params::DEFAULT }),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Locomotion, Cap::Rideable, Cap::FireImmune]),
    rules: &[crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 1, max: 3 }), RuleEntry::new(&RIDE, &RideParams::STRIDER), RuleEntry::new(&crate::equipment::EQUIP, &crate::equipment::EquipParams::SADDLE)],
    locomotion: &[(Medium::Ground, Entry::new(&walk::MODULE, 0, &walk::Params::DEFAULT)), (Medium::Lava, Entry::new(&lava::MODULE, 0, &lava::Params::DEFAULT))],
    ..Def::EMPTY
};
