//! `CamelHusk` (`equine.rs:204-205`, `camel_husk_tests.rs:10-12`): the camel without breeding; any rider holds its panic back.
// jar : canMate/canFallInLove false, so breed is removed below.
use super::{camel, Change, Def};
use crate::behaviours::{animal_panic, breed, tempt};

pub static DEF: Def = Def {
    name: "minecraft:camel_husk",
    reference: Some(&camel::DEF),
    // jar: only `CamelHusk.interact` calls `setPersistenceRequired` (the camel does not); a `reference`'s rules come first, these follow.
    rules: &[
        crate::rules::RuleEntry::new(&crate::hostile::NOT_ENEMY, &()),
        crate::rules::RuleEntry::new(&crate::keep_on_use::KEEP_ON_USE, &()),
        crate::rules::RuleEntry::new(&crate::mount_jump::JUMP_SOUND, &crate::mount_jump::JumpSound { sound: "entity.camel_husk.dash", volume: 1.0 }),
    ],
    changes: &[
        // jar: the tempting sensor asks `Animal.isFood`, and the husk's is the tag `#camel_husk_food` (rabbit foot), not the camel's cactus.
        Change::Set { module: &tempt::MODULE, nth: 0, params: &tempt::Params { speed: 2.5, items: &["minecraft:rabbit_foot"], ..tempt::Params::DEFAULT } },
        Change::Remove { module: &breed::MODULE, nth: 0 },
        Change::Set { module: &animal_panic::MODULE, nth: 0, params: &animal_panic::Params { speed: 4.0, riders: animal_panic::Riders::Any, stand_up: true, ..animal_panic::Params::DEFAULT } },
    ],
    ..Def::EMPTY
};
