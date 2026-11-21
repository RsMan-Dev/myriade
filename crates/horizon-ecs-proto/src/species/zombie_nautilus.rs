//! `ZombieNautilus` (`mobs/nautilus.rs:55-61`, `ZombieNautilusAi`): the nautilus's swim, no panic and no love; the tempt at 0.9 (1), the
//! charge at 0.5 (2), the swimming stroll every tick (3). Its variant (temperate or warm) and its drying out are not goals.
use super::{nautilus, Def};
use crate::behaviours::{charge_attack, fish_swim, tempt, Cap, Entry};

pub static DEF: Def = Def {
    name: "minecraft:zombie_nautilus",
    goals: &[
        Entry::new(&tempt::MODULE, 1, &tempt::Params { speed: 0.9, items: nautilus::FOOD, stop: 3.5, ..tempt::Params::DEFAULT }),
        Entry::new(&charge_attack::MODULE, 2, &charge_attack::Params { speed: 0.5, ..charge_attack::Params::DEFAULT }),
        Entry::new(&fish_swim::MODULE, 3, &fish_swim::Params { speed: 1.0, interval: 1 }),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::PoisonImmune]),
    locomotion: nautilus::LOCOMOTION,
    ..Def::EMPTY
};
