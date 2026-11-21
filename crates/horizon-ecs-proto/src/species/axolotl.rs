//! `Axolotl` (`AxolotlAi`, `animal_brain.rs:2849-2924`): its swim (amphibious navigation, `SmoothSwimmingMoveControl(85, 10, 0.1, 0.5, false)`, the push at
//! the mob's speed, no sinking) and its brain. Four activities (core, idle, fight, play dead; the active one is the first whose memory holds: play
//! dead, fight, idle). Idle: look at players, make love (`make_love`, speed 0.2), follow the tempting player or a same-species adult (a baby, 5 to 17
//! blocks), start an attack on the nearest attackable, find water, and the swim stroll / stroll / walk to the look target / stay put. Fight: stop,
//! chase, bite (20 ticks), forget the target when it has a breed target. Play dead: the 200 ticks, with Regeneration. Not done: the
//! `AxolotlSupportPlayer` event (help the player whose blow killed the prey), the stop of the move and look controls while playing dead (the
//! nav is stopped and the walk and look cleared; a drift in the water stays), the target selectors of other creatures that skip an axolotl
//! playing dead (`brain::behaviours::play_dead::is_playing_dead` is there to read).
use super::Def;
use crate::behaviours::{breed, Cap, Entry};
use crate::brain::behaviours::{find_liquid, follow_adult, follow_temptation, forget_memory, gate_all, look_at_player_sometimes, look_at_target_sink, make_love, melee_attack, move_to_target_sink, play_dead, run_one, set_walk_target_from_attack_target, start_attacking, stay_in_fluid, stop_attacking_if_target_invalid, stroll, swim_stroll, walk_to_look_target};
use crate::brain::sensors::{adult, attackables, hurt_by, living, tempting};
use crate::brain::*;
use crate::locomotion::{swim, walk, Medium};
use crate::rules::RuleEntry;

const FOOD: &[&str] = &["minecraft:tropical_fish_bucket"];
const START_ATTACK: start_attacking::Params = start_attacking::Params { finder: start_attacking::Finder::NearestAttackable, ..start_attacking::Params::DEFAULT };
const STOP_ATTACK: stop_attacking_if_target_invalid::Params = stop_attacking_if_target_invalid::Params { not_playing_dead: true, hunting_cooldown: Some(2400), ..stop_attacking_if_target_invalid::Params::DEFAULT };
const CHASE: set_walk_target_from_attack_target::Params = set_walk_target_from_attack_target::Params { speed: 0.6, dry_speed: Some(0.15) };
const BITE: melee_attack::Params = melee_attack::Params { cooldown: 20, ..melee_attack::Params::DEFAULT };
const FORGET_TARGET: forget_memory::Params = forget_memory::Params { forget: &ATTACK_TARGET, clear_target: true, hunting_cooldown: Some(2400) };
const FORGET_DEAD: forget_memory::Params = forget_memory::Params { forget: &play_dead::PLAY_DEAD_TICKS, clear_target: false, hunting_cooldown: None };
const MAKE_LOVE: make_love::Params = make_love::Params { speed: 0.2, ..make_love::Params::DEFAULT };
const TEMPT: follow_temptation::Params = follow_temptation::Params { speed: 1.25, close: 2.5, look_in_the_eyes: false, dry_speed: Some(0.15) };
const FOLLOW: follow_adult::Params = follow_adult::Params { speed: 0.6, min: 5.0, max: 17.0, close: 4, eyes: false, player: false, dry_speed: Some(0.15) };
const LOOK_SOMETIMES: look_at_player_sometimes::Params = look_at_player_sometimes::Params { range_sqr: 36.0, min: 30, spread: 31, players_only: true };
const SWIM_STROLL: swim_stroll::Params = swim_stroll::Params::DEFAULT;
const LAND_STROLL: stroll::Params = stroll::Params { speed: 0.15, h: 10, v: 7, dry_only: true };
const WALK_LOOK: walk_to_look_target::Params = walk_to_look_target::Params { speed: 0.5, close: 3, needs_same_fluid: true, dry_speed: Some(0.15) };
const STAY_WATER: stay_in_fluid::Params = stay_in_fluid::Params { what: stay_in_fluid::Stay::Water };
const STAY_GROUND: stay_in_fluid::Params = stay_in_fluid::Params { what: stay_in_fluid::Stay::Ground };

