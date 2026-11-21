//! `Bat`: hangs asleep (zero cost) until a player comes within 4 blocks or it is hurt, then flutters (`Control::None`: its goal steers it;
//! travel in the air with its gravity, the vertical speed damped to 0.6).
use super::Def;
use crate::behaviours::{bat_flutter, Cap, Entry};
use crate::locomotion::{fly, Medium};

pub static DEF: Def = Def {
    name: "minecraft:bat",
    goals: &[
        Entry::new(&bat_flutter::MODULE, 0, &bat_flutter::Params::DEFAULT),
        Entry::new(&crate::behaviours::move_emission::MODULE, 0, &crate::behaviours::move_emission::Params { flap: crate::behaviours::move_emission::Flap::Every { period: 10, flying_only: false, offset: false }, ..crate::behaviours::move_emission::Params::DEFAULT }),
    ],
    rules: &[crate::rules::RuleEntry::new(&bat_flutter::NO_FALL, &())],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Locomotion, Cap::IgnoresTriggers]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { control: fly::Control::None, free: false, damp_y: 0.6, ..fly::Params::DEFAULT }))],
    spawn: Some(crate::spawning::SpawnDef { category: crate::spawning::Category::Ambient, rule: crate::spawning::Rule::Cave { on: "minecraft:bats_spawnable_on" }, far: false, peaceful: true, ..crate::spawning::SpawnDef::MONSTER }), despawn: Some(crate::despawn::DespawnDef::MOB),
    ..Def::EMPTY
};
