//! `Tadpole` (`aquatic.rs:134-150` goals, `animal_brain.rs:2740-2792` brain): a fish of the base (panic, avoidance of the player, swimming)
//! and its brain (panic at 2.0, looking at players and at the nearest creatures, swimming about, following the frog food (slime ball) for
//! 60 ticks, then its cooldown count of 60 ticks, the swim stroll, the walk to the look target). Its jar control is `SmoothSwimmingMoveControl(85, 10, 0.02, 0.1, true)`, but wired here
//! it slows the swim (3D distance per 2 s: 2.0 against 2.84 in vanilla, 3.09 with the fish's control), so the fish's modes stay (DOUTE: `swim::Control::Smooth` itself). The ground walk is the fish's (DOUTE: vanilla has no ground path).
use super::{templates::fish, Def};
use crate::locomotion::{swim, walk, Medium};
use crate::behaviours::{Cap, Entry};
use crate::brain::behaviours::{follow_temptation, gate_all, look_at_player_sometimes, look_at_target_sink, move_to_target_sink, panic, stay_in_fluid, swim_stroll, walk_to_look_target};
use crate::brain::sensors::{hurt_by, living, nearest_players, tempting};
use crate::brain::*;

const FOOD: &[&str] = &["minecraft:slime_ball"];
const TEMPT: follow_temptation::Params = follow_temptation::Params::DEFAULT;
const WALK_LOOK: walk_to_look_target::Params = walk_to_look_target::Params { speed: 0.5, close: 3, needs_same_fluid: false, dry_speed: None };

pub static DEF: Def = Def {
    name: "minecraft:tadpole",
    reference: Some(&fish::DEF),
    locomotion: &[
        (Medium::Ground, Entry::new(&walk::MODULE, 0, &walk::Params::DEFAULT)),
        (Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params { flop: Some("entity.tadpole.flop"), ..swim::Params::DEFAULT })),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Brain]),
    brain: BrainDef {
        sensors: &[SensorEntry::new(&living::SENSOR, 20), SensorEntry::new(&nearest_players::SENSOR, 20), SensorEntry::new(&hurt_by::SENSOR, 20), SensorEntry::new(&tempting::SENSOR, 20).with(&tempting::Params { range: 10.0, foods: FOOD })],
        activities: &[ActivityDef { name: "core", slot: 1, requires: &[] }, ActivityDef { name: "idle", slot: 5, requires: &[] }],
        pick: &["idle"],
        initial: &[],
        behaviours: &[
            BEntry { behaviour: &panic::BEHAVIOUR, activity: "core", priority: 0, params: &panic::Params { speed: 2.0, threat: None }, needs: &[], duration: Some((100, 120)), kids: &[] },
            BEntry { behaviour: &look_at_target_sink::BEHAVIOUR, activity: "core", priority: 1, params: &(), needs: &[(&LOOK_TARGET, Need::Present)], duration: Some((45, 90)), kids: &[] },
            BEntry { behaviour: &move_to_target_sink::BEHAVIOUR, activity: "core", priority: 2, params: &(), needs: &[(&PATH, Need::Absent), (&WALK_TARGET, Need::Present)], duration: Some((150, 250)), kids: &[] },
            BEntry { behaviour: &look_at_player_sometimes::BEHAVIOUR, activity: "idle", priority: 0, params: &look_at_player_sometimes::Params { range_sqr: 36.0, min: 30, spread: 31, players_only: true }, needs: &[(&LOOK_TARGET, Need::Absent), (&VISIBLE_MOBS, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &follow_temptation::BEHAVIOUR, activity: "idle", priority: 1, params: &TEMPT, needs: &[(&TEMPTING_PLAYER, Need::Present), (&TEMPTATION_COOLDOWN_TICKS, Need::Absent), (&BREED_TARGET, Need::Absent), (&IS_PANICKING, Need::Absent)], duration: Some((60, 60)), kids: &[] },
            BEntry {
                behaviour: &gate_all::BEHAVIOUR,
                activity: "idle",
                priority: 2,
                params: &(),
                needs: &[(&WALK_TARGET, Need::Absent)],
                duration: None,
                kids: &[
                    Kid { behaviour: &swim_stroll::BEHAVIOUR, weight: 2, params: &swim_stroll::Params::DEFAULT, needs: &[], duration: None },
                    Kid { behaviour: &walk_to_look_target::BEHAVIOUR, weight: 3, params: &WALK_LOOK, needs: &[(&LOOK_TARGET, Need::Present)], duration: None },
                    Kid { behaviour: &stay_in_fluid::BEHAVIOUR, weight: 5, params: &stay_in_fluid::Params::DEFAULT, needs: &[], duration: None },
                ],
            },
        ],
    },
    rules: &[crate::rules::RuleEntry::new(&crate::loot::NO_XP_DROP, &()), crate::rules::RuleEntry::new(&crate::behaviours::held_swap::HELD_SWAP, &crate::behaviours::held_swap::SwapParams { from: "minecraft:water_bucket", to: "minecraft:tadpole_bucket", adult_only: false, takes_creature: true, sound: "minecraft:item.bucket.fill_tadpole", alt: None, extra: crate::behaviours::held_swap::tadpole_data })],
    ..Def::EMPTY
};
