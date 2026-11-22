//! Pandas on the game side: the food they find on the ground and pick up
//! (`Mob.aiStep`, `Panda.pickUpItem`). The food tag is the private
//! `Game::panda_eats_from_ground`, which a mod can listen to.
use super::Game;

impl Game<'_> {
    /// `Panda.canPickUpAndEat`'s tag.
    fn panda_eats_from_ground(&self, item: &str) -> bool {
        self.res
            .items()
            .item_tags
            .get("minecraft:panda_eats_from_ground")
            .is_some_and(|tag| tag.contains(item))
    }
}
