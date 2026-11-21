//! The dragon fireball's burst: its explosion sound. The particles are the clients' own.
use crate::game::Game;
use horizon_core::position::BlockPos;
use horizon_core::random::RandomSource;

impl Game<'_> {
    pub(in crate::game) fn dragon_fireball_burst(&mut self, pos: BlockPos, data: i32) {
        if data == 1 {
            let at = [pos.x, pos.y, pos.z].map(|v| f64::from(v) + 0.5);
            let pitch = self.world.random.next_float() * 0.1 + 0.9;
            self.audio
                .play("minecraft:entity.dragon_fireball.explode", Some(at), 1.0, pitch);
        }
    }
}
