//! The slots of each species' synchronised data, by registry id: the configuration of `bridge/sync_a.rs`'s readers.
use crate::bridge::sync::SyncSlot;
use crate::bridge::sync_a::*;

/// Sans équivalent (pas de composant dans P) : guardian, bat, piglin, parrot and cat variants, wolf (collar, variant,
/// anger), horse family, happy_ghast, villager and zombie_villager data, creeper, sulfur_cube fuse and from_bucket.
pub static TABLE: &[(&str, &[SyncSlot])] = &[
    ("minecraft:piglin", &[SyncSlot { index: 16, read: zomb_immune }]),
    ("minecraft:piglin_brute", &[SyncSlot { index: 16, read: zomb_immune }]),
    ("minecraft:hoglin", &[SyncSlot { index: 18, read: zomb_immune }]),
    ("minecraft:allay", &[SyncSlot { index: 16, read: dancing }]),
    ("minecraft:sheep", &[SyncSlot { index: 18, read: wool }]),
    ("minecraft:creaking", &[SyncSlot { index: 16, read: can_move }, SyncSlot { index: 17, read: is_active }, SyncSlot { index: 18, read: tearing_down }]),
    ("minecraft:snow_golem", &[SyncSlot { index: 16, read: pumpkin }]),
    ("minecraft:bogged", &[SyncSlot { index: 16, read: sheared }]),
    ("minecraft:goat", &[SyncSlot { index: 18, read: screaming_goat }, SyncSlot { index: 19, read: left_horn }, SyncSlot { index: 20, read: right_horn }]),
    ("minecraft:phantom", &[SyncSlot { index: 16, read: phantom_size }]),
    ("minecraft:camel", &[SyncSlot { index: 20, read: last_pose }]),
    ("minecraft:slime", &[SyncSlot { index: 18, read: size }]),
    ("minecraft:magma_cube", &[SyncSlot { index: 18, read: size }]),
    ("minecraft:sulfur_cube", &[SyncSlot { index: 18, read: size }]),
    ("minecraft:fox", &[SyncSlot { index: 18, read: variant }]),
    ("minecraft:mooshroom", &[SyncSlot { index: 18, read: variant }]),
    ("minecraft:rabbit", &[SyncSlot { index: 18, read: variant }]),
    ("minecraft:zombie_villager", &[SyncSlot { index: 19, read: converting }]),
    ("minecraft:villager", &[SyncSlot { index: 18, read: unhappy }, SyncSlot { index: 19, read: villager_data }, SyncSlot { index: 20, read: villager_finalized }]),
    ("minecraft:wandering_trader", &[SyncSlot { index: 18, read: unhappy }]),
    (
        "minecraft:wolf",
        &[SyncSlot { index: 18, read: tame_flags }, SyncSlot { index: 19, read: tame_owner }, SyncSlot { index: 20, read: interested }, SyncSlot { index: 21, read: collar }],
    ),
    ("minecraft:cat", &[SyncSlot { index: 18, read: tame_flags }, SyncSlot { index: 19, read: tame_owner }, SyncSlot { index: 23, read: collar }]),
    ("minecraft:parrot", &[SyncSlot { index: 18, read: tame_flags }, SyncSlot { index: 19, read: tame_owner }, SyncSlot { index: 20, read: variant }]),
    ("minecraft:llama", &[SyncSlot { index: 21, read: variant }]),
    ("minecraft:salmon", &[SyncSlot { index: 17, read: salmon_type }]),
];
