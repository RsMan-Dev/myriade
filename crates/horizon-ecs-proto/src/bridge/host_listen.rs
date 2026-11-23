//! What the host tells the ears of the world: a vibration, a bell heard by one villager. A random teleport of a mod and the list
//! of the followed ids sit here too (small, no state of their own).
use crate::behaviours::{bell_heard::HEARD_BELL, ender_teleport, vibration_listen::Vibration};
use crate::bridge::{host::EcsWorld, ids::NetIds};
use crate::brain::{set, Brain, MemValue};
use crate::cow::now_of;
use crate::goals::Events;
use bevy_ecs::entity::Entity;
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

impl EcsWorld {
    /// A vibration of the host's game events: one event of the queue; `vibration_listen` ranges the listeners by the index.
    pub fn post_vibration(&mut self, v: Vibration) {
        self.world_mut().resource_mut::<Events>().vibration.push(v);
    }

    /// `Level.gameEvent` for the listeners of P (`event_listen`): the entities of the box around `at` take `event` by their own
    /// parameters (a liked note, a jukebox dance); the spatial index finds them, no entity is walked.
    pub fn allays_hear(&mut self, mw: &MobWorld<'_>, event: &str, at: [f64; 3]) {
        self.lent(mw, |w| crate::behaviours::event_listen::hear(w, event, at));
    }

    /// `BellBlockEntity`: `e` heard a bell now (`HEARD_BELL_TIME` is the time of its own clock, as `bell_heard::ring` writes it).
    /// False for an entity with no brain.
    pub fn heard_bell(&mut self, e: Entity) -> bool {
        let w = self.world_mut();
        if w.get::<Brain>(e).is_none() {
            return false;
        }
        let now = now_of(w, e) as i64;
        set(w, e, &HEARD_BELL, MemValue::Int(now), None);
        true
    }

    /// `LivingEntity.randomTeleport(x, y, z, true, avoid)` for a mod, the target kept inside the world border; false when the
    /// spot is refused or `e` has no body.
    pub fn random_teleport(&mut self, mw: &MobWorld<'_>, e: Entity, at: [f64; 3], avoid: Option<&str>) -> bool {
        let (x, z) = match mw.border {
            Some(b) => {
                let [min_x, min_z, max_x, max_z] = b.bounds(0.0);
                let clamp = |v: f64, lo: f64, hi: f64| if v < lo { lo } else { v.min(hi) };
                (clamp(at[0], min_x, max_x - f64::from(1.0e-5_f32)), clamp(at[2], min_z, max_z - f64::from(1.0e-5_f32)))
            }
            None => (at[0], at[2]),
        };
        let dry = self.world().get::<ender_teleport::Teleporter>(e).is_some_and(|t| t.0.dry_ground);
        let moved = self.lent(mw, |w| ender_teleport::teleport_avoiding(w, e, Vec3::new(x, at[1], z), avoid, dry));
        if moved {
            crate::bridge::view::mark(self.world_mut(), e);
        }
        moved
    }

    /// The network ids of every followed entity (the id table, not a walk of the entities).
    pub fn followed_ids(&self) -> Vec<u32> {
        self.world().get_resource::<NetIds>().map(|t| t.ids().collect()).unwrap_or_default()
    }
}
