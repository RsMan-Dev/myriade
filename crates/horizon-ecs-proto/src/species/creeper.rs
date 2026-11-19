//! `Creeper` (`mobs/hostile.rs:18-39`): its targets, then its goals. Left out: the two avoidances of the cat and of the ocelot
//! (`avoid_entity` names the species by identifier; those species are not in the registry). The rule `R:creeper` is not ported.
use crate::behaviours::{float, hurt_by, look_at_player, melee, nearest_target, random_look, stroll, swell, Entry};
use crate::species::Def;

pub static DEF: Def = Def {
    name: "minecraft:creeper",
    targets: &[Entry::new(&nearest_target::MODULE, 1, &nearest_target::Params::PLAYER), Entry::new(&hurt_by::MODULE, 2, &hurt_by::Params::DEFAULT)],
    goals: &[
        Entry::new(&float::MODULE, 1, &float::Params::DEFAULT),
        Entry::new(&swell::MODULE, 2, &swell::Params::DEFAULT),
        // attend : avoid_entity Species [ocelot] et [cat] (priorité 3, max 6, walk 1.0, sprint 1.2) : espèces absentes du registre
        Entry::new(&melee::MODULE, 4, &melee::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 5, &stroll::Params { speed: 0.8, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 6, &random_look::Params::DEFAULT),
    ],
    ..Def::EMPTY
};
