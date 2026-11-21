use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// The milliseconds a client tick lasts (`Minecraft.getTickTargetMillis`): `millis` comes
    /// in as the clock's, and a mod may set another.
    pub(in crate::game) fn native_tick_target_millis(&self, millis: &mut f32) {
        self.fire_tick_target_millis(millis);
    }
}
