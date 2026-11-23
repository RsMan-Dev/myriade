//! Zombie villagers: the cure of a zombie villager is the ECS's (`give_item::VILLAGER_CURE`, `Curing`);
//! the kill conversion is `conversion::zombie_kill`.
use super::Game;

impl Game<'_> {
    /// Legacy `MobEvent::CureZombieVillager` (old AI path only): the cure is the
    /// ECS's; this copy of its end is not sent by the ECS and does nothing.
    pub(super) fn cure_zombie_villager(&mut self, _id: u32) {}
}
