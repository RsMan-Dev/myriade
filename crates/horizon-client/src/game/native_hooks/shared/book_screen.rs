use super::*;
#[path = "book_screen/open.rs"]
mod open;
#[path = "book_screen/placement.rs"]
mod placement;

impl Game<'_> {
    /// Whether a mod's screen places the current container and player slots.
    pub(in crate::game) fn native_screen_shown(&self) -> bool {
        false
    }
    /// The open mod screen's height.
    pub(in crate::game) fn native_screen_height(&self) -> Option<f32> {
        None
    }
    pub(in crate::game) fn native_screen_menu(&self) -> Option<&str> {
        None
    }
    pub(in crate::game) fn native_screen_toggle(&self) -> Option<[f32; 2]> {
        None
    }
    pub(in crate::game) fn native_screen_block(&self) -> Option<&str> {
        None
    }
    /// The explicitly assigned book; declarations bind it to the actual menu.
    pub(in crate::game) fn native_screen_book(&self) -> Option<&mod_api::RecipeBookTypeKey> {
        None
    }
}
