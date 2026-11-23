//! The fireworks through the bridge: a rocket launched at a place by the host, a glider's hold on one, and the fireworks' events the
//! host takes after a step. The rocket's flight and its end are the fireworks' own (`firework`, `firework_burst`).
use crate::bridge::host::EcsWorld;
use crate::firework::{launch, Shot};
use crate::firework_burst::{FireworkStar, Stars};
use crate::targeting::Who;
use bevy_ecs::entity::Entity;
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

impl EcsWorld {
    /// `FireworkRocketEntity`'s constructor at `at`, with its stars, its flight, its owner, and its holder when it glides (`carrier`, a
    /// player). Shot at an angle (`angled`), it keeps `velocity`; else it draws its own.
    #[allow(clippy::too_many_arguments)]
    pub fn launch_rocket(&mut self, mw: &MobWorld<'_>, at: [f64; 3], velocity: [f64; 3], angled: bool, explosions: Vec<FireworkStar>, flight: u8, owner: Option<Who>, carrier: Option<Who>) -> Entity {
        let shot = Shot {
            at: Vec3::new(at[0], at[1], at[2]),
            owner,
            flight,
            explosions: explosions.len() as u8,
            silent: false,
            carrier: match carrier {
                Some(Who::Player(id)) => Some(id),
                _ => None,
            },
            angled: angled.then(|| Vec3::new(velocity[0], velocity[1], velocity[2])),
        };
        self.lent(mw, |w| {
            let e = launch(w, shot);
            w.entity_mut(e).insert(Stars(explosions));
            e
        })
    }

}
