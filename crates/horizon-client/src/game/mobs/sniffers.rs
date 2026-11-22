//! Sniffers on the game side: the snifflet that hatches from an egg block
//! (`SnifferEggBlock.tick`).
use super::Game;

impl Game<'_> {
    /// A snifflet where the egg was.
    pub(super) fn hatch_sniffer(&mut self, pos: [i32; 3]) {
        let at = [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]) + 0.5,
            f64::from(pos[2]) + 0.5,
        ];
        // RESTE-SANS-ECS: the snifflet's baby age and random yaw are not set (no ECS setter).
        self.spawn_mob_raw("minecraft:sniffer", at, true);
    }
}