/// The axolotl's walk speed in water is 0.5 (stroll, tempting) or 0.6 (chase, follow); `FollowTemptation` is 0.5 there (`axolotl_speed(m, false)`).
const TEMPT_WET: follow_temptation::Params = follow_temptation::Params { speed: 0.5, ..TEMPT };

/// `Axolotl.Variant` ids (lucy 0, wild 1, gold 2, cyan 3, blue 4), saved as the int `Variant`; the draw is in `DRAWS`.
static VARIANT: crate::rules::variant::Table = crate::rules::variant::Table {
    bound: 0,
    rows: &[],
    otherwise: crate::rules::variant::Row { tag: None, cuts: &[], last: 0 },
    key: "Variant",
    pick: crate::rules::variant::Pick::Rows,
    redraw: false,
    ids: &[0, 1, 2, 3, 4],
    names: &[],
    traits: &[],
    group_baby: None,
    inherit: crate::rules::variant::Inherit::None,
};

/// `Axolotl.finalizeSpawn` (not a bucket): a new group draws two common variants (`Util.getRandom` over the four common ones, blue is the
/// rare one), each member takes one of the two (`nextInt(2)`), a member after two others is a baby; the group makes no baby of its own.
/// (`useRareVariant`, 1 in 1200, is the breeding's draw, not the spawn's.)
static DRAWS: crate::behaviours::spawn_draw::Draws = crate::behaviours::spawn_draw::Draws {
    group: None,
    list: &[crate::behaviours::spawn_draw::Draw {
        when: crate::behaviours::spawn_draw::When::Before,
        kind: crate::behaviours::spawn_draw::Kind::Shared {
            start: &[crate::rules::spawn_draw::Roll::Pick(&[0, 1, 2, 3]), crate::rules::spawn_draw::Roll::Pick(&[0, 1, 2, 3])],
            odds: None,
            member: crate::behaviours::spawn_draw::Member::OneOf,
            free: &crate::rules::spawn_draw::Roll::Const(0),
            baby_from: Some(2),
            should_baby: false,
        },
        store: crate::behaviours::spawn_draw::Store::Variant,
    }],
};

