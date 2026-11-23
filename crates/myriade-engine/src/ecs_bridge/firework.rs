//! The fireworks through the level's bridge: a rocket launched by the host, its network id, and the fireworks' events taken after a step.
//! An inert level (the client role) launches nothing and has no events.
use super::level::EcsLevel;
use crate::mobs::MobWorld;
use horizon_ecs_proto::{firework_burst::FireworkStar, targeting::Who};

impl EcsLevel {
    /// Launches a rocket at `at` shot with `velocity` (when `angled`), with its owner and its holder (`Who`, the entities as the level
    /// names them). Returns the rocket's network id (0 when inert).
    #[allow(clippy::too_many_arguments)]
    pub fn launch_rocket(&mut self, mw: &MobWorld<'_>, at: [f64; 3], velocity: [f64; 3], angled: bool, explosions: Vec<FireworkStar>, flight: u8, owner: Option<Who>, carrier: Option<Who>, alloc: &mut dyn FnMut() -> u32) -> u32 {
        let Some(w) = self.ecs_mut() else { return 0 };
        let e = w.launch_rocket(mw, at, velocity, angled, explosions, flight, owner, carrier);
        w.id_for(e, alloc)
    }

}
