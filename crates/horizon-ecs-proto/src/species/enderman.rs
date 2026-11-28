//! `Enderman` (`mobs/enderman.rs:19-45`): its targets and goals. Its endermite target has no `reach`: the `Mob` sets `must_reach` and never reads it. Not ported: the teleport (`randomTeleport`: service absent), the water damage and
//! the day departure that call it, the persistent anger, the water malus of the paths (`NavParams`).
use crate::behaviours::{ambient_sound, ender_teleport, float, freeze_when_looked_at, hurt_by, leave_block, look_at_player, look_for_player, melee, nearest_target, random_look, stroll, take_block, Entry};
use crate::species::Def;

fn enderman_ambient(w: &mut bevy_ecs::prelude::World, e: bevy_ecs::prelude::Entity) -> &'static str {
    if w.get::<look_for_player::Gaze>(e).is_some_and(|g| g.creepy) { "entity.enderman.scream" } else { "entity.enderman.ambient" }
}

/// The rule `unset_anger_end`: a calm enderman saves `anger_end_time` as -1 (`stopBeingAngry`), not 0 (the field's default).
pub static UNSET_ANGER_END: crate::rules::Rule = crate::rules::Rule { name: "unset_anger_end", equip: equip_unset_anger_end, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };
fn equip_unset_anger_end(w: &mut bevy_ecs::prelude::World, e: bevy_ecs::prelude::Entity, _: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(crate::nbt_fields::common::AngerEndUnset);
}

pub static DEF: Def = Def {
    load_attrs: &[crate::attr_mod::LoadAttr { name: "movement_speed", base: None, when: crate::attr_mod::LoadWhen::Always }],
    name: "minecraft:enderman",
    targets: &[
        Entry::new(&look_for_player::MODULE, 1, &look_for_player::Params::DEFAULT),
        Entry::new(&hurt_by::MODULE, 2, &hurt_by::Params::DEFAULT),
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:endermite"], ..nearest_target::Params::DEFAULT }),
    ],
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&freeze_when_looked_at::MODULE, 1, &freeze_when_looked_at::Params::DEFAULT),
        Entry::new(&melee::MODULE, 2, &melee::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 7, &stroll::Params { water_avoiding: 0.0, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 8, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 8, &random_look::Params::DEFAULT),
        Entry::new(&leave_block::MODULE, 10, &leave_block::Params::DEFAULT),
        Entry::new(&take_block::MODULE, 11, &take_block::Params::DEFAULT),
        Entry::new(&ender_teleport::MODULE, 255, &ender_teleport::Params::ENDERMAN),
        Entry::new(&ambient_sound::MODULE, 14, &ambient_sound::Params { interval: Some(80), choose: Some(enderman_ambient), ..ambient_sound::Params::DEFAULT }),
    ],
    rules: &[crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 5, max: 5 }), crate::rules::RuleEntry::new(&crate::hostile::SUN_FLEE, &()), crate::rules::RuleEntry::new(&UNSET_ANGER_END, &())],
    spawn: Some(crate::spawning::SpawnDef::MONSTER), despawn: Some(crate::despawn::DespawnDef::MOB),
    not_in_peaceful: true,
    sound_source: super::SoundCategory::Hostile,
    ..Def::EMPTY
};
