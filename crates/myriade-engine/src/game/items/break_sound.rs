//! The break sound of a stack that breaks in the hand, played where the item broke.
use crate::game::Game;
use myriade_engine::items::ItemStack;
use horizon_core::random::RandomSource;

impl Game<'_> {
    /// The item's `minecraft:break_sound` is read from its model, which the server does not hold:
    /// the default `entity.item.break` plays for every stack.
    pub(in crate::game) fn break_sound_at(&mut self, _stack: &ItemStack, at: [f64; 3]) {
        let pitch = 0.8 + self.world.random.next_float() * 0.4;
        self.audio.play("minecraft:entity.item.break", Some(at), 0.8, pitch);
    }
}
