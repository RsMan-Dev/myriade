//! `HappyGhast` (`mobs/happy_ghast.rs`): a free flier (`fly` Float, free, as the ghast) that floats around its home (`float_around`,
//! 16 blocks, priority 5) and is a rideable mount. Goals: `still_under_riders` (1, holds still while a player is on it, 10 ticks after);
//! `square_or_face` (0, squares to the world while stilled, else faces its motion); `heal_weather` (3, 1 health every 600 ticks dry,
//! every 20 wet); `float` (3, waits for the still timeout); `animal_panic` (1, the adult, speed 2.0); `tempt` (4, drifts to the player with
//! the food and the harness items); `float_around` (5).
//! The jar's `HappyGhastAi` gives the priorities of `float`, `animal_panic` and `tempt`; the others are NOT verified against vanilla.
//! The rule `harness_worn` takes a harness item. Riding: `RIDE` with `RideParams::GHAST`; `mount::ridden_drive` steers the first rider (`Steer::Fly`, harness, no still timeout).
//! The baby is `BABY` below (hovers, no goals).
use super::Def;
use crate::behaviours::{animal_panic, float, float_around, harness_worn, heal_weather, square_or_face, still_under_riders, tempt, Cap, Entry};
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

pub static DEF: Def = Def {
    name: "minecraft:happy_ghast",
    goals: &[
        Entry::new(&still_under_riders::MODULE, 1, &still_under_riders::Params { reach: 2.0, after: 10 }),
        Entry::new(&square_or_face::MODULE, 0, &square_or_face::Params::DEFAULT),
        Entry::new(&heal_weather::MODULE, 3, &heal_weather::Params { dry: 600, wet: 20, amount: 1.0, max: 20.0 }),
        Entry::new(&float::MODULE, 3, &float::Params { unless_still: true }),
        Entry::new(&animal_panic::MODULE, 1, &animal_panic::Params { speed: 2.0, ..animal_panic::Params::DEFAULT }),
        Entry::new(&tempt::MODULE, 4, &tempt::Params { speed: 1.25, items: HARNESS_TEMPT, stop: 7.0, range: 16.0, look_y: 95.0, calm: 100, drift: true }),
        Entry::new(&float_around::MODULE, 5, &float_around::Params { blocks: 16 }),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Locomotion, Cap::Rideable, Cap::CollidableWhenStill]),
    rules: &[RuleEntry::new(&harness_worn::RULE, &()), RuleEntry::new(&crate::hand_feed::FEED_BABY, &crate::hand_feed::FeedParams::HAPPY_GHAST),RuleEntry::new(&RIDE, &RideParams::GHAST)],
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { control: fly::Control::Float, free: true, ..fly::Params::DEFAULT }))],
    ..Def::EMPTY
};

/// The baby (`GrowAt`): hovers in place (`FlyingMoveControl` with `hovers`), no goals, the same rule for its harness check.
pub static BABY: Def = Def {
    name: "minecraft:happy_ghast",
    goals: &[],
    // `Animal.mobInteract`: the food ages the baby up (the adult cannot fall in love).
    rules: &[RuleEntry::new(&crate::hand_feed::FEED_BABY, &crate::hand_feed::FeedParams::HAPPY_GHAST)],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Locomotion, Cap::CollidableWhenStill]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { control: fly::Control::Float, free: true, hovers: true, ..fly::Params::DEFAULT }))],
    ..Def::EMPTY
};
