use super::*;

impl Game<'_> {
    /// `TextureAtlasStitchedEvent` for each atlas the resources just published, the blocks' then
    /// the GUI's: on the first load before `client_setup`, after a successful reload before
    /// `client_resources_reloaded`.
    pub(in crate::game) fn texture_atlases_stitched(&mut self) {
        for atlas in [mod_api::TextureAtlas::Blocks, mod_api::TextureAtlas::Gui] {
        }
    }
}
