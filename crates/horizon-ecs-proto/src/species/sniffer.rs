//! `Sniffer` (`SnifferAi`, `animal_brain.rs:2132-2208`): the animal that digs seeds up. Four activities (core, dig, idle, sniff; the active one is
//! the first of dig, sniff, idle whose memories hold). Core: panic (2.0), the sink that walks to the walk target (500 to 700). Idle: make love, follow
//! the tempting player (torchflower seeds), look where it is told, feel happy, then one of: walk to the look target, scent, sniff, look at a
//! player, stroll, do nothing. Sniff: search (walk to the place sniffed). Dig: dig (the seed drops 120 ticks in), then rise (`brain::behaviours::sniff`).
//! The Mob's `Swim` is the goal `float` here (the same jump). Left out: the baby's tempting stop distance (2.5, the adult's 3.5 is used), the
//! sounds, the egg block of a birth is laid by `lay_egg` below as a `block_changes` change of the block at the parent's feet, the path test of
//! the dig (see `sniff.rs`).
use super::Def;
use crate::behaviours::{breed, float, Cap, Entry};
use crate::brain::behaviours::{countdown, do_nothing, follow_temptation, look_at_player, look_at_player_sometimes, look_at_target_sink, make_love, move_to_target_sink, panic, run_one, sniff, stroll, walk_to_look_target};
use crate::brain::sensors::{hurt_by, living, nearest_players, tempting};
use crate::brain::*;
use crate::rules::RuleEntry;

const FOOD: &[&str] = &["minecraft:torchflower_seeds"];
const COUNTDOWN_TEMPTATION: countdown::Params = countdown::Params { key: &TEMPTATION_COOLDOWN_TICKS };
const MAKE_LOVE: make_love::Params = make_love::Params { birth: make_love::Birth::Lays(lay_egg), ready: sniff::calm, ..make_love::Params::DEFAULT };
const TEMPT: follow_temptation::Params = follow_temptation::Params { speed: 1.25, close: 3.5, look_in_the_eyes: false, dry_speed: None };
const WALK_LOOK: walk_to_look_target::Params = walk_to_look_target::Params { speed: 1.0, close: 3, needs_same_fluid: false, dry_speed: None };
const STROLL: stroll::Params = stroll::Params { speed: 1.0, h: 10, v: 7, dry_only: false };
const SNIFF: sniff::Params = sniff::Params::DEFAULT;

/// `Sniffer.spawnChildFromBreeding`: the egg is laid on the block at the parent's feet.
fn lay_egg(w: &mut bevy_ecs::prelude::World, parent: bevy_ecs::prelude::Entity, _: bevy_ecs::prelude::Entity) {
    let Some(p) = w.get::<crate::cow::Pos>(parent).map(|p| p.0) else { return };
    crate::blocks::set_state(w, [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32], "minecraft:sniffer_egg", &[]);
}

