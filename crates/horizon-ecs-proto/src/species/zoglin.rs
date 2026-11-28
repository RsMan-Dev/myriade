//! `Zoglin` (`mobs/piglin.rs:1570-1590`, `register_zoglin`): the melee at an interval by age (41 adult, 16 baby: `brain.rs:97`,
//! `melee_attack_by_age(40, 15)` = `intervals(41, 16)`), the target of every creature but the creeper and the zoglin (`except`).
use crate::behaviours::{ambient_sound, float, hurt_by, look_at_player, melee, nearest_target, stroll, walk_to_look, Entry};
use crate::species::Def;

fn zoglin_ambient(w: &mut bevy_ecs::prelude::World, e: bevy_ecs::prelude::Entity) -> &'static str {
    if w.get::<crate::targeting::Target>(e).is_some() { "entity.zoglin.angry" } else { "entity.zoglin.ambient" }
}

pub static DEF: Def = Def {
    load_attrs: &[crate::attr_mod::LoadAttr { name: "attack_damage", base: Some(0.5), when: crate::attr_mod::LoadWhen::Baby }],
    name: "minecraft:zoglin",
    caps: Some(super::FIRE_IMMUNE_CAPS),
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params::DEFAULT),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER),
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { any: true, except: &["minecraft:creeper", "minecraft:zoglin"], ..nearest_target::Params::DEFAULT }),
    ],
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&melee::MODULE, 2, &melee::Params { interval: 41, interval_baby: Some(16), follow_unseen: true, ..melee::Params::DEFAULT }),
        Entry::new(&walk_to_look::MODULE, 4, &walk_to_look::Params { speed: 0.4, weight: 2, total: 5 }),
        Entry::new(&stroll::MODULE, 5, &stroll::Params { speed: 0.4, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params { range: 8.0, period: 50, species: look_at_player::Params::ALL, ..look_at_player::Params::DEFAULT }),
        Entry::new(&ambient_sound::MODULE, 14, &ambient_sound::Params { interval: Some(80), choose: Some(zoglin_ambient), ..ambient_sound::Params::DEFAULT }),
    ],
    // `Zoglin`: `xpReward` 5 (`Mob`'s reward, the equipment adds), a baby keeps its loot and its experience (`Monster`).
    rules: &[
        crate::rules::RuleEntry::new(&crate::leash::LEASH, &crate::leash::LeashParams::DEFAULT),
        crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 5, max: 5 }),
        crate::rules::RuleEntry::new(&crate::loot::BABY_DROPS, &crate::loot::BabyDrops::MONSTER),
    ],
    not_in_peaceful: true,
    sound_source: super::SoundCategory::Hostile,
    ..Def::EMPTY
};
