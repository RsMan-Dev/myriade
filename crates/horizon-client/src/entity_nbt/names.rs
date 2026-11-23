//! A mob's display name: the styled text when there is one, else the plain name.
use super::*;

impl Mob {
    /// `Entity.getCustomName`: the whole text, style included, else the plain name.
    pub fn display_custom_name(&self) -> Option<crate::text::Text> {
        let plain = || self.custom_name.as_deref().map(crate::text::Text::literal);
        self.custom_name_text.clone().or_else(plain)
    }
}
