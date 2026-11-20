//! `CopperGolem` and `CopperGolemAi` WITHOUT the transport between containers (a module to come): two sensors, the core and idle
//! activities, and the generic behaviours. The transport's cooldown starts as the constructor sets it and is a memory that expires (no countdown); the
//! stroll waits for it (as the `Mob` does), so a golem walks while it lasts.
use super::Def;
use crate::behaviours::Cap;
use crate::brain::behaviours::{do_nothing, look_at_player_sometimes, look_at_target_sink, move_to_target_sink, panic, run_one, stroll};
use crate::brain::sensors::{hurt_by, living};
use crate::brain::*;

pub static DEF: Def = Def {
    name: "minecraft:copper_golem",
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Locomotion, Cap::Targets, Cap::Goals, Cap::Brain]),
    brain: BrainDef {
        sensors: &[SensorEntry::new(&living::SENSOR, 20), SensorEntry::new(&hurt_by::SENSOR, 20)],
        activities: &[ActivityDef { name: "core", slot: 1, requires: &[] }, ActivityDef { name: "idle", slot: 5, requires: &[] }],
        pick: &["idle"],
        initial: &[Initial { key: &TRANSPORT_ITEMS_COOLDOWN_TICKS, base: 60, range: 40 }],
        behaviours: &[
            BEntry { behaviour: &panic::BEHAVIOUR, activity: "core", priority: 0, params: &panic::Params { speed: 1.5, threat: None }, needs: &[], duration: Some((100, 120)), kids: &[] },
            BEntry { behaviour: &look_at_target_sink::BEHAVIOUR, activity: "core", priority: 1, params: &(), needs: &[(&LOOK_TARGET, Need::Present)], duration: Some((45, 90)), kids: &[] },
            BEntry { behaviour: &look_at_player_sometimes::BEHAVIOUR, activity: "idle", priority: 1, params: &(), needs: &[(&LOOK_TARGET, Need::Absent), (&VISIBLE_MOBS, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &move_to_target_sink::BEHAVIOUR, activity: "core", priority: 2, params: &(), needs: &[(&PATH, Need::Absent), (&WALK_TARGET, Need::Present)], duration: Some((150, 250)), kids: &[] },
            BEntry {
                behaviour: &run_one::BEHAVIOUR,
                activity: "idle",
                priority: 2,
                params: &(),
                needs: &[(&WALK_TARGET, Need::Absent), (&TRANSPORT_ITEMS_COOLDOWN_TICKS, Need::Present)],
                duration: None,
                kids: &[
                    Kid { behaviour: &stroll::BEHAVIOUR, weight: 1, params: &stroll::Params { speed: 1.0, h: 2, v: 2 }, needs: &[], duration: None },
                    Kid { behaviour: &do_nothing::BEHAVIOUR, weight: 1, params: &(), needs: &[], duration: Some((30, 60)) },
                ],
            },
        ],
    },
    ..Def::EMPTY
};
