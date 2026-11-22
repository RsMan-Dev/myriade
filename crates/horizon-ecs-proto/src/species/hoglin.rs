//! `Hoglin` (`mobs/piglin.rs:1542-1569` `register_hoglin`, brain `piglin.rs:239-357`): the melee at 41 ticks (16 baby) with the
//! headbutt on it, the target of the adult creatures it is hurt by (alerting its kind), the player when adult and neither
//! pacified nor retreating. Its brain: the repellent blocks pacify it for 200 ticks; an adult piglin nearby makes it walk away
//! when idle; an `AVOID_TARGET` makes it retreat at 1.3 to 15 blocks. The zoglin conversion and its ambient sound are goals.
//! Left out (see `suivi/ecs-portage/reste/hoglin.md`): the retreat and fight sounds, the baby recoil, the walk value.
use super::Def;
use crate::behaviours::{ambient_sound, breed, convert_timer, float, retreat_on_hurt, follow_parent, headbutt, hurt_by, melee, nearest_target, Cap, Cond, Entry};
use crate::brain::behaviours::{do_nothing, erase_memory_if, look_at_player_sometimes, look_at_target_sink, move_to_target_sink, pacify, run_one, stop_attacking_if_target_invalid, stroll, walk_away, walk_to_look_target};
use crate::brain::sensors::{hoglin_sight, hurt_by as hurt_by_sensor, living, repellent_and_adult};
use crate::brain::*;
use bevy_ecs::prelude::*;

/// `horizon:is_adult` is in `predicates::BUILTIN`.
static ADULT: Cond = Cond::Is("horizon:is_adult");
static RETREATING: Cond = Cond::Is("minecraft:hoglin_retreating");
static PACIFIED: Cond = Cond::Is("minecraft:hoglin_pacified");
static NOT_RETREATING: Cond = Cond::Not(&RETREATING);
static BREEDING: Cond = Cond::All(&[Cond::Not(&RETREATING), Cond::Not(&PACIFIED)]);
static TARGETING: Cond = Cond::All(&[Cond::Is("horizon:is_adult"), Cond::Not(&PACIFIED), Cond::Not(&RETREATING)]);
/// `HoglinAi.createIdleMovementBehaviors` (idle and avoid): a stroll and a look walk at weight 2, nothing at weight 1.
static IDLE_KIDS: &[Kid] = &[
    Kid { behaviour: &stroll::BEHAVIOUR, weight: 2, params: &stroll::Params { speed: 0.4, h: 10, v: 7, dry_only: false }, needs: &[(&WALK_TARGET, Need::Absent)], duration: None },
    Kid { behaviour: &walk_to_look_target::BEHAVIOUR, weight: 2, params: &walk_to_look_target::Params { speed: 0.4, close: 3, needs_same_fluid: false, dry_speed: None }, needs: &[(&WALK_TARGET, Need::Absent), (&LOOK_TARGET, Need::Present)], duration: None },
    Kid { behaviour: &do_nothing::BEHAVIOUR, weight: 1, params: &(), needs: &[], duration: Some((30, 60)) },
];
/// `SetEntityLookTargetSometimes` (8 blocks, 30 to 60 ticks), idle and avoid.
const LOOK_SOMETIMES: look_at_player_sometimes::Params = look_at_player_sometimes::Params { range_sqr: 64.0, min: 30, spread: 31, players_only: false };

/// `isRetreating`: an `AVOID_TARGET` is held (the attacker, or the piglin the hoglin keeps away from).
pub fn retreating(w: &World, e: Entity) -> bool {
    present(w, e, &walk_away::AVOID_TARGET)
}

/// `isPacified`: the `PACIFIED` memory is held for its time to live.
pub fn pacified(w: &World, e: Entity) -> bool {
    present(w, e, &pacify::PACIFIED)
}

/// `HoglinAi.setAttackTargetIfCloserThanCurrent`: nothing for a pacified hoglin that receives the broadcast.
fn not_pacified(w: &World, e: Entity) -> bool {
    !pacified(w, e)
}

/// `HoglinAi.maybeRetaliate`: an attacker that is avoiding a repellent (its `Avoid` point, not yet expired) is not answered.
fn attacker_avoiding(w: &World, e: Entity) -> bool {
    w.get::<crate::behaviours::walk_away::Avoid>(e).is_some_and(|a| crate::cow::now_of(w, e) < a.until)
}

