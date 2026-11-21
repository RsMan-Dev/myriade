use super::*;

impl Game<'_> {
    /// Collect world-preset editor registrations once at client startup.
    pub(in crate::game) fn native_register_preset_editors(
        context: &mod_api::RegistrationContext,
        editors: &mut Vec<mod_api::PresetEditor>,
    ) {
        Self::fire_register_preset_editors(context, editors);
    }

    pub(in crate::game) fn preset_editor(&self) -> Option<mod_api::PresetEditor> {
        self.preset_editor_title()?.preset_editor().cloned()
    }

    pub(in crate::game) fn preset_editor_dimensions(&self) -> Option<mod_api::Compound> {
        let horizon_core::nbt::Tag::Compound(dimensions) =
            self.preset_editor_title()?.preset_editor_dimensions()?
        else {
            return None;
        };
        Some(dimensions)
    }

    pub(in crate::game) fn set_preset_editor_dimensions(
        &self,
        dimensions: mod_api::Compound,
    ) -> bool {
        self.preset_editor_title().is_some_and(|title| {
            title.set_preset_editor_dimensions(horizon_core::nbt::Tag::Compound(dimensions))
        })
    }
}
