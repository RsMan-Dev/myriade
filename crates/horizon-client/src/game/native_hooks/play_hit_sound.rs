use super::*;

impl Game<'_> {
    pub(in crate::game) fn play_mining_hit_sound(
        &mut self,
        state: BlockStateId,
        pos: [i32; 3],
        _face: [i32; 3],
        unshared: bool,
    ) {
        if unshared {
            return;
        }
        let sounds = self.res.facts().get(state).sounds.clone();
        let at = Some(pos.map(|v| f64::from(v) + 0.5));
        let volume = (sounds.volume + 1.0) / 8.0;
        let pitch = sounds.pitch * 0.5;
        self.audio.play(&sounds.hit, at, volume, pitch);
    }
}

impl Game<'_> {
    pub(in crate::game) fn breaking_block_effects(
        &mut self,
        pos: [i32; 3],
        face: [i32; 3],
        play_sound: bool,
    ) {
        let state = self.world.block(pos[0], pos[1], pos[2]);
        if self.res.facts().is_air(state) {
            return;
        }
        if play_sound {
            self.play_mining_hit_sound(state, pos, face, true);
        }
    }
}