/// `HoglinAi.wantsToStopFleeing`: an adult that the piglins do not outnumber stops keeping away.
fn wants_to_stop_fleeing(w: &World, e: Entity) -> bool {
    ADULT.eval(w, e) && !hoglin_sight::piglins_outnumber(w, e)
}

/// `HoglinAi.isBreeding`: the breed target is held, and the attack target is dropped.
fn is_breeding(w: &World, e: Entity) -> bool {
    present(w, e, &BREED_TARGET)
}

pub static DEF: Def = Def {
    name: "minecraft:hoglin",
    gravity: true,
    random_max_health: false,
    sounds: None,
    sound_on_breed: false,
    reference: None,
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert: true, alert_types: &["minecraft:hoglin"], alert_when: Some(not_pacified), ignoring: &["minecraft:hoglin"], attacker_when: Some(attacker_avoiding), ..hurt_by::Params::DEFAULT }).when(&ADULT),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER).when(&TARGETING),
    ],
    goals: &[
        // `HoglinAi.wasHurtBy`, baby: it retreats from the attacker.
        Entry::new(&retreat_on_hurt::MODULE, 0, &()),
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        // `Hoglin.aiStep`: more than 300 ticks where piglins cannot live (the host's `UnsafePlace`) and it becomes a zoglin.
        Entry::new(&convert_timer::MODULE, 0, &convert_timer::Params { into: "minecraft:zoglin", after: 300 }),
        Entry::new(&melee::MODULE, 2, &melee::Params { interval: 41, interval_baby: Some(16), follow_unseen: true, forget_unreachable: Some(200), ..melee::Params::DEFAULT }).when(&NOT_RETREATING),
        Entry::new(&headbutt::MODULE, 2, &headbutt::Params { knockback: 0.6 }),
        Entry::new(&breed::MODULE, 3, &breed::Params { foods: &["minecraft:crimson_fungus"], ..breed::Params::DEFAULT }).when(&BREEDING),
        Entry::new(&follow_parent::MODULE, 4, &follow_parent::Params::DEFAULT).when(&NOT_RETREATING),
        Entry::new(&ambient_sound::MODULE, 7, &ambient_sound::Params { sounds: &["entity.hoglin.ambient"], min: 80, max: 400 }),
    ],
    changes: &[],
    // `Hoglin`: `shouldDropExperience` true and `xpReward` 5 as is, also for a baby (which leaves no loot: `LivingEntity.shouldDropLoot`).
    rules: &[
        crate::rules::RuleEntry::new(&crate::leash::LEASH, &crate::leash::LeashParams::DEFAULT),
        crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 5, max: 5 }),
        crate::rules::RuleEntry::new(&crate::loot::FLAT_XP, &()),
        crate::rules::RuleEntry::new(&crate::loot::BABY_DROPS, &crate::loot::BabyDrops::HOGLIN),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Brain]),
    brain: BrainDef {
        sensors: &[
            SensorEntry::new(&living::SENSOR, 20),
            SensorEntry::new(&hurt_by_sensor::SENSOR, 20),
            SensorEntry::new(&hoglin_sight::SENSOR, 20),
            SensorEntry::new(&repellent_and_adult::SENSOR, 20).with(&repellent_and_adult::Params {
                block_tag: "minecraft:hoglin_repellents",
                species: "minecraft:piglin",
                horizontal: 8,
                vertical: 4,
            }),
        ],
        activities: &[
            ActivityDef { name: "core", slot: 1, requires: &[] },
            ActivityDef { name: "fight", slot: 2, requires: &[(&ATTACK_TARGET, Need::Present)] },
            ActivityDef { name: "avoid", slot: 3, requires: &[(&walk_away::AVOID_TARGET, Need::Present)] },
            ActivityDef { name: "idle", slot: 5, requires: &[] },
        ],
        pick: &["fight", "avoid", "idle"],
        initial: &[],
        behaviours: &[
            // `HoglinAi.initCoreActivity`: `LookAtTargetSink` and `MoveToTargetSink` carry out the walks the activities ask for.
            BEntry { behaviour: &look_at_target_sink::BEHAVIOUR, activity: "core", priority: 0, params: &(), needs: &[(&LOOK_TARGET, Need::Present)], duration: Some((45, 90)), kids: &[] },
            BEntry { behaviour: &move_to_target_sink::BEHAVIOUR, activity: "core", priority: 1, params: &(), needs: &[(&PATH, Need::Absent), (&WALK_TARGET, Need::Present)], duration: None, kids: &[] },
            BEntry {
                behaviour: &pacify::BEHAVIOUR,
                activity: "fight",
                priority: 0,
                params: &pacify::Params { ttl: 200 },
                needs: &[(&repellent_and_adult::NEAREST_REPELLENT, Need::Present), (&pacify::PACIFIED, Need::Absent)],
                duration: None,
                kids: &[],
            },
            // `HoglinAi.initFightActivity`: `StopAttackingIfTargetInvalid.create()`, then `EraseMemoryIf(isBreeding, ATTACK_TARGET)`.
            BEntry { behaviour: &stop_attacking_if_target_invalid::BEHAVIOUR, activity: "fight", priority: 5, params: &stop_attacking_if_target_invalid::Params::DEFAULT, needs: &[(&ATTACK_TARGET, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &erase_memory_if::BEHAVIOUR, activity: "fight", priority: 6, params: &erase_memory_if::Params { when: is_breeding, forget: &ATTACK_TARGET }, needs: &[], duration: None, kids: &[] },
            BEntry {
                behaviour: &pacify::BEHAVIOUR,
                activity: "idle",
                priority: 0,
                params: &pacify::Params { ttl: 200 },
                needs: &[(&repellent_and_adult::NEAREST_REPELLENT, Need::Present), (&pacify::PACIFIED, Need::Absent)],
                duration: None,
                kids: &[],
            },
            BEntry {
                behaviour: &walk_away::BEHAVIOUR,
                activity: "avoid",
                priority: 0,
                params: &walk_away::Params { speed: 1.3, desired: 15.0, memory: &walk_away::AVOID_TARGET },
                needs: &[(&walk_away::AVOID_TARGET, Need::Present)],
                duration: None,
                kids: &[],
            },
            BEntry { behaviour: &run_one::QUIET, activity: "avoid", priority: 1, params: &(), needs: &[], duration: None, kids: IDLE_KIDS },
            BEntry { behaviour: &look_at_player_sometimes::BEHAVIOUR, activity: "avoid", priority: 2, params: &LOOK_SOMETIMES, needs: &[(&LOOK_TARGET, Need::Absent), (&VISIBLE_MOBS, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &erase_memory_if::BEHAVIOUR, activity: "avoid", priority: 3, params: &erase_memory_if::Params { when: wants_to_stop_fleeing, forget: &walk_away::AVOID_TARGET }, needs: &[], duration: None, kids: &[] },
            // `HoglinAi.initIdleActivity`: `SetWalkTargetAwayFrom.pos(NEAREST_REPELLENT, 1.0, 8, true)`.
            BEntry {
                behaviour: &walk_away::BEHAVIOUR,
                activity: "idle",
                priority: 0,
                params: &walk_away::Params { speed: 1.0, desired: 8.0, memory: &repellent_and_adult::NEAREST_REPELLENT },
                needs: &[(&repellent_and_adult::NEAREST_REPELLENT, Need::Present)],
                duration: None,
                kids: &[],
            },
            BEntry {
                behaviour: &walk_away::BEHAVIOUR,
                activity: "idle",
                priority: 0,
                params: &walk_away::Params { speed: 0.4, desired: 8.0, memory: &repellent_and_adult::NEAREST_VISIBLE_ADULT },
                needs: &[(&repellent_and_adult::NEAREST_VISIBLE_ADULT, Need::Present)],
                duration: None,
                kids: &[],
            },
            BEntry { behaviour: &look_at_player_sometimes::BEHAVIOUR, activity: "idle", priority: 2, params: &LOOK_SOMETIMES, needs: &[(&LOOK_TARGET, Need::Absent), (&VISIBLE_MOBS, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &run_one::QUIET, activity: "idle", priority: 6, params: &(), needs: &[], duration: None, kids: IDLE_KIDS },
        ],
    },
    locomotion: &[],
    spawn: None,
    despawn: None,
    saved: true,
    screaming: None,
    lost_horn: None,
    llama_strength: false,
    not_in_peaceful: true,
    variant: None,
    draws: None,
    merchant: false,
    no_portal: false,
};
