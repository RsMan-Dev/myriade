//! `Zombie.hurtServer`'s reinforcements. The ECS runs them on each hit
//! (`horizon-ecs-proto` `reinforcements::on_hurt`), so the game side has no part left.
use super::Game;
use myriade_engine::mobs::Who;

impl Game<'_> {
    /// `Zombie.hurtServer` after the hit: on Hard, a copy of the zombie appears
    /// out of sight and takes `target`. Done by the ECS's `on_hurt`.
    pub(super) fn call_reinforcements(&mut self, _caller: u32, _target: Who) {}
}
