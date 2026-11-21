//! The vehicles' pushes through the bridge (`vehicle_push.rs`): the push of an entity, the entities a vehicle touches, whether a push moves an
//! entity, and the bumps the pushes wrote since the last drain.
use crate::bridge::host::EcsWorld;
use crate::rules::push_rules::TOUCH_REACH;
use crate::vehicle_push;
use bevy_ecs::entity::Entity;

impl EcsWorld {
    /// Adds `impulse` to the velocity of `e` and wakes it.
    pub fn push_entity(&mut self, e: Entity, impulse: [f64; 3]) {
        vehicle_push::push(self.world_mut(), e, impulse);
    }
    /// The entities whose box a vehicle touches (the index answers; nothing is scanned).
    pub fn vehicle_contacts(&self, vehicle: Entity) -> Vec<Entity> {
        let mut out = Vec::new();
        vehicle_push::contacts(self.world(), vehicle, TOUCH_REACH, &mut out);
        out
    }
    /// Whether `e` is a boat or a cart (the blast's vehicle pass is P's).
    pub fn is_vehicle(&self, e: Entity) -> bool {
        let w = self.world();
        w.get::<crate::boat_state::BoatMarker>(e).is_some() || w.get::<crate::cart_state::Cart>(e).is_some()
    }
    /// Whether a push moves `e` (`Entity.isPushable`, not carrying riders).
    pub fn push_moves(&self, e: Entity) -> bool {
        vehicle_push::pushable(self.world(), e)
    }
}
