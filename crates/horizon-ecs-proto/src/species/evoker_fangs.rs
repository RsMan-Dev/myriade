//! `minecraft:evoker_fangs`: the fangs of an evoker's attack; their three deadlines are `fangs`'s.
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def { name: "minecraft:evoker_fangs", caps: Some(&[Cap::Transform]), ..Def::EMPTY };
