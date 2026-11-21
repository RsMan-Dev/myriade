//! The evoker's fangs on the bridge: `fangs.rs` arms their three deadlines and emits the bite; this keeps the yaw the host gives. A fang has
//! no tick.
use crate::fangs;
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

/// The yaw of a fang (`EvokerFangs`' rotation, as the host gives it).
#[derive(Component, Clone, Copy, Debug)]
pub struct Yaw(pub f32);

/// A fang at `at` that bites after `warmup` ticks for `owner` (a player or a creature; `None` when unknown).
pub fn spawn(w: &mut World, at: Vec3, yaw: f32, warmup: i32, owner: Option<Who>) -> Entity {
    let e = fangs::spawn_fang(w, at, warmup, owner);
    w.entity_mut(e).insert(Yaw(yaw));
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fang without an owner is gone by its end (23 ticks after its warm-up), and keeps the yaw it was given.
    #[test]
    fn a_fang_ends_and_keeps_its_yaw() {
        let mut herd = crate::cow::Herd::new(0, 4.0, false, false, 7);
        let e = spawn(&mut herd.world, Vec3::new(0.5, 64.0, 0.5), 90.0, 0, None);
        assert_eq!(herd.world.get::<Yaw>(e).map(|y| y.0), Some(90.0));
        for _ in 0..40 {
            herd.tick();
        }
        assert!(herd.world.get_entity(e).is_err());
    }
}
