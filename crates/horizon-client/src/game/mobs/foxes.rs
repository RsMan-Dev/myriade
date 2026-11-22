//! Foxes and dolphins on the game side: the food test and the eating draws.
use super::Game;
use horizon_core::random::RandomSource;

impl Game<'_> {
    /// The fox's eating draws, in vanilla's order: the particles they place are the clients'.
    pub(in crate::game) fn fox_eat_particles(&mut self, id: u32) {
        let Some(m) = self
            .remote_entities
            .iter_mut()
            .find(|m| m.id == id && m.species() == horizon_client::mobs::Species::Fox)
        else {
            return;
        };
        let item = horizon_client::mobs::finalize::item(m, horizon_client::mobs::finalize::MAINHAND);
        if item.is_none() {
            return;
        }
        for _ in 0..8 {
            let _ = m.random.next_float();
            let _ = m.random.next_float();
        }
    }
}
