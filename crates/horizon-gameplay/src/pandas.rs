//! Pandas on the game side: the food they find on the ground and pick up
//! (`Mob.aiStep`, `Panda.pickUpItem`), the bamboo a player gives them
//! (`Panda.mobInteract`) and the start a sneeze gives the grown pandas about.
use crate::panda_level::PandaLevel;
use myriade_engine::{
    entities::ItemEntity,
    items::ItemStack,
    join_seq::RestoreOrder,
    mobs::{Species, panda},
};
use horizon_core::geometry::Vec3;



