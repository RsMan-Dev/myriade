//! `PiglinBrute` (`mobs/piglin.rs:1463-1482`, `register_brute`; brain `PiglinBruteAi`, javap). Its brain: core (look and move sinks, the
//! door, the anger ending on a dead target, the anger on a blow); fight (stop when invalid, walk to the target out of reach, melee at 20
//! ticks); idle (start attacking the anger, the nearest player or the nemesis; look kids; walk kids: stroll, interact with a piglin or a
//! brute, walk home and around the home block). Not ported: the alert of the other piglins, `SetLookAndInteract`
//! (its kind is not read yet), the piglin's `R:piglin` repellent.
use super::Def;
use crate::behaviours::{Cap, Entry, float};
use crate::brain::behaviours::look_at_entity::species_id;
use crate::brain::behaviours::{
    anger_at_attacker, do_nothing, interact_with, interact_with_door, look_at_entity, look_at_player_sometimes, look_at_target_sink, melee_attack, move_to_target_sink, run_one,
    set_walk_target_from_attack_target, start_attacking, stop_attacking_if_target_invalid, stop_being_angry_if_target_dead, stroll, stroll_around_poi, stroll_to_poi,
};
use crate::brain::sensors::{hurt_by, living, nearest_players, piglin_specific};
use crate::brain::*;
use crate::targeting::{self, Who};
use bevy_ecs::prelude::World;

const BRUTE: &str = "minecraft:piglin_brute";

/// `AbstractPiglin`: a piglin or a brute does not anger the brute.
fn piglin_kin(w: &World, who: Who) -> bool {
    let (piglin, brute) = (species_id(w, crate::species::PIGLIN), species_id(w, BRUTE));
    targeting::info(w, who).is_some_and(|i| Some(i.species) == piglin || Some(i.species) == brute)
}

/// `createIdleLookBehaviors`: a player, a piglin, a brute or any mob within 8 blocks, one at a time (30 to 60 ticks for the last).
static LOOK_KIDS: &[Kid] = &[
    Kid { behaviour: &look_at_entity::BEHAVIOUR, weight: 1, params: &look_at_entity::Params { species: &[], players: true }, needs: &[(&LOOK_TARGET, Need::Absent), (&VISIBLE_MOBS, Need::Present)], duration: None },
    Kid { behaviour: &look_at_entity::BEHAVIOUR, weight: 1, params: &look_at_entity::Params { species: &[crate::species::PIGLIN], players: false }, needs: &[(&LOOK_TARGET, Need::Absent), (&VISIBLE_MOBS, Need::Present)], duration: None },
    Kid { behaviour: &look_at_entity::BEHAVIOUR, weight: 1, params: &look_at_entity::Params { species: &[BRUTE], players: false }, needs: &[(&LOOK_TARGET, Need::Absent), (&VISIBLE_MOBS, Need::Present)], duration: None },
    Kid { behaviour: &look_at_player_sometimes::BEHAVIOUR, weight: 1, params: &look_at_player_sometimes::Params { range_sqr: 64.0, min: 30, spread: 31, players_only: false }, needs: &[(&LOOK_TARGET, Need::Absent), (&VISIBLE_MOBS, Need::Present)], duration: None },
];

/// `createIdleMovementBehaviors`: a random stroll, the two interactions, the walk home and the walk around it, or nothing.
static MOVE_KIDS: &[Kid] = &[
    Kid { behaviour: &stroll::BEHAVIOUR, weight: 2, params: &stroll::Params { speed: 0.6, h: 10, v: 7, dry_only: false }, needs: &[(&WALK_TARGET, Need::Absent)], duration: None },
    Kid { behaviour: &interact_with::BEHAVIOUR, weight: 2, params: &interact_with::Params { species: crate::species::PIGLIN, range: 8.0, speed: 0.6, close: 2 }, needs: &[(&WALK_TARGET, Need::Absent)], duration: None },
    Kid { behaviour: &interact_with::BEHAVIOUR, weight: 2, params: &interact_with::Params { species: BRUTE, range: 8.0, speed: 0.6, close: 2 }, needs: &[(&WALK_TARGET, Need::Absent)], duration: None },
    Kid { behaviour: &stroll_to_poi::BEHAVIOUR, weight: 2, params: &stroll_to_poi::Params { speed: 0.6, close: 2, max: 100.0 }, needs: &[(&WALK_TARGET, Need::Absent)], duration: None },
    Kid { behaviour: &stroll_around_poi::BEHAVIOUR, weight: 2, params: &stroll_around_poi::Params { speed: 0.6, max: 5.0 }, needs: &[(&WALK_TARGET, Need::Absent)], duration: None },
    Kid { behaviour: &do_nothing::BEHAVIOUR, weight: 1, params: &(), needs: &[], duration: Some((30, 60)) },
];

