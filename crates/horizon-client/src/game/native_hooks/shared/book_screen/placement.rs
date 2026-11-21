//! The client nonce is tied to the actual shown mod screen, not just its menu type.
use super::*;
impl Game<'_> {
    fn native_book_placement_scope_matches(
        &self,
        _window: i32,
        _nonce: u64,
        _menu: &str,
        _book: &str,
    ) -> bool {
        false
    }
    pub(in crate::game) fn native_book_placement_window_matches(
        &self,
        window: &horizon_client::recipe_book::placement_sync::Window,
    ) -> bool {
        self.native_book_placement_scope_matches(
            window.window,
            window.nonce,
            window.menu.as_str(),
            window.book.as_str(),
        )
    }
    pub(in crate::game) fn native_book_placement_catalog_matches(
        &self,
        identity: &horizon_client::recipe_book::placement_sync::CatalogIdentity,
    ) -> bool {
        self.native_book_placement_scope_matches(
            identity.window,
            identity.nonce,
            identity.menu.as_str(),
            identity.book.as_str(),
        )
    }
    pub(in crate::game) fn native_book_placement_window(
        &self,
    ) -> Option<horizon_client::recipe_book::placement_sync::Window> {
        None
    }
}
