//! `Fox.registerGoals` and `Fox.registerTargetGoals` (`mobs/fox.rs:267-332`) as a species. Every number and mask of the fox is here;
//! the modules are generic. Left out (see `suivi/ecs-portage/reste/fox.md`): the spawn (`spawn: None`), the held item at spawn, the
//! trust of a player in the avoidance (`player_when` takes no world), the `clearStates` on start, the bite sound and the reset of
//! INTERESTED (no melee hook), the saving of `Trust`, and the school of fish (`interval` only).
use super::Def;
use crate::behaviours::{
    avoid_entity::{self, Avoid},
    breed, climb_on_powder_snow, defend_trusted, float, follow_parent, fox_eat_berries, fox_faceplant, fox_pick_up_loot, fox_pounce,
    fox_search_for_items, fox_sleep, fox_stalk_prey, fox_stroll_village, leap_at_target, lookout, look_at_player, melee,
    nearest_target, panic, shelter, stroll, trust,
};
use crate::behaviours::{Cap, Cond, Entry};

/// The pose bits (`Fox` entity data: sitting, crouching, interested, pouncing, sleeping, faceplanted, defending).
const SITTING: u8 = 1;
const CROUCHING: u8 = 4;
const POUNCING: u8 = 16;
const SLEEPING: u8 = 32;
const DEFENDING: u8 = 128;

/// The food of the animal: the items it eats, for breeding and for the pick-up (DOUBT: the vanilla tag `fox_food` is not in the repo).
const FOOD: &[&str] = &["minecraft:sweet_berries", "minecraft:glow_berries"];
const PREY: &[&str] = &["minecraft:chicken", "minecraft:rabbit"];
const FISH: &[&str] = &["minecraft:cod", "minecraft:salmon", "minecraft:tropical_fish"];

/// The kinds the variant (red or snow) gates: snow foxes hunt the fish, the red ones the prey (`SnowFox`, `fox:snow`).
static SNOW: Cond = Cond::Is("variant:snow");
static RED: Cond = Cond::Not(&Cond::Is("variant:snow"));
static CAN_BITE: Cond = Cond::Is("pose:can_bite");
static LOOK_FREE: Cond = Cond::Is("pose:look_free");

/// `Fox.AvoidEntityGoal` for players: not a sneaking nor a spectator player.
fn player_when(ps: &crate::hostile::PlayerState, p: &crate::goals::Player) -> bool {
    !ps.sneaking(p.id) && !ps.spectator(p.id)
}

pub static DEF: Def = Def {
    name: "minecraft:fox",
    reference: None,
    targets: &[
        Entry::new(&defend_trusted::MODULE, 3, &defend_trusted::Params { follow_range: 32.0, unseen_memory: 60, raise: DEFENDING, lower: SLEEPING, sound: "entity.fox.aggro" }),
        Entry::new(&nearest_target::MODULE, 4, &nearest_target::Params { types: PREY, must_see: false, ..nearest_target::Params::DEFAULT }).when(&RED),
        Entry::new(&nearest_target::MODULE, 4, &nearest_target::Params { types: &["minecraft:turtle"], predicate: Some("baby_on_land"), must_see: false, ..nearest_target::Params::DEFAULT }).when(&RED),
        Entry::new(&nearest_target::MODULE, 6, &nearest_target::Params { types: FISH, must_see: false, interval: 20, ..nearest_target::Params::DEFAULT }).when(&RED),
        Entry::new(&nearest_target::MODULE, 4, &nearest_target::Params { types: FISH, must_see: false, interval: 20, ..nearest_target::Params::DEFAULT }).when(&SNOW),
        Entry::new(&nearest_target::MODULE, 6, &nearest_target::Params { types: PREY, must_see: false, ..nearest_target::Params::DEFAULT }).when(&SNOW),
        Entry::new(&nearest_target::MODULE, 6, &nearest_target::Params { types: &["minecraft:turtle"], predicate: Some("baby_on_land"), must_see: false, ..nearest_target::Params::DEFAULT }).when(&SNOW),
    ],
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&climb_on_powder_snow::MODULE, 0, &climb_on_powder_snow::Params::DEFAULT),
        Entry::new(&fox_faceplant::MODULE, 1, &fox_faceplant::Params::DEFAULT),
        Entry::new(&panic::MODULE, 2, &panic::Params { speed: 2.2, ..panic::Params::DEFAULT }),
        Entry::new(&breed::MODULE, 3, &breed::Params { speed: 1.0, foods: FOOD, ..breed::Params::DEFAULT }),
        Entry::new(&avoid_entity::MODULE, 4, &avoid_entity::Params { what: Avoid::Player, max_dist: 16.0, walk: 1.6, sprint: 1.4, avoider_when: None, player_when: Some(player_when), target_when: None, target_roll: None }),
        Entry::new(&avoid_entity::MODULE, 4, &avoid_entity::Params { what: Avoid::Named(&["minecraft:wolf"]), max_dist: 8.0, walk: 1.6, sprint: 1.4, avoider_when: Some(defend_trusted::is_defending), player_when: None, target_when: Some(trust::untamed), target_roll: None }),
        Entry::new(&avoid_entity::MODULE, 4, &avoid_entity::Params { what: Avoid::Named(&["minecraft:polar_bear"]), max_dist: 8.0, walk: 1.6, sprint: 1.4, avoider_when: Some(defend_trusted::is_defending), player_when: None, target_when: None, target_roll: None }),
        Entry::new(&fox_stalk_prey::MODULE, 5, &fox_stalk_prey::Params { types: PREY, ..fox_stalk_prey::Params::DEFAULT }).when(&LOOK_FREE),
        Entry::new(&fox_pounce::MODULE, 6, &fox_pounce::Params::DEFAULT),
        Entry::new(&shelter::MODULE, 6, &shelter::Params { speed: 1.25, interval: 100, blocked: SLEEPING }),
        Entry::new(&melee::MODULE, 7, &melee::Params { speed: 1.2, follow_unseen: true, ..melee::Params::DEFAULT }).when(&CAN_BITE),
        Entry::new(&fox_sleep::MODULE, 7, &fox_sleep::Params::DEFAULT),
        Entry::new(&follow_parent::MODULE, 8, &follow_parent::Params { speed: 1.25 }),
        Entry::new(&fox_stroll_village::MODULE, 9, &fox_stroll_village::Params::DEFAULT),
        Entry::new(&fox_eat_berries::MODULE, 10, &fox_eat_berries::Params::DEFAULT),
        Entry::new(&leap_at_target::MODULE, 10, &leap_at_target::Params { yd: 0.4 }),
        Entry::new(&stroll::MODULE, 11, &stroll::Params::DEFAULT),
        Entry::new(&fox_search_for_items::MODULE, 11, &fox_search_for_items::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 12, &look_at_player::Params { range: 24.0, ..look_at_player::Params::DEFAULT }).when(&LOOK_FREE),
        Entry::new(&lookout::MODULE, 13, &lookout::Params { chance: 0.02, blocked: SLEEPING | POUNCING | CROUCHING, sit: SITTING, alarm: Some(fox_sleep::alarmed) }),
        Entry::new(&fox_pick_up_loot::MODULE, 0, &fox_pick_up_loot::Params { foods: FOOD, ..fox_pick_up_loot::Params::DEFAULT }),
    ],
    changes: &[],
    rules: &[],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::IgnoresBerryBush]),
    brain: crate::brain::BrainDef::EMPTY,
    locomotion: &[],
    spawn: None,
    ..Def::EMPTY
};
