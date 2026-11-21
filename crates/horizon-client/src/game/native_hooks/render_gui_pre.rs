use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// Before the in-world HUD (`RenderGuiEvent.Pre`): `canceled` hides it as
    /// F1 does.
    pub(in crate::game) fn native_render_gui_pre(&self, canceled: &mut bool) {
        self.fire_render_gui_pre(canceled);
    }
}
