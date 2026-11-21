//! The primed TNT on the level: lit by a player or a mob (its network id), and its network id linked on the spot (`EcsWorld::prime_tnt`).
use super::level::EcsLevel;
use crate::mobs::MobWorld;
use horizon_ecs_proto::targeting::Who;

impl EcsLevel {
    /// `EcsWorld::prime_tnt`: a TNT at `at`, `fuse` ticks long, credited to `owner` (built by the host caller). Its network id, linked by
    /// `alloc`; 0 when the level is inert.
    pub fn prime_tnt(&mut self, mw: &MobWorld<'_>, at: [f64; 3], fuse: i32, owner: Option<Who>, power: f32, alloc: &mut dyn FnMut() -> u32) -> u32 {
        let Some(w) = self.ecs_mut() else { return 0 };
        let e = w.prime_tnt(mw, at, fuse, owner, power);
        w.id_for(e, alloc)
    }
}
