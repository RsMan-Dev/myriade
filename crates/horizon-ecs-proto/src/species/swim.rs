//! The water sounds of the species (`Entity.getSwimSound` and its overrides, `mobs/ai.rs` `swim_sound`): the names only, the behaviour is
//! `behaviours/swim_sound.rs`. A species without an entry has `entity.generic.swim`; a monster (`Monster.getSwimSound`) has the hostile one.
use crate::behaviours::swim_sound::{Sounds, SwimSounds};

/// The `SwimSounds` a species of this name is created with, `None` for the generic sound.
pub fn swim_sounds(name: &str, monster: bool) -> Option<SwimSounds> {
    let one = |n: &'static str| SwimSounds { sounds: Sounds::one(n), steps: 0, baby: None };
    Some(match name {
        "minecraft:skeleton_horse" => SwimSounds {
            sounds: Sounds { step_water: "entity.skeleton_horse.step_water", gallop_water: "entity.skeleton_horse.gallop_water", swim: "entity.skeleton_horse.swim" },
            steps: 0,
            baby: None,
        },
        "minecraft:drowned" => one("entity.drowned.swim"),
        "minecraft:axolotl" => one("entity.axolotl.swim"),
        "minecraft:dolphin" => one("entity.dolphin.swim"),
        "minecraft:cod" | "minecraft:salmon" | "minecraft:tropical_fish" | "minecraft:pufferfish" | "minecraft:tadpole" => one("entity.fish.swim"),
        "minecraft:nautilus" => SwimSounds { baby: Some("entity.baby_nautilus.swim"), ..one("entity.nautilus.swim") },
        "minecraft:zombie_nautilus" => one("entity.zombie_nautilus.swim"),
        "minecraft:turtle" => one("entity.turtle.swim"),
        "minecraft:hoglin" => one("entity.hostile.swim"),
        _ if monster => one("entity.hostile.swim"),
        _ => return None,
    })
}
