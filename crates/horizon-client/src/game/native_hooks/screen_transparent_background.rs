use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
}

/// Helpers for native mods, kept as stable as the hook points.
// Native mods use these; nothing in Horizon does.
#[allow(dead_code)]
impl Game<'_> {
    /// The user's configured menu background blur radius (`menuBackgroundBlurriness` in options.txt,
    /// usually 0.0 to 10.0; default 5.0). 0.0 means blur is turned off. API @API:screen_transparent_background@.
    pub(in crate::game) fn menu_blur_radius(&self) -> f32 {
        self.settings.menu_blur as f32
    }
}
