//! Item particles: the options of an item's particle, and the eating draws that spawn them.
use super::Game;
use horizon_client::{items::ItemStack, particles::{self, ParticleOptions}};

impl Game<'_> {
    /// `ItemParticleOption(ITEM, stack)`: the layer sprites come from the item's model, which
    /// the server does not hold, so the option carries no layers.
    pub(super) fn item_particle(&self, _stack: &ItemStack) -> ParticleOptions {
        ParticleOptions::item(Vec::new())
    }
    /// `LivingEntity.spawnItemParticles(stack, count)` for the local player: the particles are
    /// client-only, so only their draws from the world random remain.
    pub(super) fn eat_particles(&mut self, _stack: &ItemStack, count: u32) {
        let Some(p) = self.player.as_ref().map(|p| p.position) else {
            return;
        };
        let eye_y = self.eye_position().map_or(p.y, |e| e[1]);
        particles::item_use_particles(
            &mut self.world.random,
            self.pitch,
            self.yaw,
            [p.x, p.y, p.z],
            eye_y,
            count,
            |_, _| {},
        );
    }
}
