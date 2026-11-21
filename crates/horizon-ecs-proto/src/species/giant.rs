//! `Giant` (`Family::Inert`: no goals, no targets): a zombie-sized body that stands where it is put. Walking mode and gravity only.
use super::Def;

pub static DEF: Def = Def { name: "minecraft:giant", not_in_peaceful: true, ..Def::EMPTY };
