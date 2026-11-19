//! `Drowned`: its swim: amphibious navigation, `DrownedMoveControl` (the marker `WantsToSwim` makes it swim, else the ground's control), the push 0.01 only under water when it wants to. Its goals are not configured yet (the water goals of the species are written on top of this configuration).
use super::Def;
use crate::behaviours::{attack_turtle_egg, drowned_go_to_beach, drowned_swim_up, look_at_player, random_look, stroll, try_find_water, Cap, Entry};
use crate::locomotion::{swim, Medium};

pub static DEF: Def = Def {
    name: "minecraft:drowned",
    goals: &[
        Entry::new(&try_find_water::MODULE, 1, &try_find_water::DROWNED),
        Entry::new(&attack_turtle_egg::MODULE, 4, &attack_turtle_egg::Params::DEFAULT),
        Entry::new(&drowned_go_to_beach::MODULE, 5, &drowned_go_to_beach::Params::DEFAULT),
        Entry::new(&drowned_swim_up::MODULE, 6, &drowned_swim_up::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 7, &stroll::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 8, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 8, &random_look::Params::DEFAULT),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Locomotion]),
    locomotion: &[(Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params { needs_water: false, amphibious: Some(false), control: swim::Control::Drowned, sinks: false, ..swim::Params::DEFAULT }))],
    spawn: Some(crate::spawning::SpawnDef { category: crate::spawning::Category::Monster, place: crate::spawning::Place::InWater, rule: crate::spawning::Rule::Drowned, cluster: 4, far: false, peaceful: false, liquid: true, despawn: crate::despawn::DespawnDef::MOB }), despawn: Some(crate::despawn::DespawnDef::MOB),
    ..Def::EMPTY
};
