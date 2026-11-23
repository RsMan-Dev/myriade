//! The carts the host asks of its ECS world: a cart spawned by its species name, and the species name of a cart's genre. The state is the
//! cart's components (`cart_state`); the rails and the physics stay with the host until the rail lot (57).
use crate::bridge::{host::EcsWorld, view};
use crate::cart_state::{self, Cart, name_of};
use crate::minecart::Kind;
use crate::zone;
use bevy_ecs::entity::Entity;
use myriade_engine_lib::mobs::MobWorld;

impl EcsWorld {
    /// A cart of the genre named `kind` at `at`, facing `yaw`, armed and indexed for the tracker; None when `kind` names no genre.
    pub fn spawn_cart(&mut self, mw: &MobWorld<'_>, kind: &str, at: [f64; 3], yaw: f32) -> Option<Entity> {
        let genre = crate::species::minecart::GENRES.iter().find(|(d, _)| d.name == kind)?.1;
        Some(self.lent(mw, |w| {
            let e = cart_state::spawn(w, genre, at, yaw);
            zone::admit(w, &[e]);
            view::mark(w, e);
            e
        }))
    }

    /// Every awake cart of the level, of every genre, read from the archetypes that hold `Cart`: the cost is the carts.
    pub fn cart_entities(&self) -> Vec<Entity> {
        crate::bridge::marked_entities::<Cart>(self.world())
    }

    /// The species name of a cart's genre; None when `e` is no cart.
    pub fn cart_kind(&self, e: Entity) -> Option<&'static str> {
        let w = self.world();
        w.get::<Cart>(e)?;
        w.get::<Kind>(e).map(|k| name_of(*k))
    }
}
