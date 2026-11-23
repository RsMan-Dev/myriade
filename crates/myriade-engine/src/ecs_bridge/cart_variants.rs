//! The cart variants on the level: a blow, a use, a fuel item, a fire, a rail and a start, by network id. An inert level answers as an
//! empty world (`Ignored`, `Pass`, false, nothing).
use super::level::EcsLevel;
use crate::mobs::MobWorld;
use horizon_ecs_proto::cart_variants::{HitResult, UseResult};
use horizon_ecs_proto::targeting::Who;

/// The credit of a blow on a cart: a player by its id, or a mob by its network id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CartAttacker {
    Player(u32),
    Mob(u32),
}

impl EcsLevel {
    /// `EcsWorld::cart_hit` on the cart `id`; `attacker` is the credit of the blow, when known (a mob unknown to the level gives none).
    pub fn cart_hit(&mut self, mw: &MobWorld<'_>, id: u32, amount: f32, attacker: Option<CartAttacker>, creative: bool) -> HitResult {
        let Some(e) = self.entity_of(id) else { return HitResult::Ignored };
        let attacker = match attacker {
            Some(CartAttacker::Player(p)) => Some(Who::Player(p)),
            Some(CartAttacker::Mob(m)) => self.entity_of(m).map(Who::Entity),
            None => None,
        };
        let Some(w) = self.ecs_mut() else { return HitResult::Ignored };
        w.cart_hit(mw, e, amount, attacker, creative)
    }

    /// `EcsWorld::cart_use` on the cart `id`, by `player` with `hand`.
    pub fn cart_use(&mut self, mw: &MobWorld<'_>, id: u32, player: u32, hand: u8) -> UseResult {
        let Some(e) = self.entity_of(id) else { return UseResult::Pass };
        let Some(w) = self.ecs_mut() else { return UseResult::Pass };
        w.cart_use(mw, e, player, hand)
    }

    /// `EcsWorld::cart_feed` on the furnace cart `id`: false when it is not taken.
    pub fn cart_feed(&mut self, id: u32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.cart_feed(e))
    }

    /// `EcsWorld::cart_ignite` on the cart `id`.
    pub fn cart_ignite(&mut self, id: u32, power: Option<f32>) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(w) = self.ecs_mut() {
            w.cart_ignite(e, power);
        }
    }

    /// `EcsWorld::cart_activate` on the cart `id`.
    pub fn cart_activate(&mut self, id: u32, powered: bool) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(w) = self.ecs_mut() {
            w.cart_activate(e, powered);
        }
    }

    /// `EcsWorld::cart_start` on the cart `id`, once after its spawn.
    pub fn cart_start(&mut self, id: u32) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(w) = self.ecs_mut() {
            w.cart_start(e);
        }
    }

}