pub static DEF: Def = Def {
    name: "minecraft:piglin_brute",
    goals: &[Entry::new(&float::MODULE, 0, &float::Params::DEFAULT), Entry::new(&crate::behaviours::growl_in_fight::MODULE, 1, &()), Entry::new(&crate::behaviours::convert_timer::MODULE, 0, &crate::behaviours::convert_timer::Params { into: "minecraft:zombified_piglin", after: 300, drop_carried: true })],
    rules: &[crate::rules::RuleEntry::new(&crate::spawn_gear::RULE, &crate::spawn_gear::PIGLIN_BRUTE)],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Brain]),
    brain: BrainDef {
        sensors: &[
            SensorEntry::new(&living::SENSOR, 20),
            SensorEntry::new(&hurt_by::SENSOR, 20),
            SensorEntry::new(&nearest_players::SENSOR, 20),
            SensorEntry::new(&piglin_specific::SENSOR, 20),
        ],
        activities: &[
            ActivityDef { name: "core", slot: 1, requires: &[] },
            ActivityDef { name: "fight", slot: 2, requires: &[(&ATTACK_TARGET, Need::Present)] },
            ActivityDef { name: "idle", slot: 5, requires: &[] },
        ],
        pick: &["fight", "idle"],
        initial: &[],
        behaviours: &[
            BEntry { behaviour: &look_at_target_sink::BEHAVIOUR, activity: "core", priority: 0, params: &(), needs: &[(&LOOK_TARGET, Need::Present)], duration: Some((45, 90)), kids: &[] },
            BEntry { behaviour: &move_to_target_sink::BEHAVIOUR, activity: "core", priority: 1, params: &(), needs: &[(&PATH, Need::Absent), (&WALK_TARGET, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &interact_with_door::BEHAVIOUR, activity: "core", priority: 2, params: &interact_with_door::Params::DEFAULT, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &stop_being_angry_if_target_dead::BEHAVIOUR, activity: "core", priority: 3, params: &(), needs: &[(&ANGRY_AT, Need::Present)], duration: None, kids: &[] },
            // `PiglinBruteAi.wasHurtBy`: the blow of a creature that is not a piglin makes it angry for 600 ticks.
            BEntry { behaviour: &anger_at_attacker::BEHAVIOUR, activity: "core", priority: 4, params: &anger_at_attacker::Params { skip: piglin_kin, ttl: 600 }, needs: &[], duration: None, kids: &[] },
            // `StopAttackingIfTargetInvalid`, `SetWalkTargetFromAttackTargetIfTargetOutOfReach(1.0)`, `MeleeAttack(20)`.
            BEntry { behaviour: &stop_attacking_if_target_invalid::BEHAVIOUR, activity: "fight", priority: 0, params: &stop_attacking_if_target_invalid::Params::DEFAULT, needs: &[(&ATTACK_TARGET, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &set_walk_target_from_attack_target::BEHAVIOUR, activity: "fight", priority: 1, params: &set_walk_target_from_attack_target::Params::DEFAULT, needs: &[(&ATTACK_TARGET, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &melee_attack::BEHAVIOUR, activity: "fight", priority: 2, params: &melee_attack::Params { cooldown: 20, ..melee_attack::Params::DEFAULT }, needs: &[(&ATTACK_TARGET, Need::Present)], duration: None, kids: &[] },
            // `PiglinBruteAi.initIdleActivity`: `StartAttacking` at 0, the look kids and the movement kids as `RunOne`s.
            BEntry {
                behaviour: &start_attacking::BEHAVIOUR,
                activity: "idle",
                priority: 0,
                params: &start_attacking::Params { finder: start_attacking::Finder::Piglin, ..start_attacking::Params::DEFAULT },
                needs: &[(&ATTACK_TARGET, Need::Absent)],
                duration: None,
                kids: &[],
            },
            BEntry { behaviour: &run_one::QUIET, activity: "idle", priority: 1, params: &(), needs: &[], duration: None, kids: LOOK_KIDS },
            BEntry { behaviour: &run_one::QUIET, activity: "idle", priority: 2, params: &(), needs: &[], duration: None, kids: MOVE_KIDS },
        ],
    },
    not_in_peaceful: true,
    sound_source: super::SoundCategory::Hostile,
    ..Def::EMPTY
};
