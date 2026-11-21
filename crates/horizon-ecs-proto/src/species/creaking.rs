//! `Creaking` and `CreakingAi` (`mobs/creaking.rs`, `monster_brain.rs:1517-1565`), by configuration: the brain's generic behaviours,
//! the generic combat (`start_attacking`, `set_walk_target_from_attack_target`, `melee_attack`, `stop_attacking_if_target_invalid`)
//! and the module `stare_freeze`, which carries the species' own rules (`is_active`, `can_move`: it stands still while a player looks
//! at it and is roused by a stare from within 12 blocks). The Mob's `Swim` is the `float` goal. Not ported: the heart (home, damage
//! protection, tear-down), the animations, the sounds and game events of the freeze and the activation, `playerIsStuckInYou`.
//! DOUTES: the gaze disguise list is empty (the item tag is not read); the swim jump is not suppressed while stilled; the stare is
//! asked again by event (view change, player move, own cell change), not every tick.
use super::Def;
use crate::behaviours::{float, stare_freeze, Cap, Entry};
use crate::brain::behaviours::{do_nothing, look_at_player_sometimes, look_at_target_sink, melee_attack, move_to_target_sink, run_one, set_walk_target_from_attack_target, start_attacking, stop_attacking_if_target_invalid, stroll, walk_to_look_target};
use crate::brain::sensors::{living, nearest_players};
use crate::brain::*;

pub static DEF: Def = Def {
    name: "minecraft:creaking",
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Brain]),
    goals: &[Entry::new(&float::MODULE, 0, &float::Params::DEFAULT), Entry::new(&stare_freeze::MODULE, 0, &stare_freeze::Params::DEFAULT)],
    brain: BrainDef {
        sensors: &[SensorEntry::new(&living::SENSOR, 20), SensorEntry::new(&nearest_players::SENSOR, 20)],
        activities: &[
            ActivityDef { name: "core", slot: 1, requires: &[] },
            ActivityDef { name: "idle", slot: 5, requires: &[] },
            ActivityDef { name: "fight", slot: 14, requires: &[(&ATTACK_TARGET, Need::Present), (&stare_freeze::STILLED, Need::Absent)] },
        ],
        pick: &["fight", "idle"],
        initial: &[],
        behaviours: &[
            BEntry { behaviour: &look_at_target_sink::BEHAVIOUR, activity: "core", priority: 1, params: &(), needs: &[(&LOOK_TARGET, Need::Present)], duration: Some((45, 90)), kids: &[] },
            BEntry { behaviour: &move_to_target_sink::BEHAVIOUR, activity: "core", priority: 2, params: &(), needs: &[(&PATH, Need::Absent), (&WALK_TARGET, Need::Present)], duration: Some((150, 250)), kids: &[] },
            BEntry {
                behaviour: &start_attacking::BEHAVIOUR,
                activity: "idle",
                priority: 10,
                params: &start_attacking::Params { finder: start_attacking::Finder::NearestVisiblePlayer, active: stare_freeze::is_roused, ..start_attacking::Params::DEFAULT },
                needs: &[(&ATTACK_TARGET, Need::Absent)],
                duration: None,
                kids: &[],
            },
            BEntry {
                behaviour: &look_at_player_sometimes::BEHAVIOUR,
                activity: "idle",
                priority: 11,
                params: &look_at_player_sometimes::Params { range_sqr: 64.0, min: 30, spread: 31, players_only: false },
                needs: &[(&LOOK_TARGET, Need::Absent), (&VISIBLE_MOBS, Need::Present)],
                duration: None,
                kids: &[],
            },
            BEntry {
                behaviour: &run_one::QUIET,
                activity: "idle",
                priority: 12,
                params: &(),
                needs: &[],
                duration: None,
                kids: &[
                    Kid { behaviour: &stroll::BEHAVIOUR, weight: 2, params: &stroll::Params { speed: 0.3, h: 10, v: 7, dry_only: false }, needs: &[], duration: None },
                    Kid { behaviour: &walk_to_look_target::BEHAVIOUR, weight: 2, params: &walk_to_look_target::Params { speed: 0.3, close: 3, needs_same_fluid: false, dry_speed: None }, needs: &[(&WALK_TARGET, Need::Absent), (&LOOK_TARGET, Need::Present)], duration: None },
                    Kid { behaviour: &do_nothing::BEHAVIOUR, weight: 1, params: &(), needs: &[], duration: Some((30, 60)) },
                ],
            },
            BEntry { behaviour: &set_walk_target_from_attack_target::BEHAVIOUR, activity: "fight", priority: 10, params: &set_walk_target_from_attack_target::Params::DEFAULT, needs: &[(&ATTACK_TARGET, Need::Present)], duration: None, kids: &[] },
            BEntry {
                behaviour: &melee_attack::BEHAVIOUR,
                activity: "fight",
                priority: 11,
                params: &melee_attack::Params { cooldown: 40, can_move: stare_freeze::can_move },
                needs: &[(&ATTACK_TARGET, Need::Present), (&ATTACK_COOLING_DOWN, Need::Absent)],
                duration: None,
                kids: &[],
            },
            BEntry {
                behaviour: &stop_attacking_if_target_invalid::BEHAVIOUR,
                activity: "fight",
                priority: 12,
                params: &stop_attacking_if_target_invalid::Params { tired_ticks: 200, still_visible: true, attackable: None, not_playing_dead: false, hunting_cooldown: None },
                needs: &[(&ATTACK_TARGET, Need::Present)],
                duration: None,
                kids: &[],
            },
        ],
    },
    ..Def::EMPTY
};
