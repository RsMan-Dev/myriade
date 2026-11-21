//! The thrown tridents of the host's world: a throw (from the host's own throw, with its owner, pickup mode and Channeling), a pickup the
//! host asks for a player (the flight's own pickup path), the hit bonus the host computes, and the events since the last drain. The free
//! functions take the world itself.
use crate::bridge::host::EcsWorld;
use crate::host_out_trident::TridentEvent;
use crate::targeting::Who;
use crate::trident::{self, BlowBonus, HitBonus, Pickup, Trident};
use crate::trident_hit;
use bevy_ecs::{entity::Entity, world::World};
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

/// A trident thrown from `at` with `velocity`, owned by `owner`, with its Loyalty level, its pickup mode and its Channeling.
pub fn spawn_thrown(w: &mut World, at: [f64; 3], velocity: [f64; 3], owner: Option<Who>, loyalty: u8, channeling: bool, pickup: Pickup, stack: ItemStack) -> Entity {
    let e = trident::throw(w, stack, Vec3::new(at[0], at[1], at[2]), Vec3::new(velocity[0], velocity[1], velocity[2]), owner, loyalty, pickup);
    if let Some(mut t) = w.get_mut::<Trident>(e) {
        t.channeling = channeling;
    }
    e
}

/// The stack a player takes from the trident `e`, when the pickup rule lets it (`trident_hit::pickup`).
pub fn pickup(w: &mut World, e: Entity, player: u32) -> Option<ItemStack> {
    trident_hit::pickup(w, e, player)
}

/// Sets the enchantment bonus of a blow, the host's table (see `BlowBonus`).
pub fn set_hit_bonus(w: &mut World, f: BlowBonus) {
    w.insert_resource(HitBonus(f));
}

/// The events of the tridents since the last call, in order.
pub fn drain_events(w: &mut World) -> Vec<TridentEvent> {
    trident_hit::drain_events(w)
}

impl EcsWorld {
    /// Throws a trident from `at` (the host's throw); the mob world is lent for the call.
    pub fn throw_trident(&mut self, mw: &MobWorld<'_>, at: [f64; 3], velocity: [f64; 3], owner: Option<Who>, loyalty: u8, channeling: bool, pickup: Pickup, stack: ItemStack) -> Entity {
        self.lent(mw, |w| spawn_thrown(w, at, velocity, owner, loyalty, channeling, pickup, stack))
    }

    /// The host asks for the pickup of `e` by `player`: the stack it gives, or None when the rule refuses (a taken trident: None).
    pub fn trident_pickup(&mut self, e: Entity, player: u32) -> Option<ItemStack> {
        pickup(self.world_mut(), e, player)
    }

    /// Sets the enchantment bonus of a blow (see `set_hit_bonus`).
    pub fn set_trident_hit_bonus(&mut self, f: BlowBonus) {
        set_hit_bonus(self.world_mut(), f);
    }

}