pub static DEF: Def = Def {
    name: "minecraft:axolotl",
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Brain]),
    variant: Some(&VARIANT),
    draws: Some(&DRAWS),
    locomotion: &[(Medium::Ground, Entry::new(&walk::MODULE, 0, &walk::Params { smooth_land: Some((10.0, 0.5)), ..walk::Params::DEFAULT })), (Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params { needs_water: false, amphibious: Some(false), control: swim::Control::Smooth { max_turn_x: 85.0, max_turn_y: 10.0, in_water: 0.1, outside: 0.5, gravity: false }, push: swim::Push::Speed, sinks: false, ..swim::Params::DEFAULT }))],
    rules: &[RuleEntry::new(&breed::LOVE, &breed::Params { foods: FOOD, ..breed::Params::DEFAULT })],
    brain: BrainDef {
        sensors: &[
            SensorEntry::new(&living::SENSOR, 20),
            SensorEntry::new(&adult::SAME, 20),
            SensorEntry::new(&hurt_by::SENSOR, 20),
            SensorEntry::new(&attackables::SENSOR, 20).with(&attackables::Params::DEFAULT),
            SensorEntry::new(&tempting::SENSOR, 20).with(&tempting::Params { range: 10.0, foods: FOOD }),
        ],
        activities: &[
            ActivityDef { name: "core", slot: 1, requires: &[] },
            ActivityDef { name: "idle", slot: 5, requires: &[] },
            ActivityDef { name: "fight", slot: 14, requires: &[(&ATTACK_TARGET, Need::Present)] },
            ActivityDef { name: "play_dead", slot: 15, requires: &[(&play_dead::PLAY_DEAD_TICKS, Need::Present)] },
        ],
        pick: &["play_dead", "fight", "idle"],
        initial: &[],
        behaviours: &[
            BEntry { behaviour: &look_at_target_sink::BEHAVIOUR, activity: "core", priority: 0, params: &(), needs: &[(&LOOK_TARGET, Need::Present)], duration: Some((45, 90)), kids: &[] },
            BEntry { behaviour: &look_at_player_sometimes::BEHAVIOUR, activity: "idle", priority: 0, params: &LOOK_SOMETIMES, needs: &[(&LOOK_TARGET, Need::Absent), (&VISIBLE_MOBS, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &stop_attacking_if_target_invalid::BEHAVIOUR, activity: "fight", priority: 0, params: &STOP_ATTACK, needs: &[(&ATTACK_TARGET, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &play_dead::BEHAVIOUR, activity: "play_dead", priority: 0, params: &play_dead::Params::DEFAULT, needs: &[(&play_dead::PLAY_DEAD_TICKS, Need::Present), (&HURT_BY_ENTITY, Need::Present)], duration: Some((200, 200)), kids: &[] },
            BEntry { behaviour: &move_to_target_sink::BEHAVIOUR, activity: "core", priority: 1, params: &(), needs: &[(&PATH, Need::Absent), (&WALK_TARGET, Need::Present)], duration: Some((150, 250)), kids: &[] },
            BEntry { behaviour: &make_love::BEHAVIOUR, activity: "idle", priority: 1, params: &MAKE_LOVE, needs: &[(&VISIBLE_MOBS, Need::Present), (&BREED_TARGET, Need::Absent), (&IS_PANICKING, Need::Absent)], duration: Some((110, 110)), kids: &[] },
            BEntry { behaviour: &set_walk_target_from_attack_target::BEHAVIOUR, activity: "fight", priority: 1, params: &CHASE, needs: &[(&ATTACK_TARGET, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &forget_memory::BEHAVIOUR, activity: "play_dead", priority: 1, params: &FORGET_DEAD, needs: &[(&BREED_TARGET, Need::Present), (&play_dead::PLAY_DEAD_TICKS, Need::Present)], duration: None, kids: &[] },
            BEntry {
                behaviour: &run_one::QUIET,
                activity: "idle",
                priority: 2,
                params: &(),
                needs: &[],
                duration: None,
                kids: &[
                    Kid { behaviour: &follow_temptation::BEHAVIOUR, weight: 1, params: &TEMPT_WET, needs: &[(&TEMPTING_PLAYER, Need::Present), (&TEMPTATION_COOLDOWN_TICKS, Need::Absent), (&BREED_TARGET, Need::Absent), (&IS_PANICKING, Need::Absent)], duration: Some((60, 60)) },
                    Kid { behaviour: &follow_adult::BEHAVIOUR, weight: 1, params: &FOLLOW, needs: &[(&NEAREST_VISIBLE_ADULT, Need::Present)], duration: None },
                ],
            },
            BEntry { behaviour: &melee_attack::BEHAVIOUR, activity: "fight", priority: 2, params: &BITE, needs: &[(&ATTACK_TARGET, Need::Present), (&ATTACK_COOLING_DOWN, Need::Absent)], duration: None, kids: &[] },
            BEntry { behaviour: &start_attacking::BEHAVIOUR, activity: "idle", priority: 3, params: &START_ATTACK, needs: &[(&ATTACK_TARGET, Need::Absent), (&BREED_TARGET, Need::Absent), (&attackables::NEAREST_ATTACKABLE, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &find_liquid::BEHAVIOUR, activity: "idle", priority: 3, params: &find_liquid::Params::DEFAULT, needs: &[(&WALK_TARGET, Need::Absent), (&ATTACK_TARGET, Need::Absent)], duration: None, kids: &[] },
            BEntry { behaviour: &forget_memory::BEHAVIOUR, activity: "fight", priority: 3, params: &FORGET_TARGET, needs: &[(&BREED_TARGET, Need::Present), (&ATTACK_TARGET, Need::Present)], duration: None, kids: &[] },
            BEntry {
                behaviour: &gate_all::QUIET,
                activity: "idle",
                priority: 4,
                params: &(),
                needs: &[(&WALK_TARGET, Need::Absent)],
                duration: None,
                kids: &[
                    Kid { behaviour: &swim_stroll::BEHAVIOUR, weight: 2, params: &SWIM_STROLL, needs: &[], duration: None },
                    Kid { behaviour: &stroll::BEHAVIOUR, weight: 2, params: &LAND_STROLL, needs: &[], duration: None },
                    Kid { behaviour: &walk_to_look_target::BEHAVIOUR, weight: 3, params: &WALK_LOOK, needs: &[(&LOOK_TARGET, Need::Present)], duration: None },
                    Kid { behaviour: &stay_in_fluid::BEHAVIOUR, weight: 5, params: &STAY_WATER, needs: &[], duration: None },
                    Kid { behaviour: &stay_in_fluid::BEHAVIOUR, weight: 5, params: &STAY_GROUND, needs: &[], duration: None },
                ],
            },
        ],
    },
    ..Def::EMPTY
};
