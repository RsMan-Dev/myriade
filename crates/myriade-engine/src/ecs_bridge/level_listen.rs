//! The ears and a few writes of a mod on `EcsLevel`: a vibration, a bell heard, a random teleport, the followed ids. An inert level
//! takes nothing.
use super::level::EcsLevel;
use crate::mobs::MobWorld;
use bevy_ecs::entity::Entity;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::behaviours::vibration_listen::Vibration;

impl EcsLevel {
    /// A game event's vibration: posted once, heard by the listeners in range after their delay.
    pub fn post_vibration(&mut self, v: Vibration) {
        if let Some(w) = self.ecs_mut() {
            w.post_vibration(v);
        }
    }

    /// `Level.gameEvent` for the listeners of P (a liked note block, a jukebox dance): `event` at `at`.
    pub fn allays_hear(&mut self, mw: &MobWorld<'_>, event: &str, at: [f64; 3]) {
        if let Some(w) = self.ecs_mut() {
            w.allays_hear(mw, event, at);
        }
    }

    /// `HEARD_BELL_TIME` of the villager `e`, written on its own clock.
    pub fn heard_bell(&mut self, e: Entity) -> bool {
        self.ecs_mut().is_some_and(|w| w.heard_bell(e))
    }

    /// `LivingEntity.randomTeleport` of the creature `id` toward `at`, avoiding the blocks of the tag `avoid`.
    pub fn random_teleport(&mut self, mw: &MobWorld<'_>, id: u32, at: Vec3, avoid: Option<&str>) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.random_teleport(mw, e, [at.x, at.y, at.z], avoid))
    }

    /// The network ids of the followed entities (none for an inert level).
    pub fn followed_ids(&self) -> Vec<u32> {
        self.ecs().map(|w| w.followed_ids()).unwrap_or_default()
    }
}
