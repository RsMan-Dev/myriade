//! `minecraft:item`: a dropped stack. Its flight, pickup and ageing are `item`'s; the species declares only that the origin saves it.
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def { sound_source: super::SoundCategory::Ambient, name: "minecraft:item", caps: Some(&[Cap::Transform]), ..Def::EMPTY };
