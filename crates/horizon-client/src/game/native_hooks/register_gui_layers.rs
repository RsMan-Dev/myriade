use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// The HUD layers a mod adds among vanilla's (NeoForge's `RegisterGuiLayersEvent`): push a
    /// `GuiLayer` per layer, its id `<mod id>:<path>` and where it goes. Asked once at start,
    /// on the client.
    pub(in crate::game) fn native_register_gui_layers(
        _context: &mod_api::RegistrationContext,
        _layers: &mut Vec<GuiLayer>,
    ) {}
}
