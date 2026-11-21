//! The cart variants on the bridge: a blow, a use, a fuel item, a fire, a powered rail and the start after a spawn, each run on the world, and
//! the events they queued, read by `drain_cart`. The effects (sounds, drops, the screen, the blast) are the host's, read from `CartEvent`.
use crate::bridge::host::EcsWorld;
use crate::cart_variants::{self, HitResult, UseResult};
use crate::host_out_cart_variants::{CartEvent, CartOut};
use crate::targeting::Who;
use bevy_ecs::entity::Entity;
use horizon_client_lib::mobs::MobWorld;

impl EcsWorld {
    /// A blow on the cart `e` (`attacker` gives the credit, `creative` breaks it with no drop): wounded, or broken with or without its drop.
    pub fn cart_hit(&mut self, mw: &MobWorld<'_>, e: Entity, amount: f32, attacker: Option<Who>, creative: bool) -> HitResult {
        self.lent(mw, |w| cart_variants::hit_cart(w, e, amount, creative, attacker))
    }

    /// A use of the cart `e` by `player` with `hand`: what the host must do next (`UseResult`).
    pub fn cart_use(&mut self, mw: &MobWorld<'_>, e: Entity, player: u32, hand: u8) -> UseResult {
        self.lent(mw, |w| cart_variants::use_cart(w, e, player, hand))
    }

    /// A fuel item given to the furnace cart `e`: true when it took it (the fuel is under the cap), and the furnace is lit.
    pub fn cart_fuel(&mut self, e: Entity) -> bool {
        cart_variants::fuel_cart(self.world_mut(), e)
    }

    /// A fire, or a powered rail (`power` the blast's power when the host knows it), lights the TNT cart `e`.
    pub fn cart_ignite(&mut self, e: Entity, power: Option<f32>) {
        cart_variants::ignite_cart(self.world_mut(), e, power);
    }

    /// A rail under the cart `e` is powered or not: a hopper is enabled by an unpowered one, a TNT cart is lit, a command block runs.
    pub fn cart_activate(&mut self, e: Entity, powered: bool) {
        cart_variants::activate_cart(self.world_mut(), e, powered);
    }

    /// Starts the cart's own deadline after its spawn (a hopper, a generator); the host calls it once for each new cart of those genres.
    pub fn cart_start(&mut self, e: Entity) {
        cart_variants::start_cart(self.world_mut(), e);
    }

}
