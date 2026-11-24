//! The slots of each species' synchronised data, by registry id: the configuration of `bridge/sync_b.rs`'s readers.
use crate::bridge::sync::SyncSlot;
use crate::bridge::sync_b::*;

/// Each species' slots, by registry id (a species not listed has none here).
pub static TABLE: &[(&str, &[SyncSlot])] = &[
    ("minecraft:camel", &[SyncSlot { index: 19, read: camel_dashing }, SyncSlot { index: 20, read: camel_pose_tick }]),
    ("minecraft:camel_husk", &[SyncSlot { index: 19, read: camel_dashing }, SyncSlot { index: 20, read: camel_pose_tick }]),
    (
        "minecraft:creeper",
        &[
            SyncSlot { index: 16, read: creeper_swell },
            SyncSlot { index: 17, read: creeper_charged },
            SyncSlot { index: 18, read: creeper_ignited },
        ],
    ),
    (
        "minecraft:pillager",
        &[SyncSlot { index: 16, read: raider_celebrating }, SyncSlot { index: 17, read: pillager_charging }],
    ),
    (
        "minecraft:panda",
        &[
            SyncSlot { index: 20, read: panda_eat },
            SyncSlot { index: 21, read: panda_gene },
            SyncSlot { index: 22, read: panda_hidden },
            SyncSlot { index: 23, read: panda_flags },
        ],
    ),
    ("minecraft:polar_bear", &[SyncSlot { index: 18, read: polar_bear_standing }]),
    ("minecraft:evoker", &[SyncSlot { index: 16, read: raider_celebrating }]),
    ("minecraft:illusioner", &[SyncSlot { index: 16, read: raider_celebrating }]),
    ("minecraft:vex", &[SyncSlot { index: 16, read: vex_charging }]),
    ("minecraft:ocelot", &[SyncSlot { index: 18, read: ocelot_trusting }]),
    (
        "minecraft:shulker",
        &[
            SyncSlot { index: 16, read: shulker_face },
            SyncSlot { index: 17, read: shulker_peek },
            SyncSlot { index: 18, read: shulker_color },
        ],
    ),
    ("minecraft:copper_golem", &[SyncSlot { index: 16, read: copper_weather }, SyncSlot { index: 17, read: copper_golem }]),
    ("minecraft:sniffer", &[SyncSlot { index: 18, read: sniffer_state }, SyncSlot { index: 19, read: sniffer_seed_at }]),
    ("minecraft:armadillo",&[SyncSlot { index: 18, read: armadillo }]),
    ("minecraft:zombie", &[SyncSlot { index: 18, read: converting }]),
    ("minecraft:husk", &[SyncSlot { index: 18, read: converting }]),
    ("minecraft:skeleton", &[SyncSlot { index: 16, read: converting }]),
    ("minecraft:blaze", &[SyncSlot { index: 16, read: blaze_charged }]),
    (
        "minecraft:turtle",
        &[SyncSlot { index: 18, read: turtle_egg }, SyncSlot { index: 19, read: turtle_laying }],
    ),
];
