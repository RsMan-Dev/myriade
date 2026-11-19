//! `Goat` (`mobs/goat.rs:60-65`, `GoatAi`): the farm animal base (panic, breed, tempt with wheat, follow the parent, stroll, look), the ram (2) and
//! the long jump (2: 600 to 1200 ticks between two, a box of 5 x 5, any landing spot, only while the ram cools down). Left out: the screaming
//! goat's horns, the lowered-head pose of the ram, the effects of the jump (`jump_boost` 0).
use super::{templates::farm_animal, Change, Def, Section};
use crate::behaviours::{long_jump, ram, Entry};

pub static DEF: Def = Def {
    name: "minecraft:goat",
    reference: Some(&farm_animal::DEF),
    changes: &[
        Change::Add(Section::Goals, Entry::new(&ram::MODULE, 2, &ram::Params::DEFAULT)),
        Change::Add(Section::Goals, Entry::new(&long_jump::MODULE, 2, &long_jump::Params { cooldown: (600, 1200), width: 5, height: 5, preferred: None, swim_rules: false, gated_by_ram: true, ..long_jump::Params::DEFAULT })),
    ],
    ..Def::EMPTY
};
