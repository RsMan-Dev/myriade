//! The fireworks' events of the ECS (`FireworkEvent`), applied by the host: a rocket's launch sound, a burst's particles, a creature
//! hurt by a rocket, a player hit by its blast. The flight and the end of a rocket are P's; a rocket gone needs nothing here.
use crate::game::Game;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::host_out_firework::FireworkEvent;

impl Game<'_> {
    /// Takes the fireworks' events of the level since the last call and applies them in order.
    pub(in crate::game) fn apply_firework_events(&mut self, evs: impl IntoIterator<Item = FireworkEvent>) {
        for ev in evs {
            match ev {
                FireworkEvent::Launched { at } => {
                    self.audio.play("minecraft:entity.firework_rocket.launch", Some(at), 3.0, 1.0);
                }
                FireworkEvent::Burst { at, velocity, sound, stars } => {
                    let camera = self.eye_position().unwrap_or(at);
                    self.particles.fireworks(at, velocity, stars, camera, sound);
                }
                // Gap: `ecs_hurt` takes no damage type (`kind`): the damage is generic.
                FireworkEvent::Hurt { victim, damage, cause, .. } => {
                    let id = self.ecs_id_of(victim);
                    let attacker = self.host_who_opt(cause);
                    self.ecs_hurt_by(id, damage, attacker);
                }
                FireworkEvent::PlayerBlast { player, damage, at, .. } => {
                    let from = Vec3::new(at[0], at[1], at[2]);
                    self.with_player(player, |g| g.rocket_hurt_player(from, damage));
                }
                FireworkEvent::Expired { .. } => {}
            }
        }
    }
}
