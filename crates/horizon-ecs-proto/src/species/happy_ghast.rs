//! `HappyGhast` (`mobs/happy_ghast.rs`): a free flier (`fly` Float, free, as the ghast) that floats around its home (`float_around`,
//! 16 blocks, priority 5) and is a rideable mount. Goals: `still_under_riders` (1, holds still while a player is on it, 10 ticks after);
//! `square_or_face` (0, squares to the world while stilled, else faces its motion); `heal_weather` (3, 1 health every 600 ticks dry,
//! every 20 wet); `harness_tempt` (4, tempt with the harness items: list not verifiable offline, empty for now); `float_around` (5).
//! The rule `harness_worn` takes a harness item. Riding: `fly_steer::drive` steers the first rider (socle: skip `ridden_drive` there).
//! Priorities are NOT verified against vanilla: no decompiled source is available. The baby is `BABY` below (hovers, no goals).
use super::Def;
use crate::behaviours::{float_around, fly_steer, harness_worn, heal_weather, square_or_face, still_under_riders, tempt, Cap, Entry};
use crate::locomotion::{fly, Medium};
use crate::rules::RuleEntry;

/// The harness items that tempt (the `happy_ghast` tempt tag, not resolvable offline): empty until the tag is read.
const HARNESS_TEMPT: &[&str] = &[];

pub static DEF: Def = Def {
    name: "minecraft:happy_ghast",
    goals: &[
        Entry::new(&still_under_riders::MODULE, 1, &still_under_riders::Params { reach: 2.0, after: 10 }),
        Entry::new(&square_or_face::MODULE, 0, &square_or_face::Params::DEFAULT),
        Entry::new(&heal_weather::MODULE, 3, &heal_weather::Params { dry: 600, wet: 20, amount: 1.0, max: 20.0 }),
        Entry::new(&tempt::MODULE, 4, &tempt::Params { speed: 1.2, items: HARNESS_TEMPT, stop: 2.5, range: 10.0, look_y: 95.0, calm: 100 }),
        Entry::new(&float_around::MODULE, 5, &float_around::Params { blocks: 16 }),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Locomotion, Cap::Rideable]),
    rules: &[RuleEntry::new(&harness_worn::RULE, &())],
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { control: fly::Control::Float, free: true, ..fly::Params::DEFAULT }))],
    ..Def::EMPTY
};

/// The baby (`GrowAt`): hovers in place (`FlyingMoveControl` with `hovers`), no goals, the same rule for its harness check.
pub static BABY: Def = Def {
    name: "minecraft:happy_ghast",
    goals: &[],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Locomotion]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { control: fly::Control::Float, free: true, hovers: true, ..fly::Params::DEFAULT }))],
    ..Def::EMPTY
};
