//! `Panda` (`mobs/panda.rs:1040-1085`): the goals that need no variant. Left out, awaiting the predicates `panda:acts`,
//! `panda:avoids`, `panda:aggressive`, `panda:unhappy_over` (to add by the integrator, `Builder::predicate`) and the parameters they
//! would select: the panic (`PandaPanic`), the breed (`PandaBreed`), the melee at 1.2, the look (`PandaLook`), the hurt-by alert of
//! the aggressive pandas, the avoidance of the player and of the monsters (the monsters are a category: `Avoid::Species` names ids).
use crate::behaviours::{float, follow_parent, panda_lie_on_back, panda_roll, panda_sit, panda_sneeze, random_look, stroll, tempt, Entry};
use crate::species::Def;

pub static DEF: Def = Def {
    name: "minecraft:panda",
    targets: &[],
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        // attend : panic `PandaPanic` (when panda:acts), breed `PandaBreed` (when panda:unhappy_over), melee 1.2 (when panda:acts)
        Entry::new(&tempt::MODULE, 4, &tempt::Params { speed: 1.0, items: &["minecraft:bamboo"], ..tempt::Params::DEFAULT }),
        // attend : avoid_entity Player (8.0, 2.0, 2.0) (when panda:avoids), avoid_entity monstres (4.0, 2.0, 2.0) (when panda:avoids)
        Entry::new(&panda_sit::MODULE, 7, &panda_sit::Params::DEFAULT),
        Entry::new(&panda_lie_on_back::MODULE, 8, &panda_lie_on_back::Params::DEFAULT),
        Entry::new(&panda_sneeze::MODULE, 8, &panda_sneeze::Params::DEFAULT),
        // attend : look_at_player `PandaLook` (when panda:acts)
        Entry::new(&random_look::MODULE, 10, &random_look::Params::DEFAULT),
        Entry::new(&panda_roll::MODULE, 12, &panda_roll::Params::DEFAULT),
        Entry::new(&follow_parent::MODULE, 13, &follow_parent::Params { speed: 1.25 }),
        Entry::new(&stroll::MODULE, 14, &stroll::Params::DEFAULT),
    ],
    ..Def::EMPTY
};
