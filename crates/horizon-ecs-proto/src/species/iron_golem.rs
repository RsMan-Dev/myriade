//! `IronGolem.registerGoals` as far as the frame has the modules: the blow, closing in on the target, going back to the village and
//! strolling in it (`village_walk`, on the registry of the points of interest), looking; the target of what hurt it. Missing: the offer of
//! the flower to a child villager (`OfferFlowerGoal`: the villager's state), `DefendVillageTargetGoal` (the reputation a villager keeps of a
//! player: its brain), the nearest hostile (`Enemy` but creepers), the anger, the walk toward a villager who wants a golem, the cracks.
use super::Def;
use crate::behaviours::{hurt_by, look_at_player, melee, move_towards_target, random_look, village_walk, Entry};

pub static DEF: Def = Def {
    name: "minecraft:iron_golem",
    targets: &[Entry::new(&hurt_by::MODULE, 2, &hurt_by::Params::DEFAULT)],
    goals: &[
        Entry::new(&melee::MODULE, 1, &melee::Params::DEFAULT),
        Entry::new(&move_towards_target::MODULE, 2, &move_towards_target::Params::DEFAULT),
        Entry::new(&village_walk::MODULE, 2, &village_walk::Params::BACK),
        Entry::new(&village_walk::MODULE, 4, &village_walk::Params::IN_VILLAGE),
        Entry::new(&look_at_player::MODULE, 7, &look_at_player::Params { range: 6.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 8, &random_look::Params::DEFAULT),
    ],
    ..Def::EMPTY
};
