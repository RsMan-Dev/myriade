//! `Enderman` (`mobs/enderman.rs:19-45`): its targets and goals. Left out: the enderman neighbour's target (`nearest_target`
//! option `reach`: not read by the framework). Not ported: the teleport (`randomTeleport`: service absent), the water damage and
//! the day departure that call it, the persistent anger, the water malus of the paths (`NavParams`).
use crate::behaviours::{ender_teleport, float, freeze_when_looked_at, hurt_by, leave_block, look_at_player, look_for_player, melee, random_look, stroll, take_block, Entry};
use crate::species::Def;

pub static DEF: Def = Def {
    name: "minecraft:enderman",
    targets: &[
        Entry::new(&look_for_player::MODULE, 1, &look_for_player::Params::DEFAULT),
        Entry::new(&hurt_by::MODULE, 2, &hurt_by::Params::DEFAULT),
        // attend : reach (nearest_target Species [endermite], must_see, `.reach()` : aucun champ dans nearest_target::Params)
    ],
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&freeze_when_looked_at::MODULE, 1, &freeze_when_looked_at::Params::DEFAULT),
        Entry::new(&melee::MODULE, 2, &melee::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 7, &stroll::Params { water_avoiding: 0.0, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 8, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 8, &random_look::Params::DEFAULT),
        Entry::new(&leave_block::MODULE, 10, &leave_block::Params::DEFAULT),
        Entry::new(&take_block::MODULE, 11, &take_block::Params::DEFAULT),
        Entry::new(&ender_teleport::MODULE, 255, &ender_teleport::Params::ENDERMAN),
    ],
    rules: &[crate::rules::RuleEntry::new(&crate::hostile::SUN_FLEE, &())],
    ..Def::EMPTY
};
