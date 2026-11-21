//! The minecart state the mod handles read and write through the bridge (`cart_refs.rs`): the wobble, the flip, the furnace's fuel and push,
//! the TNT's fuse, the hopper's enable and the custom display. Each reads or writes the component that holds it. A write that must move the
//! cart again wakes it (`cart_move::wake`); a fuel that lights a furnace inserts `Lit`, as `fuel_cart` does.
use crate::bridge::{host::{lend, EcsWorld}, view};
use crate::cart_move;
use crate::cow::Pos;
use crate::cart_state::{default_offset, Cart, CartHurt, Display, Enabled, Flipped};
use crate::cart_variants::{self, Lit};
use crate::cart_move::TrackRead;
use crate::minecart::{furnace_max_speed, tnt_primed_by, Fuse, Furnace, Kind, Track, MAX_LAND, MAX_WATER};
use bevy_ecs::entity::Entity;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;
use horizon_core::nbt::Tag;

impl EcsWorld {
    /// The genre of cart `e`; None when it is not a cart.
    pub fn cart_genre(&self, e: Entity) -> Option<Kind> {
        self.world().get::<Kind>(e).copied()
    }
    /// Sets the damage cart `e` holds (`CartHurt`) and wakes it; false when it is not a cart.
    pub fn set_cart_damage(&mut self, e: Entity, damage: f32) -> bool {
        self.set_hurt(e, |h| h.damage = damage)
    }
    /// The wobble ticks of cart `e` (`CartHurt`, 0 before any blow); None when it is not a cart.
    pub fn cart_hurt_time(&self, e: Entity) -> Option<i32> {
        self.world().get::<Cart>(e)?;
        Some(self.world().get::<CartHurt>(e).map_or(0, |h| h.time))
    }
    /// Sets the wobble ticks of cart `e` and wakes it; false when it is not a cart.
    pub fn set_cart_hurt_time(&mut self, e: Entity, ticks: i32) -> bool {
        self.set_hurt(e, |h| h.time = ticks)
    }
    /// The speed cart `e` is held to (`getMaxSpeed`): 0.4 on land, 0.2 in water, halved or three quarters for a furnace; None when it is not a cart.
    pub fn cart_max_speed(&self, e: Entity) -> Option<f64> {
        let w = self.world();
        let kind = *w.get::<Kind>(e)?;
        let in_water = w.get::<Track>(e)?.in_water;
        let base = if in_water { MAX_WATER } else { MAX_LAND };
        Some(if kind == Kind::Furnace { furnace_max_speed(base, in_water) } else { base })
    }
    /// Sets the flipped rotation of cart `e`; false when it is not a cart.
    pub fn set_cart_flipped(&mut self, e: Entity, flipped: bool) -> bool {
        let w = self.world_mut();
        if w.get::<Cart>(e).is_none() {
            return false;
        }
        w.entity_mut(e).insert(Flipped(flipped));
        true
    }
    /// The fuel ticks of furnace cart `e`; None for another cart.
    pub fn cart_fuel(&self, e: Entity) -> Option<i32> {
        self.world().get::<Furnace>(e).map(|f| f.fuel)
    }
    /// Sets the fuel ticks of furnace cart `e`; a fuelled furnace is lit. False for another cart.
    pub fn set_cart_fuel(&mut self, e: Entity, fuel: i32) -> bool {
        let w = self.world_mut();
        let Some(mut f) = w.get_mut::<Furnace>(e) else { return false };
        f.fuel = fuel;
        drop(f);
        if fuel > 0 {
            w.entity_mut(e).insert(Lit);
        }
        view::mark(w, e);
        true
    }
    /// The push of furnace cart `e`; None for another cart.
    pub fn cart_furnace_push(&self, e: Entity) -> Option<[f64; 3]> {
        self.world().get::<Furnace>(e).map(|f| [f.push.x, f.push.y, f.push.z])
    }
    /// Sets the push of furnace cart `e` (the value, not an impulse: `cart_push` adds) and wakes it; false for another cart.
    pub fn set_cart_furnace_push(&mut self, e: Entity, push: [f64; 3]) -> bool {
        let w = self.world_mut();
        let Some(mut f) = w.get_mut::<Furnace>(e) else { return false };
        f.push = Vec3::new(push[0], push[1], push[2]);
        drop(f);
        cart_move::wake(w, e);
        true
    }
    /// Primes TNT cart `e` as `primeFuse(null)` does; false when it is no TNT cart or is primed already.
    pub fn prime_cart_fuse(&mut self, e: Entity) -> bool {
        let w = self.world_mut();
        let Some(fuse) = w.get::<Fuse>(e).map(|f| f.0) else { return false };
        if tnt_primed_by(true, fuse).is_none() {
            return false;
        }
        cart_variants::ignite_cart(w, e, None);
        view::mark(w, e);
        true
    }
    /// `AbstractMinecart.isOnRails`: the cell of cart `e` or the one under it is a rail, read from the world lent by `mw`; None when it is not a cart.
    pub fn cart_on_rails(&self, mw: &MobWorld<'_>, e: Entity) -> Option<bool> {
        let w = self.world();
        w.get::<Cart>(e)?;
        let p = w.get::<Pos>(e)?.0;
        let read = *w.get_resource::<TrackRead>()?;
        Some(cart_move::rail_under(read, lend(mw), p).is_some())
    }
    /// The `Enabled` of hopper cart `e`; None for another cart.
    pub fn cart_hopper_enabled(&self, e: Entity) -> Option<bool> {
        self.world().get::<Enabled>(e).map(|x| x.0)
    }
    /// Sets the `Enabled` of hopper cart `e`, and starts its deadline when it is on; false for another kind of cart.
    pub fn set_cart_hopper_enabled(&mut self, e: Entity, enabled: bool) -> bool {
        let w = self.world_mut();
        if w.get::<Kind>(e).copied() != Some(Kind::Hopper) {
            return false;
        }
        w.entity_mut(e).insert(Enabled(enabled));
        cart_variants::start_cart(w, e);
        true
    }
    /// The custom display of cart `e` (its block state as saved, its offset); the genre's default when never set. None when it is not a cart.
    pub fn cart_display(&self, e: Entity) -> Option<Display> {
        let kind = *self.world().get::<Kind>(e)?;
        Some(self.world().get::<Display>(e).cloned().unwrap_or(Display { block: None, offset: default_offset(kind) }))
    }
    /// Sets the custom display of cart `e`; false when it is not a cart.
    pub fn set_cart_display(&mut self, e: Entity, block: Option<Tag>, offset: i32) -> bool {
        let w = self.world_mut();
        if w.get::<Cart>(e).is_none() {
            return false;
        }
        w.entity_mut(e).insert(Display { block, offset });
        view::mark(w, e);
        true
    }
    /// Changes the `CartHurt` of cart `e` by `f`, the rest kept, and wakes it; false when it is not a cart.
    fn set_hurt(&mut self, e: Entity, f: impl FnOnce(&mut CartHurt)) -> bool {
        let w = self.world_mut();
        if w.get::<Cart>(e).is_none() {
            return false;
        }
        let mut h = w.get::<CartHurt>(e).copied().unwrap_or_default();
        f(&mut h);
        w.entity_mut(e).insert(h);
        cart_move::wake(w, e);
        view::mark(w, e);
        true
    }
}
