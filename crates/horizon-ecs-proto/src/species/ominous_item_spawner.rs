//! `minecraft:ominous_item_spawner`: the spawner over a trial's fight; its two deadlines are `ominous_spawner`'s.
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def { name: "minecraft:ominous_item_spawner", caps: Some(&[Cap::Transform, Cap::IgnoresTriggers]), ..Def::EMPTY };
