//! `Breeze` and `BreezeAi` (`mobs/breeze.rs`, `monster_brain.rs:1475-1515`), by configuration: the brain's generic behaviours, the generic
//! combat (`start_attacking` with the finders `NearestAttackable` and `HurtBy`, `stop_attacking_if_target_invalid` with the window of 100
//! ticks), and its own four behaviours (`brain/behaviours/breeze.rs`: shoot, shoot when stuck, slide, long jump) with the sensor
//! `attackable`. The wind charge is the `Flight::BREEZE_WIND_CHARGE` of the frame (burst of radius 3: knockback of the creatures).
//! The Mob's `Swim` is the `float` goal. Not ported: sounds, particles, the pose animations, `Breeze.isInvulnerableTo` (another breeze's
//! charges: the burst spares the owner only), the jump boost and levitation effects, the mount, the burst's push of players and of blocks.
//! DOUTES: the attackable window is refreshed at the sensor's scans (20 ticks) and ends at a deadline of the wheel; the Mob refreshes it every tick.
use super::Def;
use crate::behaviours::{float, Cap, Entry};
use crate::brain::behaviours::{breeze, do_nothing, look_at_target_sink, run_one, start_attacking, stop_attacking_if_target_invalid, stroll};
use crate::brain::sensors::{attackable, hurt_by, living, nearest_players};
use crate::brain::*;
use crate::targeting::{self, Who};
use bevy_ecs::prelude::{Entity, World};

const GOLEM: &str = "minecraft:iron_golem";
/// `Breeze.canAttack`: players and iron golems.
fn players_and_golems(w: &World, who: Who) -> bool {
    match who {
        Who::Player(_) => true,
        Who::Entity(_) => targeting::info(w, who).is_some_and(|i| crate::species::registry().id(GOLEM) == Some(usize::from(i.species))),
    }
}
pub static DEF: Def = Def {
    name: "minecraft:breeze",
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Brain]),
    goals: &[Entry::new(&float::MODULE, 0, &float::Params::DEFAULT)],
    brain: BrainDef {
        sensors: &[
            SensorEntry::new(&living::SENSOR, 20),
            SensorEntry::new(&hurt_by::SENSOR, 20),
            SensorEntry::new(&nearest_players::SENSOR, 20),
            SensorEntry::new(&attackable::SENSOR, 20).with(&attackable::Params { creature: Some(GOLEM), window: 100 }),
        ],
        activities: &[
            ActivityDef { name: "core", slot: 1, requires: &[] },
            ActivityDef { name: "idle", slot: 5, requires: &[] },
            ActivityDef { name: "fight", slot: 14, requires: &[(&ATTACK_TARGET, Need::Present), (&WALK_TARGET, Need::Absent)] },
        ],
        pick: &["fight", "idle"],
        initial: &[],
        behaviours: &[
            BEntry { behaviour: &look_at_target_sink::BEHAVIOUR, activity: "core", priority: 1, params: &(), needs: &[(&LOOK_TARGET, Need::Present)], duration: Some((45, 90)), kids: &[] },
            BEntry {
                behaviour: &start_attacking::BEHAVIOUR,
                activity: "idle",
                priority: 0,
                params: &start_attacking::Params { finder: start_attacking::Finder::NearestAttackable, allowed: players_and_golems, ..start_attacking::Params::DEFAULT },
                needs: &[(&ATTACK_TARGET, Need::Absent)],
                duration: None,
                kids: &[],
            },
            BEntry {
                behaviour: &start_attacking::BEHAVIOUR,
                activity: "idle",
                priority: 1,
                params: &start_attacking::Params { finder: start_attacking::Finder::HurtBy, allowed: players_and_golems, ..start_attacking::Params::DEFAULT },
                needs: &[(&ATTACK_TARGET, Need::Absent)],
                duration: None,
                kids: &[],
            },
            BEntry { behaviour: &breeze::SLIDE_SINK, activity: "idle", priority: 2, params: &(), needs: &[(&PATH, Need::Absent), (&WALK_TARGET, Need::Present)], duration: Some((20, 40)), kids: &[] },
            BEntry {
                behaviour: &run_one::BEHAVIOUR,
                activity: "idle",
                priority: 3,
                params: &(),
                needs: &[],
                duration: None,
                kids: &[
                    Kid { behaviour: &do_nothing::BEHAVIOUR, weight: 1, params: &(), needs: &[], duration: Some((20, 100)) },
                    Kid { behaviour: &stroll::BEHAVIOUR, weight: 2, params: &stroll::Params { speed: 0.6, h: 10, v: 7, dry_only: false }, needs: &[], duration: None },
                ],
            },
            BEntry {
                behaviour: &stop_attacking_if_target_invalid::BEHAVIOUR,
                activity: "fight",
                priority: 0,
                params: &stop_attacking_if_target_invalid::Params { tired_ticks: 200, still_visible: false, attackable: Some(100), not_playing_dead: false, hunting_cooldown: None },
                needs: &[(&ATTACK_TARGET, Need::Present)],
                duration: None,
                kids: &[],
            },
            BEntry {
                behaviour: &breeze::SHOOT,
                activity: "fight",
                priority: 1,
                params: &(),
                needs: &[
                    (&ATTACK_TARGET, Need::Present),
                    (&breeze::SHOOT_COOLDOWN, Need::Absent),
                    (&breeze::SHOOT_CHARGING, Need::Absent),
                    (&breeze::SHOOT_RECOVERING, Need::Absent),
                    (&breeze::SHOOT_MEM, Need::Present),
                    (&WALK_TARGET, Need::Absent),
                    (&breeze::JUMP_TARGET, Need::Absent),
                ],
                duration: Some((20, 20)),
                kids: &[],
            },
            BEntry {
                behaviour: &breeze::LONG_JUMP,
                activity: "fight",
                priority: 2,
                params: &(),
                needs: &[(&ATTACK_TARGET, Need::Present), (&breeze::JUMP_COOLDOWN, Need::Absent), (&breeze::SHOOT_MEM, Need::Absent), (&WALK_TARGET, Need::Absent)],
                duration: Some((200, 200)),
                kids: &[],
            },
            BEntry {
                behaviour: &breeze::SHOOT_WHEN_STUCK,
                activity: "fight",
                priority: 3,
                params: &(),
                needs: &[(&ATTACK_TARGET, Need::Present), (&breeze::JUMP_INHALING, Need::Absent), (&breeze::JUMP_TARGET, Need::Absent), (&WALK_TARGET, Need::Absent), (&breeze::SHOOT_MEM, Need::Absent)],
                duration: Some((60, 60)),
                kids: &[],
            },
            BEntry {
                behaviour: &breeze::SLIDE,
                activity: "fight",
                priority: 4,
                params: &(),
                needs: &[(&ATTACK_TARGET, Need::Present), (&WALK_TARGET, Need::Absent), (&breeze::JUMP_COOLDOWN, Need::Absent), (&breeze::SHOOT_MEM, Need::Absent)],
                duration: Some((60, 60)),
                kids: &[],
            },
        ],
    },
    ..Def::EMPTY
};
