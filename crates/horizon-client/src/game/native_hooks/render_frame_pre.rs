use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    pub(in crate::game) fn frame_partial_ticks(&self) -> (f32, f32) {
        let partial_tick = self.clock.window_fraction();
        let world_partial_tick = if !self.shows_panorama() && self.player.is_some() {
            self.clock.fraction() as f32
        } else {
            partial_tick
        };
        (world_partial_tick, partial_tick)
    }

    pub(in crate::game) fn frame_world_partial_tick(&self) -> f64 {
        self.extras
            .frame_partial_ticks
            .map_or_else(|| self.clock.fraction(), |(world, _)| f64::from(world))
    }

    /// Once a frame, in a world or not, after Horizon's extras and just
    /// before the frame is rendered (`RenderFrameEvent.Pre`).
    pub(in crate::game) fn native_render_frame_pre(
        &mut self,
        elapsed: Duration,
        world_partial_tick: f32,
        partial_tick: f32,
    ) {
        self.fire_render_frame_pre(elapsed, world_partial_tick, partial_tick);
    }
}
