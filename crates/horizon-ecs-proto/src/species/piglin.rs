//! `Piglin` (`mobs/piglin.rs:1329-1363`, `register_piglin`, brain `PiglinAi`). Goals and targets in the priorities of `register_piglin`;
//! the brain holds the activities of `PiglinAi` (`updateActivity` order: admire, fight, avoid, celebrate, ride, idle). Not ported:
//! the avoidance of repellents (`SetWalkTargetAwayFrom.pos`), the look at 14 blocks of a player holding a loved item, the look
//! at any entity, and the ticker of the baby's ride.
use crate::behaviours::{ambient_sound, convert_timer, float, flee_repellent, hunt_prey, hurt_by, look_at_held, look_at_player, nearest_target, random_look, stroll, walk_away, Cap, Entry};
use crate::brain::behaviours::{
    back_up_if_too_close, copy_memory_with_expiry, crossbow_attack, dismount_or_skip_mounting, do_nothing, erase_memory_if, go_to_target_location,
    go_to_wanted_item, interact_with, interact_with_door, look_and_interact, look_at_entity, look_at_target_sink, melee_attack, mount, move_to_target_sink,
    remember_hoglin_killed, run_one, set_walk_target_from_attack_target, spear_approach, spear_attack, spear_retreat, start_admiring_item_if_seen,
    start_attacking, start_celebrating_if_target_dead, start_hunting_hoglin, stop_admiring_if_item_too_far, stop_admiring_if_tired,
    stop_attacking_if_target_invalid, stop_being_angry_if_target_dead, stop_holding_item_if_no_longer_admiring, stroll as brain_stroll,
    walk_away as avoid_walk, walk_to_look_target,
};
use crate::brain::sensors::{hurt_by as hurt_by_sensor, living, nearest_wanted_item, piglin_specific};
use crate::brain::*;
use crate::cow::Pos;
use crate::brain::behaviours::look_at_entity::species_id;
use crate::brain::behaviours::walk_away::AVOID_TARGET;
use crate::goals::{d2, Slot};
use bevy_ecs::prelude::{Entity, World};
use crate::species::{predicates::Cond, Def};
use crate::targeting::{info, Who};

static HOSTILE: Cond = Cond::Is("horizon:hostile_to_player");

/// `Piglin.isBaby`.
fn is_baby(w: &World, e: Entity) -> bool {
    info(w, Who::Entity(e)).is_some_and(|i| i.baby)
}

/// `hasCrossbow`: a crossbow in the main hand.
fn holds_crossbow(w: &World, e: Entity) -> bool {
    crate::items::item_name(crate::goals::equipped(w, e, Slot::Main)) == Some("minecraft:crossbow")
}

/// `isNearZombified`: a visible zombified piglin within 6 blocks.
fn is_near_zombified(w: &World, e: Entity) -> bool {
    let Some(MemValue::Who(z)) = get(w, e, &NEAREST_VISIBLE_ZOMBIFIED) else { return false };
    let (Some(me), Some(i)) = (w.get::<Pos>(e).map(|p| p.0), info(w, *z)) else { return false };
    d2(me, i.pos) < 36.0
}

/// `wantsToStopFleeing`: a hoglin is left when the piglins equal or outnumber the hoglins (the sensor's adult counts); a
/// zombified piglin is left when it is no longer the nearest one seen.
fn wants_to_stop_fleeing(w: &World, e: Entity) -> bool {
    let Some(MemValue::Who(avoid)) = get(w, e, &AVOID_TARGET) else { return true };
    let avoid = *avoid;
    let Some(i) = info(w, avoid) else { return true };
    if Some(i.species) == species_id(w, "minecraft:hoglin") {
        // `piglinsEqualOrOutnumberHoglins`: not `hoglins > piglins + 1`, the counts of the sensor.
        let n = |k: &MemKey| match get(w, e, k) {
            Some(MemValue::Int(n)) => *n,
            _ => 0,
        };
        return n(&VISIBLE_ADULT_HOGLIN_COUNT) <= n(&VISIBLE_ADULT_PIGLIN_COUNT) + 1;
    }
    if Some(i.species) == species_id(w, "minecraft:zombified_piglin") {
        return !matches!(get(w, e, &NEAREST_VISIBLE_ZOMBIFIED), Some(MemValue::Who(z)) if *z == avoid);
    }
    false
}

/// `wantsToStopRiding(piglin, vehicle)`: a baby vehicle that is alive and has not hurt the rider or been hurt stays; a
/// piglin vehicle with no vehicle of its own is left.
fn wants_to_stop_riding(w: &World, piglin: Entity, vehicle: Entity) -> bool {
    let Some(i) = info(w, Who::Entity(vehicle)).filter(|i| !i.player) else { return false };
    let piglin_vehicle = species_id(w, "minecraft:piglin") == Some(i.species);
    !i.baby
        || i.exempt
        || present(w, piglin, &HURT_BY)
        || present(w, vehicle, &HURT_BY)
        || (piglin_vehicle && crate::seat::vehicle_of(w, vehicle).is_none())
}

