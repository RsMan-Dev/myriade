//! `Armadillo` goals (`mobs/armadillo.rs:56-78`) that need no variant. Left out, awaiting a `when` predicate or a parameter:
//! the panic (only the environmental causes), the breed (only when not afraid) and the tempt (its own variant).
use crate::behaviours::{baby_follow_adult, float, look_at_player, random_look, rolled, stroll, Entry};
use crate::species::Def;

pub static DEF: Def = Def {
    name: "minecraft:armadillo",
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        // attend : panic (causes d'environnement seulement : `environmental`), breed (`when` : pas effrayé), tempt (variante ArmadilloTempt)
        Entry::new(&rolled::MODULE, 2, &rolled::Params::DEFAULT),
        Entry::new(&baby_follow_adult::MODULE, 4, &baby_follow_adult::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 5, &stroll::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params { range: 6.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 7, &random_look::Params::DEFAULT),
    ],
    ..Def::EMPTY
};
