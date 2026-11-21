use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// The hook `screen_exclusion` asks: a mod adds the rectangles it
    /// draws over `screen` (a side panel, a button column) to `areas`, in GUI
    /// pixels, `[x, y, width, height]`. NeoForge has no counterpart in the
    /// game; recipe viewers ask for it through their own API
    /// (`EmiExclusionArea`, JEI's `IGuiContainerHandler.getGuiExtraAreas`).
    /// Client-side.
    pub(in crate::game) fn native_screen_exclusion_areas(
        &self,
        screen: &ScreenView<'_>,
        areas: &mut Vec<[f32; 4]>,
    ) {
        self.fire_screen_exclusion_areas(screen, areas);
    }
}

/// Helpers for native mods, kept as stable as the hook points.
// Native mods use these; nothing in Horizon does.
#[allow(dead_code)]
impl Game<'_> {
}
