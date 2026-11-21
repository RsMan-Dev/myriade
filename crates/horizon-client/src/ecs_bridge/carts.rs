//! The carts on the server's side of the bridge: a cart spawned by its genre's species name, and the network id it is given. The
//! cart's state and its genre live in the ECS world (`horizon_ecs_proto::cart_state`, `bridge::carts`).
use super::level::EcsLevel;
use crate::mobs::MobWorld;

impl EcsLevel {
    /// A cart of the genre named `kind` at `at`, facing `yaw`, with the network id `alloc` gives it; None when the level is inert or
    /// `kind` names no genre.
    pub fn spawn_cart_id(&mut self, mw: &MobWorld<'_>, kind: &str, at: [f64; 3], yaw: f32, alloc: &mut dyn FnMut() -> u32) -> Option<u32> {
        let e = self.ecs_mut()?.spawn_cart(mw, kind, at, yaw)?;
        Some(self.id_for(e, alloc))
    }
    /// `EcsWorld::cart_kind`: the species name of the cart `id`'s genre, None when it is not a cart.
    pub fn cart_kind(&self, id: u32) -> Option<&'static str> {
        let e = self.entity_of(id)?;
        self.ecs()?.cart_kind(e)
    }
}
