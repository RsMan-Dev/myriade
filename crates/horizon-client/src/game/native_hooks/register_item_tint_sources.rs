use super::*;

impl Game<'_> {
    /// The item tint sources a mod adds (NeoForge's `RegisterColorHandlersEvent.ItemTintSources`):
    /// push each id, `<mod id>:<path>`. Asked once at start on the client, before resources load.
    pub(in crate::game) fn native_register_item_tint_sources(
        _context: &mod_api::RegistrationContext,
        _sources: &mut Vec<String>,
    ) {}
}