/// The look kids of the idle and ride activities (`createLookBehaviors`): a player, a piglin within 8 blocks, and the nothing.
const LOOK_KIDS: &[Kid] = &[
    Kid { behaviour: &look_at_entity::BEHAVIOUR, weight: 1, params: &look_at_entity::Params { species: &[], players: true }, needs: &[], duration: None },
    Kid { behaviour: &look_at_entity::BEHAVIOUR, weight: 1, params: &look_at_entity::Params { species: &["minecraft:piglin"], players: false }, needs: &[], duration: None },
    Kid { behaviour: &do_nothing::BEHAVIOUR, weight: 1, params: &(), needs: &[], duration: Some((30, 60)) },
];

/// `createIdleMovementBehaviors`: a random stroll at 0.6, the walk to the piglin interaction, the walk to the look target, and the nothing.
const MOVE_KIDS: &[Kid] = &[
    Kid { behaviour: &brain_stroll::BEHAVIOUR, weight: 2, params: &brain_stroll::Params { speed: 0.6, h: 10, v: 7, dry_only: false }, needs: &[], duration: None },
    Kid { behaviour: &interact_with::BEHAVIOUR, weight: 2, params: &interact_with::Params { species: "minecraft:piglin", range: 8.0, speed: 0.6, close: 2 }, needs: &[], duration: None },
    Kid { behaviour: &walk_to_look_target::BEHAVIOUR, weight: 2, params: &walk_to_look_target::Params { speed: 0.6, ..walk_to_look_target::Params::DEFAULT }, needs: &[], duration: None },
    Kid { behaviour: &do_nothing::BEHAVIOUR, weight: 1, params: &(), needs: &[], duration: Some((30, 60)) },
];

