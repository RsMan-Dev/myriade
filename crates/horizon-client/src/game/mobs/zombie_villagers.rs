//! Zombie villagers in the world: zombies turning the villagers they kill
//! into zombie villagers (`Zombie.killedEntity`), the golden apple that starts
//! a cure (`ZombieVillager.mobInteract`), and the cured villager that
//! remembers who saved it (`ZombieVillager.finishConversion`).
use super::Game;

impl Game<'_> {
    /// `Zombie.killedEntity`. RESTE-SANS-ECS: the conversion of a killed
    /// villager has no ECS hook yet (`conversion::convert_or_ask` is not called
    /// from a zombie's kill).
    pub(super) fn zombie_killed(&mut self, _killer: u32, _victim: u32) {}

    /// `ZombieVillager.mobInteract`: a golden apple on a zombie villager is used.
    /// RESTE-SANS-ECS: the weakness check and the cure timer have no ECS hook
    /// yet, so the apple is not consumed. Returns whether the click was used.
    pub(super) fn interact_zombie_villager(&mut self, id: u32, item: &str) -> bool {
        self.ecs.species(id) == Some("minecraft:zombie_villager") && item == "minecraft:golden_apple"
    }

    /// `ZombieVillager.finishConversion`. RESTE-SANS-ECS: the cure, the
    /// villager's data copy and the cured-villager advancement have no ECS hook yet.
    pub(super) fn cure_zombie_villager(&mut self, _id: u32) {}
}
