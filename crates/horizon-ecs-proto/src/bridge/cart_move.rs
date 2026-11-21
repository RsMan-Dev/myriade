//! The minecarts on rails through the bridge (`cart_move.rs`): the track reads the host lends once, the push of a cart (it wakes), its
//! velocity, and the events the carts wrote since the last drain.
use crate::bridge::host::EcsWorld;
use crate::cart_move::{self, TrackRead};
use bevy_ecs::entity::Entity;

impl EcsWorld {
    /// The host's reads of the track, kept in the world for the carts' ticks.
    pub fn set_track_read(&mut self, read: TrackRead) {
        self.world_mut().insert_resource(read);
    }
    /// Adds `impulse` to the cart `e`'s velocity and wakes it.
    pub fn cart_push(&mut self, e: Entity, impulse: [f64; 3]) {
        cart_move::push(self.world_mut(), e, impulse);
    }
    /// A rail at `pos` changed or got its power: the resting carts near it wake.
    pub fn rail_changed(&mut self, pos: [i32; 3]) {
        cart_move::rail_changed(self.world_mut(), pos);
    }
    /// The velocity of the cart `e`, None when `e` is not a cart.
    pub fn cart_velocity(&self, e: Entity) -> Option<[f64; 3]> {
        cart_move::velocity(self.world(), e)
    }
}
