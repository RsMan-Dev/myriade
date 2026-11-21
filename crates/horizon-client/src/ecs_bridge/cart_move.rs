//! The minecarts on rails by network id (lot 57): the host's track reads, the push of a cart and its velocity, through the bridge's
//! `EcsWorld` (bridge/cart_move.rs). An inert level answers as an empty world.
use super::level::EcsLevel;
use horizon_ecs_proto::cart_move::TrackRead;

impl EcsLevel {
    /// Lends the host's track reads to the world (once, when the level opens).
    pub fn set_track_read(&mut self, read: TrackRead) {
        if let Some(b) = self.ecs_mut() {
            b.set_track_read(read);
        }
    }
    /// Adds `impulse` to the velocity of the cart `id` and wakes it (`EcsWorld::cart_push`).
    pub fn cart_push(&mut self, id: u32, impulse: [f64; 3]) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(b) = self.ecs_mut() {
            b.cart_push(e, impulse);
        }
    }
    /// A rail at `pos` changed or got its power: the resting carts near it wake (`EcsWorld::rail_changed`).
    pub fn rail_changed(&mut self, pos: [i32; 3]) {
        if let Some(b) = self.ecs_mut() {
            b.rail_changed(pos);
        }
    }
    /// The velocity of the cart `id`, None when it is not a cart.
    pub fn cart_velocity(&self, id: u32) -> Option<[f64; 3]> {
        self.entity_of(id).zip(self.ecs()).and_then(|(e, b)| b.cart_velocity(e))
    }
}