pub static DEF: Def = Def {
    name: "minecraft:sniffer",
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Brain]),
    goals: &[Entry::new(&float::MODULE, 0, &float::Params::DEFAULT)],
    rules: &[RuleEntry::new(&breed::LOVE, &breed::Params { foods: FOOD, baby_age: 48000, ..breed::Params::DEFAULT })],
    brain: BrainDef {
        sensors: &[
            SensorEntry::new(&living::SENSOR, 20),
            SensorEntry::new(&hurt_by::SENSOR, 20),
            SensorEntry::new(&nearest_players::SENSOR, 20),
            SensorEntry::new(&tempting::SENSOR, 20).with(&tempting::Params { range: 10.0, foods: FOOD }),
        ],
        activities: &[
            ActivityDef { name: "core", slot: 1, requires: &[] },
            ActivityDef { name: "dig", slot: 3, requires: &[(&IS_PANICKING, Need::Absent), (&WALK_TARGET, Need::Absent), (&sniff::SNIFFER_DIGGING, Need::Present)] },
            ActivityDef { name: "idle", slot: 5, requires: &[(&sniff::SNIFFER_DIGGING, Need::Absent)] },
            ActivityDef { name: "sniff", slot: 6, requires: &[(&IS_PANICKING, Need::Absent), (&sniff::SNIFFER_SNIFFING_TARGET, Need::Present), (&WALK_TARGET, Need::Present)] },
        ],
        pick: &["dig", "sniff", "idle"],
        initial: &[],
        behaviours: &[
            BEntry { behaviour: &sniff::DIGGING_B, activity: "dig", priority: 0, params: &SNIFF, needs: &[], duration: Some((160, 180)), kids: &[] },
            BEntry { behaviour: &sniff::FINISHED_DIGGING_B, activity: "dig", priority: 0, params: &SNIFF, needs: &[], duration: Some((40, 40)), kids: &[] },
            BEntry { behaviour: &make_love::BEHAVIOUR, activity: "idle", priority: 0, params: &MAKE_LOVE, needs: &[(&VISIBLE_MOBS, Need::Present), (&BREED_TARGET, Need::Absent), (&IS_PANICKING, Need::Absent)], duration: Some((110, 110)), kids: &[] },
            BEntry { behaviour: &sniff::SEARCHING_B, activity: "sniff", priority: 0, params: &SNIFF, needs: &[], duration: Some((600, 600)), kids: &[] },
            BEntry { behaviour: &panic::BEHAVIOUR, activity: "core", priority: 1, params: &panic::Params { speed: 2.0 }, needs: &[], duration: Some((100, 120)), kids: &[] },
            BEntry { behaviour: &follow_temptation::BEHAVIOUR, activity: "idle", priority: 1, params: &TEMPT, needs: &[(&TEMPTING_PLAYER, Need::Present), (&TEMPTATION_COOLDOWN_TICKS, Need::Absent), (&BREED_TARGET, Need::Absent), (&IS_PANICKING, Need::Absent)], duration: Some((60, 60)), kids: &[] },
            BEntry { behaviour: &move_to_target_sink::BEHAVIOUR, activity: "core", priority: 2, params: &(), needs: &[(&PATH, Need::Absent), (&WALK_TARGET, Need::Present)], duration: Some((500, 700)), kids: &[] },
            BEntry { behaviour: &look_at_target_sink::BEHAVIOUR, activity: "idle", priority: 2, params: &(), needs: &[(&LOOK_TARGET, Need::Present)], duration: Some((45, 90)), kids: &[] },
            BEntry { behaviour: &countdown::BEHAVIOUR, activity: "core", priority: 3, params: &COUNTDOWN_TEMPTATION, needs: &[(&TEMPTATION_COOLDOWN_TICKS, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &sniff::FEELING_HAPPY_B, activity: "idle", priority: 3, params: &SNIFF, needs: &[(&sniff::SNIFFER_HAPPY, Need::Present)], duration: Some((40, 100)), kids: &[] },
            BEntry {
                behaviour: &run_one::BEHAVIOUR,
                activity: "idle",
                priority: 4,
                params: &(),
                needs: &[],
                duration: None,
                kids: &[
                    Kid { behaviour: &walk_to_look_target::BEHAVIOUR, weight: 2, params: &WALK_LOOK, needs: &[(&WALK_TARGET, Need::Absent), (&LOOK_TARGET, Need::Present)], duration: None },
                    Kid { behaviour: &sniff::SCENTING_B, weight: 1, params: &SNIFF, needs: &[], duration: Some((40, 80)) },
                    Kid { behaviour: &sniff::SNIFFING_B, weight: 1, params: &SNIFF, needs: &[], duration: Some((40, 80)) },
                    Kid { behaviour: &look_at_player::BEHAVIOUR, weight: 1, params: &(), needs: &[(&LOOK_TARGET, Need::Absent), (&VISIBLE_MOBS, Need::Present)], duration: None },
                    Kid { behaviour: &stroll::BEHAVIOUR, weight: 1, params: &STROLL, needs: &[(&WALK_TARGET, Need::Absent)], duration: None },
                    Kid { behaviour: &do_nothing::BEHAVIOUR, weight: 2, params: &(), needs: &[], duration: Some((5, 20)) },
                ],
            },
        ],
    },
    ..Def::EMPTY
};
