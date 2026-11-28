//! `Husk` (`mobs/undead.rs:36-90`): the zombie's configuration without the sun burn (it does not burn).
use super::{templates::zombie_base, Change, Def, Section};
use crate::behaviours::{ambient_sound, convert_in_water, Entry};

pub static DEF: Def = Def { sound_source: super::SoundCategory::Hostile, load_attrs: zombie_base::LOAD_ATTRS, name: "minecraft:husk", reference: Some(&zombie_base::DEF), rules: &[
        crate::rules::RuleEntry::new(&crate::effects::ON_HIT, &crate::effects::HUSK),
        crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 5, max: 5 }),
        crate::rules::RuleEntry::new(&crate::loot::BABY_DROPS, &crate::loot::BabyDrops::ZOMBIE),
        crate::rules::RuleEntry::new(&crate::reinforcements::RULE, &crate::reinforcements::Params::ZOMBIE),
    ], changes: &[Change::Add(Section::Goals, Entry::new(&convert_in_water::MODULE, 0, &convert_in_water::Params { into: "minecraft:zombie", sound: 1041, ..convert_in_water::Params::DEFAULT })), Change::Add(Section::Goals, Entry::new(&ambient_sound::MODULE, 14, &ambient_sound::Params { sounds: &["entity.husk.ambient"], interval: Some(80), ..ambient_sound::Params::DEFAULT }))], not_in_peaceful: true, ..Def::EMPTY };
