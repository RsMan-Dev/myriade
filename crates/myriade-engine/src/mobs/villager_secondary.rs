//! Profession-keyed additions to the vanilla secondary POI sensor.
use super::*;


const TAGS: [&str; 15] = [
    "horizon:villager_secondary_poi/none",
    "horizon:villager_secondary_poi/armorer",
    "horizon:villager_secondary_poi/butcher",
    "horizon:villager_secondary_poi/cartographer",
    "horizon:villager_secondary_poi/cleric",
    "horizon:villager_secondary_poi/farmer",
    "horizon:villager_secondary_poi/fisherman",
    "horizon:villager_secondary_poi/fletcher",
    "horizon:villager_secondary_poi/leatherworker",
    "horizon:villager_secondary_poi/librarian",
    "horizon:villager_secondary_poi/mason",
    "horizon:villager_secondary_poi/nitwit",
    "horizon:villager_secondary_poi/shepherd",
    "horizon:villager_secondary_poi/toolsmith",
    "horizon:villager_secondary_poi/weaponsmith",
];

#[cfg(test)]
thread_local! { static SCAN_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