pub static DEF: Def = Def {
    load_attrs: &[crate::attr_mod::LoadAttr { name: "movement_speed", base: None, when: crate::attr_mod::LoadWhen::Always }, crate::attr_mod::LoadAttr { name: "movement_speed", base: Some((0.2f32 as f64)), when: crate::attr_mod::LoadWhen::BabyBonus }],
    name: "minecraft:piglin",
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert: true, alert_types: &["minecraft:piglin", "minecraft:piglin_brute"], ignoring: &["minecraft:piglin", "minecraft:piglin_brute"], ..hurt_by::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER).when(&HOSTILE),
        Entry::new(&hunt_prey::MODULE, 3, &hunt_prey::Params { prey: &["minecraft:hoglin"], ..hunt_prey::Params::DEFAULT }),
    ],
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&convert_timer::MODULE, 0, &convert_timer::Params { into: "minecraft:zombified_piglin", after: 300, drop_carried: true }),
        Entry::new(&walk_away::MODULE, 2, &walk_away::Params::DEFAULT),
        Entry::new(&crate::behaviours::piglin_eat::MODULE, 3, &()),
        Entry::new(&flee_repellent::MODULE, 2, &flee_repellent::Params {
            blocks: &["minecraft:soul_campfire", "minecraft:soul_torch", "minecraft:soul_wall_torch", "minecraft:soul_lantern"],
            species: &[],
            shape: flee_repellent::Shape::Sphere,
            lit: &["minecraft:soul_campfire"],
            ..flee_repellent::Params::DEFAULT
        }),
        Entry::new(&look_at_held::MODULE, 5, &look_at_held::Params { items: &["minecraft:gold_ingot"], ..look_at_held::Params::DEFAULT }),
        Entry::new(&stroll::MODULE, 5, &stroll::Params { speed: 0.6, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 7, &random_look::Params::DEFAULT),
        Entry::new(&ambient_sound::MODULE, 8, &ambient_sound::Params { sounds: &["minecraft:entity.piglin.ambient"], min: 80, max: 160, ..ambient_sound::Params::DEFAULT }),
    ],
    rules: &[
        // `Piglin.getBaseExperienceReward`: `xpReward` (5) as is; a baby keeps its loot and its experience (`Monster`).
        crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 5, max: 5 }),
        crate::rules::RuleEntry::new(&crate::loot::FLAT_XP, &()),
        crate::rules::RuleEntry::new(&crate::loot::BABY_DROPS, &crate::loot::BabyDrops::MONSTER),
        crate::rules::RuleEntry::new(&crate::guard_alert::RULE, &crate::guard_alert::Params::PIGLIN),
        crate::rules::RuleEntry::new(&crate::spawn_gear::RULE, &crate::spawn_gear::PIGLIN),
        crate::rules::RuleEntry::new(&crate::item_store::STORE, &crate::item_store::Slots::PIGLIN),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Brain]),
    brain: BrainDef {
        sensors: &[
            SensorEntry::new(&living::SENSOR, 20),
            SensorEntry::new(&hurt_by_sensor::SENSOR, 20),
            SensorEntry::new(&nearest_wanted_item::SENSOR, 20).with(&nearest_wanted_item::Params { tag: "minecraft:piglin_loved", range: 9.0 }),
            SensorEntry::new(&piglin_specific::SENSOR, 20),
        ],
        activities: &[
            ActivityDef { name: "core", slot: 1, requires: &[] },
            ActivityDef { name: "admire_item", slot: 2, requires: &[(&ADMIRING_ITEM, Need::Present)] },
            ActivityDef { name: "fight", slot: 3, requires: &[(&ATTACK_TARGET, Need::Present)] },
            ActivityDef { name: "avoid", slot: 4, requires: &[(&AVOID_TARGET, Need::Present)] },
            ActivityDef { name: "celebrate", slot: 5, requires: &[(&CELEBRATE_LOCATION, Need::Present)] },
            ActivityDef { name: "ride", slot: 6, requires: &[(&RIDE_TARGET, Need::Present)] },
            ActivityDef { name: "idle", slot: 7, requires: &[] },
        ],
        pick: &["admire_item", "fight", "avoid", "celebrate", "ride", "idle"],
        initial: &[],
        behaviours: &[
            // `PiglinAi.initCoreActivity`: the look and move sinks, the door, the avoidance of a nemesis (baby) or a zombified piglin,
            // the admiration, the celebration and the anger's end.
            BEntry { behaviour: &look_at_target_sink::BEHAVIOUR, activity: "core", priority: 0, params: &(), needs: &[(&LOOK_TARGET, Need::Present)], duration: Some((45, 90)), kids: &[] },
            BEntry { behaviour: &move_to_target_sink::BEHAVIOUR, activity: "core", priority: 1, params: &(), needs: &[(&PATH, Need::Absent), (&WALK_TARGET, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &interact_with_door::BEHAVIOUR, activity: "core", priority: 2, params: &interact_with_door::Params::DEFAULT, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &copy_memory_with_expiry::BEHAVIOUR, activity: "core", priority: 3, params: &copy_memory_with_expiry::Params { from: &NEAREST_VISIBLE_NEMESIS, to: &avoid_walk::AVOID_TARGET, min: 100, spread: 41, when: is_baby }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &copy_memory_with_expiry::BEHAVIOUR, activity: "core", priority: 4, params: &copy_memory_with_expiry::Params { from: &NEAREST_VISIBLE_ZOMBIFIED, to: &avoid_walk::AVOID_TARGET, min: 100, spread: 41, when: is_near_zombified }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &stop_holding_item_if_no_longer_admiring::BEHAVIOUR, activity: "core", priority: 5, params: &(), needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &start_admiring_item_if_seen::BEHAVIOUR, activity: "core", priority: 6, params: &(), needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &start_celebrating_if_target_dead::BEHAVIOUR, activity: "core", priority: 7, params: &start_celebrating_if_target_dead::Params { duration: 300 }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &stop_being_angry_if_target_dead::BEHAVIOUR, activity: "core", priority: 8, params: &(), needs: &[], duration: None, kids: &[] },
            // `PiglinAi.initAdmireItemActivity`: the walk to the item (range 9), the end when it is too far or the tries run out.
            BEntry { behaviour: &go_to_wanted_item::BEHAVIOUR, activity: "admire_item", priority: 0, params: &go_to_wanted_item::Params { speed: 1.0, range: 9.0 }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &stop_admiring_if_item_too_far::BEHAVIOUR, activity: "admire_item", priority: 1, params: &stop_admiring_if_item_too_far::Params { distance: 9.0 }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &stop_admiring_if_tired::BEHAVIOUR, activity: "admire_item", priority: 2, params: &stop_admiring_if_tired::Params { max_time: 200, ttl: 200 }, needs: &[], duration: None, kids: &[] },
            // `PiglinAi.initFightActivity`: the target's end, the back-off of a crossbow, the walk to the target out of reach, the spear
            // (approach, attack, retreat), the melee, the crossbow, the hoglin it killed is remembered, then the end near a zombified piglin.
            BEntry { behaviour: &stop_attacking_if_target_invalid::BEHAVIOUR, activity: "fight", priority: 0, params: &stop_attacking_if_target_invalid::Params::DEFAULT, needs: &[(&ATTACK_TARGET, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &back_up_if_too_close::BEHAVIOUR, activity: "fight", priority: 1, params: &back_up_if_too_close::Params { distance: 5.0, speed: 0.75, when: holds_crossbow }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &set_walk_target_from_attack_target::BEHAVIOUR, activity: "fight", priority: 2, params: &set_walk_target_from_attack_target::Params::DEFAULT, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &spear_approach::BEHAVIOUR, activity: "fight", priority: 3, params: &spear_approach::Params { speed: 1.0, distance: 10.0, mount_speed: 1.0 }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &spear_attack::BEHAVIOUR, activity: "fight", priority: 4, params: &spear_attack::Params { charging: 1.0, repositioning: 1.0, radius: 2.0, mount_speed: 1.0, mount_distance: 2 }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &spear_retreat::BEHAVIOUR, activity: "fight", priority: 5, params: &spear_retreat::Params { speed: 1.0, mount_speed: 1.0, mount_distance: 2 }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &melee_attack::BEHAVIOUR, activity: "fight", priority: 6, params: &melee_attack::Params { cooldown: 20, ..melee_attack::Params::DEFAULT }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &crossbow_attack::BEHAVIOUR, activity: "fight", priority: 7, params: &crossbow_attack::Params { charge: 25, delay: 20, jitter: 20, difficulty: 2 }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &remember_hoglin_killed::BEHAVIOUR, activity: "fight", priority: 8, params: &(), needs: &[(&ATTACK_TARGET, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &erase_memory_if::BEHAVIOUR, activity: "fight", priority: 9, params: &erase_memory_if::Params { when: is_near_zombified, forget: &ATTACK_TARGET }, needs: &[], duration: None, kids: &[] },
            // `PiglinAi.initCelebrateActivity`: the walk to the celebration spot, close at speed 1.0, then from 4 blocks at 0.6.
            BEntry { behaviour: &go_to_target_location::BEHAVIOUR, activity: "celebrate", priority: 0, params: &go_to_target_location::Params { memory: &CELEBRATE_LOCATION, distance: 2, speed: 1.0 }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &go_to_target_location::BEHAVIOUR, activity: "celebrate", priority: 1, params: &go_to_target_location::Params { memory: &CELEBRATE_LOCATION, distance: 4, speed: 0.6 }, needs: &[], duration: None, kids: &[] },
            // `PiglinAi.initAvoidActivity`: the walk away from the avoided entity (speed 1.0, 12 blocks), the idle look and move, the end.
            BEntry { behaviour: &avoid_walk::BEHAVIOUR, activity: "avoid", priority: 0, params: &avoid_walk::Params { speed: 1.0, desired: 12.0, memory: &AVOID_TARGET }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &run_one::QUIET, activity: "avoid", priority: 1, params: &(), needs: &[], duration: None, kids: LOOK_KIDS },
            BEntry { behaviour: &run_one::QUIET, activity: "avoid", priority: 2, params: &(), needs: &[], duration: None, kids: MOVE_KIDS },
            BEntry { behaviour: &erase_memory_if::BEHAVIOUR, activity: "avoid", priority: 3, params: &erase_memory_if::Params { when: wants_to_stop_fleeing, forget: &AVOID_TARGET }, needs: &[], duration: None, kids: &[] },
            // `PiglinAi.initRideHoglinActivity`: the mount of the ride target at 0.8, the look, the dismount when `wantsToStopRiding`.
            BEntry { behaviour: &mount::BEHAVIOUR, activity: "ride", priority: 0, params: &mount::Params { speed: 0.8 }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &run_one::QUIET, activity: "ride", priority: 1, params: &(), needs: &[], duration: None, kids: LOOK_KIDS },
            BEntry { behaviour: &dismount_or_skip_mounting::BEHAVIOUR, activity: "ride", priority: 2, params: &dismount_or_skip_mounting::Params { range: 8.0, stop: wants_to_stop_riding }, needs: &[], duration: None, kids: &[] },
            // `PiglinAi.initIdleActivity`: the start of a hunt on a hoglin, the baby's ride of a baby hoglin, the idle look and move,
            // and the look and interaction with a player within 4 blocks.
            BEntry { behaviour: &start_attacking::BEHAVIOUR, activity: "idle", priority: 0, params: &start_attacking::Params { finder: start_attacking::Finder::Piglin, ..start_attacking::Params::DEFAULT }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &start_hunting_hoglin::BEHAVIOUR, activity: "idle", priority: 1, params: &(), needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &run_one::QUIET, activity: "idle", priority: 3, params: &(), needs: &[], duration: None, kids: LOOK_KIDS },
            BEntry { behaviour: &run_one::QUIET, activity: "idle", priority: 4, params: &(), needs: &[], duration: None, kids: MOVE_KIDS },
            BEntry { behaviour: &look_and_interact::BEHAVIOUR, activity: "idle", priority: 5, params: &(), needs: &[], duration: None, kids: &[] },
        ],
    },
    sound_source: super::SoundCategory::Hostile,
    ..Def::EMPTY
};
