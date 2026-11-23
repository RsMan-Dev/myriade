//! Saved profession identity is independent of the active registry's slot order.
use crate::{
    mobs::{Species, villager::ProfessionState},
    save::text,
};
use horizon_core::nbt::Tag;


impl super::ResolvedProfessions {
}

fn string(value: &str) -> Tag {
    Tag::String(value.into())
}
