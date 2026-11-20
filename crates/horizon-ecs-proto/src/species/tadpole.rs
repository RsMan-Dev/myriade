//! `Tadpole` (`aquatic.rs:134-150` goals, `animal_brain.rs:2740-2792` brain): a fish of the base (panic, avoidance of the player, swimming)
//! and its brain (panic at 2.0, looking at players and at the nearest creatures, swimming about). Left out, waiting for their modules:
//! the frog food (`follow_temptation`, with its temptation sensor and its cooldown count), the swim stroll (`swim_stroll`) and the walk to
//! the look target (`walk_to_look_target`). Its own swimming mode (`SmoothSwimming`) is not in this file: the fish's modes apply (DOUTE).
use super::{templates::fish, Def};
use crate::behaviours::Cap;
use crate::brain::behaviours::{gate_all, look_at_player_sometimes, look_at_target_sink, move_to_target_sink, panic, stay_in_fluid};
use crate::brain::sensors::{hurt_by, living, nearest_players};
use crate::brain::*;

pub static DEF: Def = Def {
    name: "minecraft:tadpole",
    reference: Some(&fish::DEF),
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Brain]),
    brain: BrainDef {
        sensors: &[SensorEntry::new(&living::SENSOR, 20), SensorEntry::new(&nearest_players::SENSOR, 20), SensorEntry::new(&hurt_by::SENSOR, 20)],
        activities: &[ActivityDef { name: "core", slot: 1, requires: &[] }, ActivityDef { name: "idle", slot: 5, requires: &[] }],
        pick: &["idle"],
        initial: &[],
        behaviours: &[
            BEntry { behaviour: &panic::BEHAVIOUR, activity: "core", priority: 0, params: &panic::Params { speed: 2.0, threat: None }, needs: &[], duration: Some((100, 120)), kids: &[] },
            BEntry { behaviour: &look_at_target_sink::BEHAVIOUR, activity: "core", priority: 1, params: &(), needs: &[(&LOOK_TARGET, Need::Present)], duration: Some((45, 90)), kids: &[] },
            BEntry { behaviour: &move_to_target_sink::BEHAVIOUR, activity: "core", priority: 2, params: &(), needs: &[(&PATH, Need::Absent), (&WALK_TARGET, Need::Present)], duration: Some((150, 250)), kids: &[] },
            BEntry { behaviour: &look_at_player_sometimes::BEHAVIOUR, activity: "idle", priority: 0, params: &(), needs: &[(&LOOK_TARGET, Need::Absent), (&VISIBLE_MOBS, Need::Present)], duration: None, kids: &[] },
            BEntry {
                behaviour: &gate_all::BEHAVIOUR,
                activity: "idle",
                priority: 2,
                params: &(),
                needs: &[(&WALK_TARGET, Need::Absent)],
                duration: None,
                kids: &[Kid { behaviour: &stay_in_fluid::BEHAVIOUR, weight: 1, params: &stay_in_fluid::Params::DEFAULT, needs: &[], duration: None }],
            },
        ],
    },
    rules: &[crate::rules::RuleEntry::new(&crate::behaviours::held_swap::HELD_SWAP, &crate::behaviours::held_swap::SwapParams { from: "minecraft:water_bucket", to: "minecraft:tadpole_bucket", adult_only: false, takes_creature: true })],
    ..Def::EMPTY
};
