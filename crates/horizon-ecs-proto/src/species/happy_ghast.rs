//! `HappyGhast` (`mobs/happy_ghast.rs`): a free flier (`fly` Float, free, as the ghast) that floats around its home (`float_around`,
//! 16 blocks, priority 5) and is a rideable mount. Goals: `still_under_riders` (1, holds still while a player is on it, 10 ticks after);
//! `square_or_face` (0, squares to the world while stilled, else faces its motion); `heal_weather` (3, 1 health every 600 ticks dry,
//! every 20 wet); `float` (3, waits for the still timeout); `float_around` (5).
//! Brain (`HappyGhastAi`, javap): core (swim at 0.8, the look and move sinks, the temptation cooldown); panic (`AnimalPanic` 2.0, while
//! `IS_PANICKING`); idle (`FollowTemptation` 1.25, 3 blocks, looking in the eyes; `RunOne` of the fly stroll 1.0 and the look walk 1.0,
//! 3 blocks). The baby (`BABY`) follows the nearest player, then the nearest adult, from 3 to 16 blocks at 1.1.
//! Not verified against vanilla: the weights of the idle `RunOne`s, the priorities of the goals.
//! The rule `harness_worn` takes a harness item. Riding: `RIDE` with `RideParams::GHAST`; `mount::ridden_drive` steers the first rider (`Steer::Fly`, harness, no still timeout).
use super::Def;
use crate::behaviours::{float, float_around, harness_worn, heal_weather, square_or_face, still_under_riders, Cap, Entry};
use crate::brain::behaviours::{count_down_cooldown_ticks, fly_stroll, follow_adult, follow_temptation, look_at_target_sink, move_to_target_sink, panic, run_one, walk_to_look_target, work_swim};
use crate::brain::sensors::{adult, hurt_by, living, nearest_players, tempting};
use crate::brain::*;
use crate::locomotion::{fly, Medium};
use crate::mount::{RideParams, RIDE};
use crate::rules::RuleEntry;

/// The items that tempt: the food (the Mob's snowball) and the sixteen harnesses, by vanilla colour.
const HARNESS_TEMPT: &[&str] = &[
    "minecraft:snowball",
    "minecraft:white_harness",
    "minecraft:orange_harness",
    "minecraft:magenta_harness",
    "minecraft:light_blue_harness",
    "minecraft:yellow_harness",
    "minecraft:lime_harness",
    "minecraft:pink_harness",
    "minecraft:gray_harness",
    "minecraft:light_gray_harness",
    "minecraft:cyan_harness",
    "minecraft:purple_harness",
    "minecraft:blue_harness",
    "minecraft:brown_harness",
    "minecraft:green_harness",
    "minecraft:red_harness",
    "minecraft:black_harness",
];

/// The idle walks of `RunOne`: a fly stroll (1.0) or a walk to the look target (1.0, 3 blocks).
static IDLE_KIDS: &[Kid] = &[
    Kid { behaviour: &fly_stroll::BEHAVIOUR, weight: 1, params: &fly_stroll::Params { speed: 1.0 }, needs: &[(&WALK_TARGET, Need::Absent)], duration: None },
    Kid { behaviour: &walk_to_look_target::BEHAVIOUR, weight: 1, params: &walk_to_look_target::Params { speed: 1.0, close: 3, needs_same_fluid: false, dry_speed: None }, needs: &[(&WALK_TARGET, Need::Absent), (&LOOK_TARGET, Need::Present)], duration: None },
];

