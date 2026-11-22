//! The minecart state the mod handles read and write by network id (`cart_refs.rs`), through the bridge's `cart_refs.rs` of the proto. The
//! display's block state is converted here, with the block registry the caller holds. An inert level answers as an empty world.
use super::level::EcsLevel;
use crate::entity_nbt::{state_from_nbt, state_to_nbt};
use horizon_client_lib::mobs::MobWorld;
use horizon_ecs_proto::minecart::Kind;
use horizon_world::registry::{BlockRegistry, BlockStateId};

impl EcsLevel {
    /// Whether cart `id` is on a rail, the world lent by `mw`; None when it is not a cart or the level is inert.
    pub fn cart_on_rails(&self, mw: &MobWorld<'_>, id: u32) -> Option<bool> {
        let e = self.entity_of(id)?;
        self.ecs()?.cart_on_rails(mw, e)
    }
    /// The genre of cart `id`; None when it is not a cart or the level is inert.
    pub fn cart_genre(&self, id: u32) -> Option<Kind> {
        let e = self.entity_of(id)?;
        self.ecs()?.cart_genre(e)
    }
    /// Sets the damage of cart `id`; false when it is not a cart or the level is inert.
    pub fn set_cart_damage(&mut self, id: u32, damage: f32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_cart_damage(e, damage))
    }
    /// The wobble ticks of cart `id`; None when it is not a cart or the level is inert.
    pub fn cart_hurt_time(&self, id: u32) -> Option<i32> {
        let e = self.entity_of(id)?;
        self.ecs()?.cart_hurt_time(e)
    }
    /// The wobble direction of cart `id`; None when it is not a cart or the level is inert.
    pub fn cart_hurt_dir(&self, id: u32) -> Option<i32> {
        let e = self.entity_of(id)?;
        self.ecs()?.cart_hurt_dir(e)
    }
    /// Sets the wobble ticks of cart `id`; false when it is not a cart or the level is inert.
    pub fn set_cart_hurt_time(&mut self, id: u32, ticks: i32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_cart_hurt_time(e, ticks))
    }
    /// The speed cart `id` is held to; None when it is not a cart or the level is inert.
    pub fn cart_max_speed(&self, id: u32) -> Option<f64> {
        let e = self.entity_of(id)?;
        self.ecs()?.cart_max_speed(e)
    }
    /// Sets the flipped rotation of cart `id`; false when it is not a cart or the level is inert.
    pub fn set_cart_flipped(&mut self, id: u32, flipped: bool) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_cart_flipped(e, flipped))
    }
    /// The fuel ticks of furnace cart `id`; None for another cart or when the level is inert.
    pub fn cart_fuel(&self, id: u32) -> Option<i32> {
        let e = self.entity_of(id)?;
        self.ecs()?.cart_fuel(e)
    }
    /// Sets the fuel ticks of furnace cart `id`; false for another cart or when the level is inert.
    pub fn set_cart_fuel(&mut self, id: u32, fuel: i32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_cart_fuel(e, fuel))
    }
    /// The push of furnace cart `id`; None for another cart or when the level is inert.
    pub fn cart_furnace_push(&self, id: u32) -> Option<[f64; 3]> {
        let e = self.entity_of(id)?;
        self.ecs()?.cart_furnace_push(e)
    }
    /// Sets the push of furnace cart `id`; false for another cart or when the level is inert.
    pub fn set_cart_furnace_push(&mut self, id: u32, push: [f64; 3]) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_cart_furnace_push(e, push))
    }
    /// Primes TNT cart `id`; false when it is no TNT cart, is primed already, or the level is inert.
    pub fn prime_cart_fuse(&mut self, id: u32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.prime_cart_fuse(e))
    }
    /// The `Enabled` of hopper cart `id`; None for another cart or when the level is inert.
    pub fn cart_hopper_enabled(&self, id: u32) -> Option<bool> {
        let e = self.entity_of(id)?;
        self.ecs()?.cart_hopper_enabled(e)
    }
    /// Sets the `Enabled` of hopper cart `id`; false for another kind of cart or when the level is inert.
    pub fn set_cart_hopper_enabled(&mut self, id: u32, enabled: bool) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_cart_hopper_enabled(e, enabled))
    }
    /// The custom display of cart `id`: its block state (None for the cart's own) and its offset; None when it is not a cart or the level is inert.
    pub fn cart_display(&self, registry: &BlockRegistry, id: u32) -> Option<(Option<BlockStateId>, i32)> {
        let e = self.entity_of(id)?;
        let d = self.ecs()?.cart_display(e)?;
        Some((d.block.as_ref().and_then(|t| state_from_nbt(registry, t)), d.offset))
    }
    /// Sets the custom display of cart `id`; false when it is not a cart or the level is inert.
    pub fn set_cart_display(&mut self, registry: &BlockRegistry, id: u32, state: Option<BlockStateId>, offset: i32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let block = state.map(|s| state_to_nbt(registry, s));
        self.ecs_mut().is_some_and(|w| w.set_cart_display(e, block, offset))
    }
}
