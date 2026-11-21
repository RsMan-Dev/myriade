//! `Panda` (`mobs/panda.rs:1040-1085`): the goals that need no variant. Left out, awaiting the parameters they would select: the
//! sulking of the breed (`PandaBreed`'s `unhappy_cooldown`, `panda.rs:1046-1051`: DOUTE, the breed runs without it), the look's
//! `lookAt` set by the breed (`PandaLook`, `panda.rs:930-990`: the sulking target is not ported), the melee at 1.2 (ACTS), the
//! hurt-by alert of the aggressive pandas, the avoidance of the player and of the monsters (the monsters are a category: `Avoid::Species`
//! names ids).
use crate::behaviours::{float, hurt_by, follow_parent, look_at_player, breed, melee, panda_lie_on_back, panic, panda_roll, panda_sit, panda_sneeze, random_look, stroll, tempt, Cond, Entry};
use crate::behaviours::avoid_entity::{self, Avoid};
use crate::species::Def;

static ACTS: Cond = Cond::Is("panda:acts");
static AVOIDS: Cond = Cond::Is("panda:avoids");
static FLEE_PLAYER: avoid_entity::Params = avoid_entity::Params { what: Avoid::Player, max_dist: 8.0, walk: 2.0, sprint: 2.0, ..avoid_entity::Params::DEFAULT };
static FLEE_MONSTERS: avoid_entity::Params = avoid_entity::Params { what: Avoid::Monsters, max_dist: 4.0, walk: 2.0, sprint: 2.0, ..avoid_entity::Params::DEFAULT };

pub static DEF: Def = Def {
    name: "minecraft:panda",
    // `PandaHurtByTargetGoal`: the call goes to the aggressive pandas only (`alert_if`).
    targets: &[Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert: true, alert_when: Some(crate::behaviours::panda_state::is_aggressive), ..hurt_by::Params::DEFAULT })],
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&panic::MODULE, 2, &panic::Params { environmental: true, ..panic::Params::DEFAULT }),
        // `PandaBreed` (`panda.rs:1046`): `Breed::new(1.0)`, the bamboo as food (`panda.rs:1036`, the tempt's item).
        Entry::new(&breed::MODULE, 2, &breed::Params { speed: 1.0, foods: &["minecraft:bamboo"], ..breed::Params::DEFAULT }),
        Entry::new(&melee::MODULE, 3, &melee::Params { speed: 1.2_f32 as f64, follow_unseen: true, ..melee::Params::DEFAULT }).when(&ACTS),
        Entry::new(&tempt::MODULE, 4, &tempt::Params { speed: 1.0, items: &["minecraft:bamboo"], ..tempt::Params::DEFAULT }),
        Entry::new(&avoid_entity::MODULE, 6, &FLEE_PLAYER).when(&AVOIDS),
        Entry::new(&avoid_entity::MODULE, 6, &FLEE_MONSTERS).when(&AVOIDS),
        Entry::new(&panda_sit::MODULE, 7, &panda_sit::Params::DEFAULT),
        Entry::new(&panda_lie_on_back::MODULE, 8, &panda_lie_on_back::Params::DEFAULT),
        Entry::new(&panda_sneeze::MODULE, 8, &panda_sneeze::Params::DEFAULT),
        // `PandaLookAtPlayerGoal` (`panda.rs:1076`, range 6, one chance in 50 per ask, `acts`: `PandaLook::can_use`, `panda.rs:952-960`).
        Entry::new(&look_at_player::MODULE, 9, &look_at_player::Params::DEFAULT).when(&ACTS),
        Entry::new(&random_look::MODULE, 10, &random_look::Params::DEFAULT),
        Entry::new(&panda_roll::MODULE, 12, &panda_roll::Params::DEFAULT),
        Entry::new(&follow_parent::MODULE, 13, &follow_parent::Params { speed: 1.25 }),
        Entry::new(&stroll::MODULE, 14, &stroll::Params::DEFAULT),
    ],
    ..Def::EMPTY
};
