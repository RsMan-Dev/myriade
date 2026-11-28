//! `Cod`: a fish of the base, in a school (`AbstractSchoolingFish`): it follows its leader at priority 5 (`FollowFlockLeader`).
use super::{templates::fish, Def};
use crate::locomotion::{swim, walk, Medium};
use crate::behaviours::{ambient_sound, follow_flock_leader, Entry};

pub static DEF: Def = Def {
    name: "minecraft:cod",
    reference: Some(&fish::DEF),
    locomotion: &[
        (Medium::Ground, Entry::new(&walk::MODULE, 0, &walk::Params::DEFAULT)),
        (Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params { flop: Some("entity.cod.flop"), ..swim::Params::DEFAULT })),
    ],
    // `WaterAnimal.getAmbientSoundInterval`: 120 ticks, the `Mob.baseTick` law; `Cod.getAmbientSound` names the one sound (no draw).
    goals: &[
        Entry::new(&follow_flock_leader::MODULE, 5, &follow_flock_leader::Params::DEFAULT),
        Entry::new(&ambient_sound::MODULE, 9, &ambient_sound::Params { interval: Some(120), choose: Some(ambient), ..ambient_sound::Params::DEFAULT }),
    ],
    rules: &[crate::rules::RuleEntry::new(&crate::behaviours::held_swap::HELD_SWAP, &crate::behaviours::held_swap::SwapParams { from: "minecraft:water_bucket", to: "minecraft:cod_bucket", adult_only: false, takes_creature: true, sound: "minecraft:item.bucket.fill_fish", alt: None, extra: crate::behaviours::held_swap::no_data })],
    ..Def::EMPTY
};

/// `Cod.getAmbientSound`.
fn ambient(_: &mut bevy_ecs::prelude::World, _: bevy_ecs::prelude::Entity) -> &'static str {
    "entity.cod.ambient"
}