pub static DEF: Def = Def {
    name: "minecraft:happy_ghast",
    goals: &[
        Entry::new(&still_under_riders::MODULE, 1, &still_under_riders::Params { reach: 2.0, after: 10 }),
        Entry::new(&square_or_face::MODULE, 0, &square_or_face::Params::DEFAULT),
        Entry::new(&heal_weather::MODULE, 3, &heal_weather::Params { dry: 600, wet: 20, amount: 1.0, max: 20.0 }),
        Entry::new(&float::MODULE, 3, &float::Params { unless_still: true, ..float::Params::DEFAULT }),
        Entry::new(&float_around::MODULE, 5, &float_around::Params { blocks: 16 }),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Rideable, Cap::CollidableWhenStill, Cap::Brain]),
    rules: &[RuleEntry::new(&harness_worn::RULE, &()), RuleEntry::new(&crate::hand_feed::FEED_BABY, &crate::hand_feed::FeedParams::HAPPY_GHAST), RuleEntry::new(&RIDE, &RideParams::GHAST), RuleEntry::new(&crate::leash::LEASH, &crate::leash::LeashParams::GHAST)],
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { control: fly::Control::Float, free: true, ..fly::Params::DEFAULT }))],
    brain: BrainDef {
        sensors: &[
            SensorEntry::new(&living::SENSOR, 20),
            SensorEntry::new(&hurt_by::SENSOR, 20),
            SensorEntry::new(&tempting::SENSOR, 10).with(&tempting::Params { range: 16.0, foods: HARNESS_TEMPT }),
        ],
        activities: &[
            ActivityDef { name: "core", slot: 1, requires: &[] },
            ActivityDef { name: "panic", slot: 2, requires: &[(&IS_PANICKING, Need::Present)] },
            ActivityDef { name: "idle", slot: 5, requires: &[] },
        ],
        pick: &["panic", "idle"],
        initial: &[],
        behaviours: &[
            // The core: the swim, the look and move sinks, the temptation cooldown counted down.
            BEntry { behaviour: &work_swim::BEHAVIOUR, activity: "core", priority: 0, params: &(), needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &look_at_target_sink::BEHAVIOUR, activity: "core", priority: 1, params: &(), needs: &[(&LOOK_TARGET, Need::Present)], duration: Some((45, 90)), kids: &[] },
            BEntry { behaviour: &move_to_target_sink::BEHAVIOUR, activity: "core", priority: 2, params: &(), needs: &[(&PATH, Need::Absent), (&WALK_TARGET, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &count_down_cooldown_ticks::BEHAVIOUR, activity: "core", priority: 3, params: &count_down_cooldown_ticks::Params { key: &TEMPTATION_COOLDOWN_TICKS }, needs: &[], duration: None, kids: &[] },
            // `AnimalPanic` at 2.0: hurt, or panicking until the walk is over.
            BEntry { behaviour: &panic::BEHAVIOUR, activity: "panic", priority: 0, params: &panic::Params { speed: 2.0, threat: None }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &follow_temptation::BEHAVIOUR, activity: "idle", priority: 0, params: &follow_temptation::Params { speed: 1.25, close: 3.0, look_in_the_eyes: true, dry_speed: None }, needs: &[(&TEMPTING_PLAYER, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &run_one::QUIET, activity: "idle", priority: 1, params: &(), needs: &[], duration: None, kids: IDLE_KIDS },
        ],
    },
    ..Def::EMPTY
};

/// The baby (`GrowAt`): hovers in place (`FlyingMoveControl` with `hovers`), follows the nearest player, else the nearest adult, from 3
/// to 16 blocks at 1.1 (`BabyFollowAdult`). The same rule for its harness check.
pub static BABY: Def = Def {
    name: "minecraft:happy_ghast",
    // `Animal.mobInteract`: the food ages the baby up (the adult cannot fall in love).
    rules: &[RuleEntry::new(&crate::hand_feed::FEED_BABY, &crate::hand_feed::FeedParams::HAPPY_GHAST)],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::CollidableWhenStill, Cap::Brain]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { control: fly::Control::Float, free: true, hovers: true, ..fly::Params::DEFAULT }))],
    brain: BrainDef {
        sensors: &[
            SensorEntry::new(&living::SENSOR, 20),
            SensorEntry::new(&hurt_by::SENSOR, 20),
            SensorEntry::new(&nearest_players::SENSOR, 20),
            SensorEntry::new(&adult::SAME, 20),
        ],
        activities: &[
            ActivityDef { name: "core", slot: 1, requires: &[] },
            ActivityDef { name: "panic", slot: 2, requires: &[(&IS_PANICKING, Need::Present)] },
            ActivityDef { name: "idle", slot: 5, requires: &[] },
        ],
        pick: &["panic", "idle"],
        initial: &[],
        behaviours: &[
            BEntry { behaviour: &work_swim::BEHAVIOUR, activity: "core", priority: 0, params: &(), needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &look_at_target_sink::BEHAVIOUR, activity: "core", priority: 1, params: &(), needs: &[(&LOOK_TARGET, Need::Present)], duration: Some((45, 90)), kids: &[] },
            BEntry { behaviour: &move_to_target_sink::BEHAVIOUR, activity: "core", priority: 2, params: &(), needs: &[(&PATH, Need::Absent), (&WALK_TARGET, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &panic::BEHAVIOUR, activity: "panic", priority: 0, params: &panic::Params { speed: 2.0, threat: None }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &follow_adult::BEHAVIOUR, activity: "idle", priority: 0, params: &follow_adult::Params { speed: 1.1, min: 3.0, max: 16.0, close: 2, eyes: true, player: true, dry_speed: None }, needs: &[(&NEAREST_VISIBLE_PLAYER, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &follow_adult::BEHAVIOUR, activity: "idle", priority: 1, params: &follow_adult::Params { speed: 1.1, min: 3.0, max: 16.0, close: 2, eyes: true, player: false, dry_speed: None }, needs: &[(&NEAREST_VISIBLE_ADULT, Need::Present)], duration: None, kids: &[] },
        ],
    },
    ..Def::EMPTY
};
